//! PlexCloud-API (dieselbe wie Web und Android): Baum lesen, Ordner anlegen, hoch- und herunterladen, Papierkorb.

use crate::auth::Auth;
use serde::Deserialize;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

pub const API_BASE: &str = "https://7hrkm580pb.execute-api.eu-central-1.amazonaws.com";
pub const ROOT: &str = "root";

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteItem {
    pub item_id: String,
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub parent_id: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub content_type: String,
    #[serde(default)]
    pub updated_at: String,
}

impl RemoteItem {
    pub fn folder(&self) -> bool {
        self.kind == "folder"
    }
}

#[derive(Deserialize, Clone, Debug, Default, serde::Serialize)]
pub struct Usage {
    #[serde(default)]
    pub used: u64,
    pub limit: Option<u64>,
    #[serde(default)]
    pub plan: String,
}

#[derive(Deserialize)]
struct Listing {
    #[serde(default)]
    ansicht: String,
    #[serde(default)]
    items: Vec<RemoteItem>,
    #[serde(default)]
    usage: Usage,
}

/// Fortschritt für die Anzeige: Bytes dieser Übertragung
#[derive(Default)]
pub struct Progress {
    pub done: AtomicU64,
    pub total: AtomicU64,
}

pub struct Api {
    pub auth: Arc<Auth>,
    http: reqwest::Client,
    ua: String,
}

#[derive(Debug)]
pub struct ApiError {
    pub status: u16,
    pub message: String,
}
impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.status)
    }
}
impl std::error::Error for ApiError {}

impl Api {
    pub fn new(auth: Arc<Auth>, http: reqwest::Client) -> Api {
        Api { auth, http, ua: format!("PlexCloud-Linux/{}", env!("CARGO_PKG_VERSION")) }
    }

    async fn call(&self, method: reqwest::Method, path: &str, query: &[(&str, &str)], body: Option<serde_json::Value>) -> anyhow::Result<serde_json::Value> {
        for attempt in 0..2 {
            let token = self.auth.id_token(attempt == 1).await?;
            let url = format!("{API_BASE}{}{path}", if path.starts_with("/api/") { "" } else { "/api/plexcloud" });
            let mut r = self.http.request(method.clone(), &url).query(query).bearer_auth(&token).header("User-Agent", &self.ua).header("Accept", "application/json");
            if let Some(b) = &body {
                r = r.json(b);
            }
            let res = r.send().await?;
            let status = res.status().as_u16();
            let text = res.text().await.unwrap_or_default();
            if status == 401 && attempt == 0 {
                continue;
            }
            if status >= 400 {
                let j: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
                let m = j.get("message").or_else(|| j.get("statusMessage")).and_then(|v| v.as_str()).map(String::from).unwrap_or_else(|| match status {
                    401 => "Bitte neu anmelden.".into(),
                    403 => "Keine Berechtigung.".into(),
                    404 => "Nicht gefunden.".into(),
                    429 => "Zu viele Anfragen.".into(),
                    _ => format!("Fehler {status}"),
                });
                return Err(ApiError { status, message: m }.into());
            }
            return Ok(if text.trim().is_empty() { serde_json::Value::Null } else { serde_json::from_str(&text)? });
        }
        unreachable!()
    }

    pub async fn listing(&self, folder: &str) -> anyhow::Result<(Vec<RemoteItem>, Usage)> {
        let l: Listing = serde_json::from_value(self.call(reqwest::Method::GET, "/items", &[("ordner", folder)], None).await?)?;
        Ok((l.items, l.usage))
    }

    /// Alle fertigen Ordner und Dateien. Neuer Server: ein Aufruf (?ansicht=baum), sonst Ordner für Ordner.
    pub async fn tree(&self) -> anyhow::Result<(Vec<RemoteItem>, Usage)> {
        let l: Listing = serde_json::from_value(self.call(reqwest::Method::GET, "/items", &[("ansicht", "baum")], None).await?)?;
        if l.ansicht == "baum" {
            return Ok((l.items, l.usage));
        }
        let usage = l.usage;
        let mut all = Vec::new();
        let mut queue = vec![ROOT.to_string()];
        while let Some(f) = queue.pop() {
            let (items, _) = self.listing(&f).await?;
            for i in items {
                if i.folder() {
                    queue.push(i.item_id.clone());
                }
                all.push(RemoteItem { parent_id: if i.parent_id.is_empty() { f.clone() } else { i.parent_id.clone() }, ..i });
            }
        }
        Ok((all, usage))
    }

    pub async fn create_folder(&self, name: &str, parent: &str) -> anyhow::Result<RemoteItem> {
        let j = self.call(reqwest::Method::POST, "/folders", &[], Some(serde_json::json!({ "name": name, "parentId": parent }))).await?;
        Ok(serde_json::from_value(j["item"].clone())?)
    }

    pub async fn trash(&self, item_id: &str) -> anyhow::Result<()> {
        self.call(reqwest::Method::DELETE, &format!("/items/{}", enc(item_id)), &[], None).await.map(|_| ())
    }

    pub async fn move_to(&self, item_id: &str, parent: &str) -> anyhow::Result<()> {
        self.call(reqwest::Method::PATCH, &format!("/items/{}", enc(item_id)), &[], Some(serde_json::json!({ "parentId": parent }))).await.map(|_| ())
    }

