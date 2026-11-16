//! Anmeldung bei Plexora (Cognito, PKCE) über den Browser mit Rücksprung auf http://localhost:53682/callback.

use crate::config::{dir, write_private};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;

pub const COGNITO_DOMAIN: &str = "eu-central-1lm7sn6lvc.auth.eu-central-1.amazoncognito.com";
pub const CLIENT_ID: &str = "1aa9chqqkgr9dp232cgpa4nanb";
pub const PORT: u16 = 53682;
pub const REDIRECT: &str = "http://localhost:53682/callback";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Tokens {
    pub id_token: String,
    pub refresh_token: String,
    /// Sekunden seit 1970
    pub expires_at: u64,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Profile {
    pub name: String,
    pub email: String,
    pub picture: Option<String>,
    pub provider: String,
}

pub struct Auth {
    tokens: Mutex<Option<Tokens>>,
    http: reqwest::Client,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn random(n: usize) -> String {
    let mut b = vec![0u8; n];
    rand::thread_rng().fill_bytes(&mut b);
    B64.encode(b)
}

pub fn challenge(verifier: &str) -> String {
    B64.encode(Sha256::digest(verifier.as_bytes()))
}

/// Inhalt (Claims) eines JWT ohne Prüfung lesen: nur für Name und Bild, die Prüfung macht der Server
pub fn claims(jwt: &str) -> serde_json::Value {
    jwt.split('.').nth(1).and_then(|p| B64.decode(p.trim_end_matches('=')).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(serde_json::Value::Null)
}

pub fn profile_from(id_token: &str) -> Profile {
    let c = claims(id_token);
    let s = |k: &str| c.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let name = [s("given_name"), s("family_name")].iter().filter(|x| !x.is_empty()).cloned().collect::<Vec<_>>().join(" ");
    let google = c.get("identities").map(|v| v.to_string().contains("Google")).unwrap_or(false);
    Profile {
        name: if name.is_empty() { s("name") } else { name },
        email: s("email"),
        picture: Some(s("picture")).filter(|p| p.starts_with("https://")).map(|p| p.replace("=s96-c", "=s256-c")),
        provider: if google { "Google".into() } else { "E-Mail".into() },
    }
}

/// Wert eines Query-Parameters aus "/callback?code=…&state=…"
fn query(path: &str, key: &str) -> Option<String> {
    let u = url::Url::parse(&format!("http://localhost{path}")).ok()?;
    let v = u.query_pairs().find(|(k, _)| k == key).map(|(_, v)| v.into_owned());
    v
}

impl Auth {
    pub fn new(http: reqwest::Client) -> Auth {
        let tokens = std::fs::read_to_string(dir().join("tokens.json")).ok().and_then(|s| serde_json::from_str(&s).ok());
        Auth { tokens: Mutex::new(tokens), http }
    }

    pub async fn signed_in(&self) -> bool {
        self.tokens.lock().await.is_some()
    }

    pub async fn profile(&self) -> Option<Profile> {
        self.tokens.lock().await.as_ref().map(|t| profile_from(&t.id_token))
    }

    /// Browser öffnen, auf den Rücksprung warten, Code gegen Tokens tauschen
    pub async fn login(&self, provider: &str, open: impl FnOnce(String)) -> anyhow::Result<Profile> {
        let verifier = random(48);
        let state = random(16);
        // localhost kann ::1 oder 127.0.0.1 sein: auf beiden lauschen
        let v4 = tokio::net::TcpListener::bind(("127.0.0.1", PORT)).await.map_err(|e| anyhow::anyhow!("Port {PORT} ist belegt ({e}). Läuft PlexCloud zweimal?"))?;
        let v6 = tokio::net::TcpListener::bind(("::1", PORT)).await.ok();
        let mut url = url::Url::parse(&format!("https://{COGNITO_DOMAIN}/oauth2/authorize"))?;
        url.query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", CLIENT_ID)
            .append_pair("redirect_uri", REDIRECT)
            .append_pair("scope", "openid email profile")
            .append_pair("state", &state)
            .append_pair("code_challenge", &challenge(&verifier))
            .append_pair("code_challenge_method", "S256");
        if provider == "google" {
            url.query_pairs_mut().append_pair("identity_provider", "Google");
        }
        open(url.to_string());

        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(600);
        let code = loop {
            let accept = async {
                match &v6 {
                    Some(l6) => tokio::select! { r = v4.accept() => r, r = l6.accept() => r },
                    None => v4.accept().await,
                }
            };
            let (mut sock, _) = tokio::time::timeout_at(deadline, accept).await.map_err(|_| anyhow::anyhow!("Anmeldung abgelaufen. Bitte nochmal."))??;
            let mut buf = vec![0u8; 8192];
            let n = sock.read(&mut buf).await.unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
            if !path.starts_with("/callback") {
                let _ = sock.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
                continue;
            }
            let ok = query(&path, "state").as_deref() == Some(state.as_str()) && query(&path, "code").is_some();
            let (title, text) = if ok {
                ("Angemeldet", "Du bist bei PlexCloud angemeldet. Dieses Fenster kannst du schließen.")
            } else {
                ("Anmeldung fehlgeschlagen", "Bitte in PlexCloud nochmal auf „Anmelden“ klicken.")
            };
            let html = format!("<!doctype html><meta charset=utf-8><title>{title}</title><body style=\"font-family:sans-serif;background:#070B16;color:#E8EEFF;display:grid;place-items:center;height:100vh;margin:0\"><div><h1 style=\"font-size:22px\">{title}</h1><p>{text}</p></div>");
            let _ = sock.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{html}", html.len()).as_bytes()).await;
            if let Some(err) = query(&path, "error_description").or_else(|| query(&path, "error")) {
                anyhow::bail!(err);
            }
            if ok {
                break query(&path, "code").unwrap();
            }
        };

        let t = self.token_request(&[("grant_type", "authorization_code"), ("client_id", CLIENT_ID), ("code", &code), ("redirect_uri", REDIRECT), ("code_verifier", &verifier)], None).await?;
        let p = profile_from(&t.id_token);
        self.store(Some(t)).await?;
        Ok(p)
    }

    async fn token_request(&self, form: &[(&str, &str)], old_refresh: Option<&str>) -> anyhow::Result<Tokens> {
        let r = self.http.post(format!("https://{COGNITO_DOMAIN}/oauth2/token")).form(form).send().await?;
        let status = r.status();
        let j: serde_json::Value = r.json().await.unwrap_or_default();
        if !status.is_success() {
            let e = j.get("error").and_then(|v| v.as_str()).unwrap_or("unbekannt");
            anyhow::bail!(if e == "invalid_grant" { "Anmeldung abgelaufen. Bitte neu anmelden.".to_string() } else { format!("Anmeldung fehlgeschlagen ({e})") });
        }
        let s = |k: &str| j.get(k).and_then(|v| v.as_str()).map(String::from);
        Ok(Tokens {
            id_token: s("id_token").ok_or_else(|| anyhow::anyhow!("Kein Token erhalten"))?,
            refresh_token: s("refresh_token").or(old_refresh.map(String::from)).ok_or_else(|| anyhow::anyhow!("Kein Refresh-Token erhalten"))?,
            expires_at: now() + j.get("expires_in").and_then(|v| v.as_u64()).unwrap_or(3600),
        })
    }

    async fn store(&self, t: Option<Tokens>) -> anyhow::Result<()> {
        let path = dir().join("tokens.json");
        match &t {
            Some(t) => write_private(&path, serde_json::to_string(t)?.as_bytes())?,
            None => {
                let _ = std::fs::remove_file(&path);
            }
        }
        *self.tokens.lock().await = t;
        Ok(())
    }

    /// Gültiges ID-Token; erneuert es, wenn es in den nächsten 2 Minuten abläuft (oder force)
    pub async fn id_token(&self, force: bool) -> anyhow::Result<String> {
        let cur = self.tokens.lock().await.clone().ok_or_else(|| anyhow::anyhow!("Nicht angemeldet"))?;
        if !force && cur.expires_at > now() + 120 {
            return Ok(cur.id_token);
        }
        match self.token_request(&[("grant_type", "refresh_token"), ("client_id", CLIENT_ID), ("refresh_token", &cur.refresh_token)], Some(&cur.refresh_token)).await {
            Ok(t) => {
                let id = t.id_token.clone();
                self.store(Some(t)).await?;
                Ok(id)
            }
            Err(e) => {
                if e.to_string().contains("abgelaufen") {
                    self.store(None).await?;
                }
                Err(e)
            }
        }
    }

    pub async fn logout(&self) -> anyhow::Result<()> {
        if let Some(t) = self.tokens.lock().await.clone() {
            let _ = self.http.post(format!("https://{COGNITO_DOMAIN}/oauth2/revoke")).form(&[("token", t.refresh_token.as_str()), ("client_id", CLIENT_ID)]).send().await;
        }
        self.store(None).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge_rfc7636() {
        assert_eq!(challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn profile_from_google_token() {
        let payload = B64.encode(r#"{"given_name":"Peter","family_name":"P","email":"p@x.de","picture":"https://lh3/a=s96-c","identities":[{"providerName":"Google"}]}"#);
        let p = profile_from(&format!("h.{payload}.s"));
        assert_eq!(p.name, "Peter P");
        assert_eq!(p.picture.as_deref(), Some("https://lh3/a=s256-c"));
        assert_eq!(p.provider, "Google");
    }

    #[test]
    fn callback_query() {
        assert_eq!(query("/callback?code=abc&state=x%2By", "state").as_deref(), Some("x+y"));
        assert_eq!(query("/callback", "code"), None);
    }
}