    /// Datei hochladen (neu in parent oder als neue Version von replace). Gibt die itemId zurück.
    pub async fn upload(&self, path: &Path, name: &str, parent: &str, replace: Option<&str>, limit_kbps: u64, prog: &Arc<Progress>) -> anyhow::Result<String> {
        let size = tokio::fs::metadata(path).await?.len();
        let ct = mime_guess::from_path(path).first_or_octet_stream().essence_str().to_string();
        let body = match replace {
            Some(id) => serde_json::json!({ "replaceId": id, "size": size, "contentType": ct }),
            None => serde_json::json!({ "name": name, "size": size, "contentType": ct, "parentId": parent }),
        };
        let start = self.call(reqwest::Method::POST, "/uploads", &[], Some(body)).await?;
        let id = start["itemId"].as_str().ok_or_else(|| anyhow::anyhow!("Keine itemId"))?.to_string();
        prog.total.store(size, Ordering::Relaxed);
        prog.done.store(0, Ordering::Relaxed);
        if start["multipart"].as_bool() == Some(true) {
            let part_size = start["partSize"].as_u64().unwrap_or(0).max(1);
            let parts = start["parts"].as_u64().unwrap_or(1);
            let mut n = 1;
            while n <= parts {
                let batch: Vec<u64> = (n..=parts.min(n + 19)).collect();
                let j = self.call(reqwest::Method::POST, &format!("/items/{}/teile", enc(&id)), &[], Some(serde_json::json!({ "teile": batch }))).await?;
                for p in &batch {
                    let url = j["urls"][p.to_string()].as_str().ok_or_else(|| anyhow::anyhow!("Kein Link für Teil {p}"))?;
                    let off = (p - 1) * part_size;
                    self.put(url, path, off, part_size.min(size - off), None, limit_kbps, prog).await?;
                }
                n = batch.last().unwrap() + 1;
            }
        } else {
            let url = start["url"].as_str().ok_or_else(|| anyhow::anyhow!("Kein Upload-Link"))?;
            self.put(url, path, 0, size, Some(&ct), limit_kbps, prog).await?;
        }
        self.call(reqwest::Method::POST, &format!("/items/{}/complete", enc(&id)), &[], Some(serde_json::json!({}))).await?;
        Ok(replace.map(String::from).unwrap_or(id))
    }

    /// Ein Stück der Datei nach S3, gestreamt; bei Upload-Grenze gedrosselt
    async fn put(&self, url: &str, path: &Path, off: u64, len: u64, ct: Option<&str>, limit_kbps: u64, prog: &Arc<Progress>) -> anyhow::Result<()> {
        let mut f = tokio::fs::File::open(path).await?;
        f.seek(std::io::SeekFrom::Start(off)).await?;
        let reader = f.take(len);
        let started = std::time::Instant::now();
        let base = prog.done.load(Ordering::Relaxed);
        let p = prog.clone();
        let stream = futures_util::stream::unfold((reader, 0u64, p), move |(mut r, sent, p)| async move {
            let mut buf = vec![0u8; 256 * 1024];
            match r.read(&mut buf).await {
                Ok(0) => None,
                Ok(n) => {
                    buf.truncate(n);
                    let sent = sent + n as u64;
                    p.done.store(base + sent, Ordering::Relaxed);
                    if limit_kbps > 0 {
                        let should = std::time::Duration::from_secs_f64(sent as f64 / (limit_kbps as f64 * 1024.0));
                        let el = started.elapsed();
                        if should > el {
                            tokio::time::sleep(should - el).await;
                        }
                    }
                    Some((Ok::<_, std::io::Error>(buf), (r, sent, p)))
                }
                Err(e) => Some((Err(e), (r, sent, p))),
            }
        });
        let mut req = self.http.put(url).header("Content-Length", len).body(reqwest::Body::wrap_stream(stream));
        if let Some(ct) = ct {
            req = req.header("Content-Type", ct);
        }
        let res = req.send().await?;
        if !res.status().is_success() {
            anyhow::bail!("Speicher hat den Upload abgelehnt ({})", res.status().as_u16());
        }
        Ok(())
    }

    /// Datei herunterladen: erst in eine versteckte Teildatei, dann umbenennen (nie halbe Dateien im Ordner)
    pub async fn download(&self, item_id: &str, dest: &Path, prog: &Progress) -> anyhow::Result<()> {
        let j = self.call(reqwest::Method::GET, &format!("/items/{}/download", enc(item_id)), &[], None).await?;
        let url = j["url"].as_str().ok_or_else(|| anyhow::anyhow!("Kein Download-Link"))?;
        let mut res = self.http.get(url).send().await?;
        if !res.status().is_success() {
            anyhow::bail!("Download fehlgeschlagen ({})", res.status().as_u16());
        }
        prog.total.store(res.content_length().unwrap_or(0), Ordering::Relaxed);
        prog.done.store(0, Ordering::Relaxed);
        if let Some(p) = dest.parent() {
            tokio::fs::create_dir_all(p).await?;
        }
        let tmp = dest.with_file_name(format!(".{}.plexcloud-part", dest.file_name().and_then(|n| n.to_str()).unwrap_or("datei")));
        let mut out = tokio::fs::File::create(&tmp).await?;
        let mut done = 0u64;
        while let Some(chunk) = res.chunk().await? {
            out.write_all(&chunk).await?;
            done += chunk.len() as u64;
            prog.done.store(done, Ordering::Relaxed);
        }
        out.sync_all().await?;
        drop(out);
        tokio::fs::rename(&tmp, dest).await?;
        Ok(())
    }
}

fn enc(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}
