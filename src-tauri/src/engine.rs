//! Abgleich ausführen: Ordner lesen, Plan machen, abarbeiten, Stand merken. Ein Lauf nach dem anderen.

use crate::api::{Api, Progress, RemoteItem, Usage, ROOT};
use crate::config::{Config, Mapping};
use crate::plan::{self, Action, Entry, Local, Remote};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

#[derive(Serialize, Clone, Debug, Default)]
pub struct Summary {
    pub uploads: usize,
    pub upload_bytes: u64,
    pub downloads: usize,
    pub download_bytes: u64,
    pub deletes: usize,
    pub conflicts: usize,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct MappingStatus {
    pub remote: String,
    pub local: String,
    pub enabled: bool,
    pub approved: bool,
    /// synchron | arbeitet | wartet | fehler | aus | löschstopp
    pub state: String,
    pub message: String,
    pub pending: Summary,
}

#[derive(Serialize, Clone, Debug)]
pub struct Recent {
    pub name: String,
    /// up | down | trash
    pub dir: String,
    pub folder: String,
    pub at: u64,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Current {
    pub name: String,
    pub dir: String,
    pub path: String,
    pub done: u64,
    pub total: u64,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Status {
    pub signed_in: bool,
    pub paused: bool,
    pub running: bool,
    pub online: bool,
    pub error: String,
    pub last_sync: u64,
    pub mappings: Vec<MappingStatus>,
    pub current: Option<Current>,
    pub recent: Vec<Recent>,
    pub usage: Option<Usage>,
    /// Ordner im Stamm der Cloud, die noch nicht verbunden sind
    pub unmapped: Vec<String>,
}

pub struct Engine {
    pub api: Arc<Api>,
    pub status: Mutex<Status>,
    pub progress: Arc<Progress>,
    run_lock: Mutex<()>,
    /// Löschungen über der Sicherungsgrenze einmal erlauben (je Cloud-Ordner)
    pub allow_deletes: Mutex<Vec<String>>,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn state_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_else(|| crate::config::home().join(".local/share")).join("plexcloud")
}

fn state_path(remote: &str) -> PathBuf {
    let safe: String = remote.chars().map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' }).collect();
    state_dir().join(format!("stand-{}.json", safe.to_lowercase()))
}

pub fn load_state(remote: &str) -> BTreeMap<String, Entry> {
    std::fs::read_to_string(state_path(remote)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub fn forget_state(remote: &str) {
    let _ = std::fs::remove_file(state_path(remote));
}

fn save_state(remote: &str, s: &BTreeMap<String, Entry>) -> anyhow::Result<()> {
    crate::config::write_private(&state_path(remote), serde_json::to_string(s)?.as_bytes())
}

pub fn mtime_of(m: &std::fs::Metadata) -> i64 {
    use std::os::unix::fs::MetadataExt;
    m.mtime() * 1000 + m.mtime_nsec() / 1_000_000
}

/// Lokalen Ordner lesen; Dateien, die gerade noch geschrieben werden (jünger als 3 s), bleiben bis zum nächsten Lauf liegen
pub fn scan_local(root: &Path, excludes: &[String]) -> Vec<Local> {
    let mut out = Vec::new();
    let fresh = (now() as i64 - 3) * 1000;
    let mut seen = std::collections::HashSet::new();
    for e in walkdir::WalkDir::new(root).follow_links(false).min_depth(1).into_iter().filter_entry(|e| {
        let rel = e.path().strip_prefix(root).ok().and_then(|p| p.to_str()).unwrap_or("");
        !plan::excluded(rel, excludes)
    }) {
        let Ok(e) = e else { continue };
        let Some(rel) = e.path().strip_prefix(root).ok().and_then(|p| p.to_str()).map(String::from) else { continue };
        let ft = e.file_type();
        if ft.is_symlink() || !(ft.is_file() || ft.is_dir()) {
            continue;
        }
        // zwei Dateien, die sich nur in Groß-/Kleinschreibung unterscheiden: die zweite auslassen
        if !seen.insert(plan::key(&rel)) {
            continue;
        }
        let Ok(m) = e.metadata() else { continue };
        let mt = mtime_of(&m);
        if ft.is_file() && mt > fresh {
            continue;
        }
        out.push(Local { rel, dir: ft.is_dir(), size: if ft.is_file() { m.len() } else { 0 }, mtime: if ft.is_file() { mt } else { 0 } });
    }
    out
}

/// Cloud-Elemente unter dem Ordner `top` als relative Pfade
pub fn remote_under(tree: &[RemoteItem], top_id: &str) -> Vec<Remote> {
    let by_id: HashMap<&str, &RemoteItem> = tree.iter().map(|i| (i.item_id.as_str(), i)).collect();
    let mut cache: HashMap<String, Option<String>> = HashMap::new();
    fn rel_of(id: &str, top: &str, by_id: &HashMap<&str, &RemoteItem>, cache: &mut HashMap<String, Option<String>>, depth: usize) -> Option<String> {
        if id == top {
            return Some(String::new());
        }
        if depth > 64 {
            return None;
        }
        if let Some(c) = cache.get(id) {
            return c.clone();
        }
        let r = by_id.get(id).and_then(|i| {
            if i.parent_id.is_empty() || i.parent_id == ROOT {
                return None;
            }
            rel_of(&i.parent_id, top, by_id, cache, depth + 1).map(|p| if p.is_empty() { i.name.clone() } else { format!("{p}/{}", i.name) })
        });
        cache.insert(id.to_string(), r.clone());
        r
    }
    tree.iter()
        .filter(|i| i.item_id != top_id)
        .filter_map(|i| rel_of(&i.item_id, top_id, &by_id, &mut cache, 0).map(|rel| Remote { rel, dir: i.folder(), item_id: i.item_id.clone(), size: i.size, updated: i.updated_at.clone() }))
        .collect()
}

fn top_folder<'a>(tree: &'a [RemoteItem], name: &str) -> Option<&'a RemoteItem> {
    tree.iter().find(|i| i.folder() && (i.parent_id == ROOT || i.parent_id.is_empty()) && i.name.eq_ignore_ascii_case(name))
}

pub fn summarize(actions: &[Action]) -> Summary {
    let mut s = Summary::default();
    for a in actions {
        match a {
            Action::Upload { size, .. } => {
                s.uploads += 1;
                s.upload_bytes += size;
            }
            Action::Download { size, .. } => {
                s.downloads += 1;
                s.download_bytes += size;
            }
            Action::Conflict { size, .. } => {
                s.conflicts += 1;
                s.downloads += 1;
                s.download_bytes += size;
            }
            Action::TrashLocal { .. } | Action::TrashRemote { .. } => s.deletes += 1,
            _ => {}
        }
    }
    s
}

fn today() -> String {
    // Datum ohne Zusatz-Crate: Tage seit 1970 → Kalender (proleptisch gregorianisch, UTC)
    let days = (now() / 86400) as i64;
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}")
}

impl Engine {
    pub fn new(api: Arc<Api>) -> Engine {
        Engine { api, status: Mutex::new(Status::default()), progress: Arc::new(Progress::default()), run_lock: Mutex::new(()), allow_deletes: Mutex::new(Vec::new()) }
    }

    async fn set_current(&self, c: Option<Current>) {
        self.status.lock().await.current = c;
    }

    async fn push_recent(&self, name: &str, dir: &str, folder: &str) {
        let mut s = self.status.lock().await;
        s.recent.insert(0, Recent { name: name.into(), dir: dir.into(), folder: folder.into(), at: now() });
        s.recent.truncate(30);
    }

    pub async fn snapshot(&self) -> Status {
        let mut s = self.status.lock().await.clone();
        if let Some(c) = s.current.as_mut() {
            c.done = self.progress.done.load(Ordering::Relaxed);
            c.total = self.progress.total.load(Ordering::Relaxed).max(c.total);
        }
        s
    }

    /// Einmal alles abgleichen. Gibt true zurück, wenn etwas übertragen wurde.
    pub async fn run(&self) -> anyhow::Result<bool> {
        let Ok(_guard) = self.run_lock.try_lock() else { return Ok(false) };
        let cfg = Config::load();
        {
            let mut s = self.status.lock().await;
            s.paused = cfg.paused;
            s.signed_in = self.api.auth.signed_in().await;
            if !s.signed_in || cfg.paused {
                return Ok(false);
            }
            s.running = true;
            s.error.clear();
        }
        let r = self.run_inner(&cfg).await;
        let mut s = self.status.lock().await;
        s.running = false;
        s.current = None;
        match &r {
            Ok(_) => {
                s.online = true;
                s.last_sync = now();
            }
            Err(e) => {
                s.online = e.downcast_ref::<reqwest::Error>().map(|x| !(x.is_connect() || x.is_timeout())).unwrap_or(true);
                s.error = e.to_string();
                s.signed_in = self.api.auth.signed_in().await;
            }
        }
        r
    }

    async fn run_inner(&self, cfg: &Config) -> anyhow::Result<bool> {
        let (mut tree, usage) = self.api.tree().await?;
        // Monatsordner der Handy-App (bis 0.2.7) zuerst in Jahresordner, damit nichts doppelt landet
        if self.migrate_years(&tree).await? {
            tree = self.api.tree().await?.0;
        }
        {
            let mut s = self.status.lock().await;
            s.usage = Some(usage);
            s.unmapped = tree
                .iter()
                .filter(|i| i.folder() && (i.parent_id == ROOT || i.parent_id.is_empty()) && !cfg.mappings.iter().any(|m| m.remote.eq_ignore_ascii_case(&i.name)))
                .map(|i| i.name.clone())
                .collect();
            s.mappings = cfg.mappings.iter().map(|m| MappingStatus { remote: m.remote.clone(), local: m.local.display().to_string(), enabled: m.enabled, approved: m.approved, state: if m.enabled { "arbeitet".into() } else { "aus".into() }, ..Default::default() }).collect();
        }
        let mut moved = false;
        for (n, m) in cfg.mappings.iter().enumerate() {
            if !m.enabled {
                continue;
            }
            let res = self.sync_mapping(m, &tree, cfg).await;
            let mut s = self.status.lock().await;
            let ms = &mut s.mappings[n];
            match res {
                Ok((st, msg, pending, did)) => {
                    ms.state = st;
                    ms.message = msg;
                    ms.pending = pending;
                    moved |= did;
                }
                Err(e) => {
                    ms.state = "fehler".into();
                    ms.message = e.to_string();
                }
            }
        }
        Ok(moved)
    }

    /// Bilder/2026-11 → Bilder/2026 (ebenso Videos). Doppelte (Name + Größe) in den Papierkorb.
    async fn migrate_years(&self, tree: &[RemoteItem]) -> anyhow::Result<bool> {
        let mut changed = false;
        for top in tree.iter().filter(|i| i.folder() && (i.parent_id == ROOT || i.parent_id.is_empty()) && (i.name.eq_ignore_ascii_case("Bilder") || i.name.eq_ignore_ascii_case("Videos"))) {
            let months: Vec<&RemoteItem> = tree.iter().filter(|i| i.folder() && i.parent_id == top.item_id && plan::month_to_year(&i.name).is_some()).collect();
            let mut years: HashMap<String, String> = tree.iter().filter(|i| i.folder() && i.parent_id == top.item_id).map(|i| (i.name.clone(), i.item_id.clone())).collect();
            for month in months {
                let year = plan::month_to_year(&month.name).unwrap().to_string();
                let target = match years.get(&year) {
                    Some(id) => id.clone(),
                    None => {
                        let f = self.api.create_folder(&year, &top.item_id).await?;
                        years.insert(year.clone(), f.item_id.clone());
                        f.item_id
                    }
                };
                let mut have: std::collections::HashSet<(String, u64)> = tree.iter().filter(|i| i.parent_id == target && !i.folder()).map(|i| (strip_copy(&i.name), i.size)).collect();
                for f in tree.iter().filter(|i| i.parent_id == month.item_id) {
                    if !f.folder() && have.contains(&(strip_copy(&f.name), f.size)) {
                        self.api.trash(&f.item_id).await?;
                    } else {
                        self.api.move_to(&f.item_id, &target).await?;
                        have.insert((strip_copy(&f.name), f.size));
                    }
                }
                self.api.trash(&month.item_id).await?;
                changed = true;
            }
        }
        Ok(changed)
    }

    async fn sync_mapping(&self, m: &Mapping, tree: &[RemoteItem], cfg: &Config) -> anyhow::Result<(String, String, Summary, bool)> {
        if !m.local.is_dir() {
            if m.local.exists() {
                anyhow::bail!("{} ist kein Ordner", m.local.display());
            }
            // Ordner fehlt ganz (z. B. Platte nicht eingehängt): nichts löschen, nur anlegen, wenn er noch nie abgeglichen war
            if !load_state(&m.remote).is_empty() {
                anyhow::bail!("{} fehlt. Nichts wird gelöscht, bis der Ordner wieder da ist.", m.local.display());
            }
            std::fs::create_dir_all(&m.local)?;
        }
        let state = load_state(&m.remote);
        let top = top_folder(tree, &m.remote);
        let remote = top.map(|t| remote_under(tree, &t.item_id)).unwrap_or_default();
        let local = scan_local(&m.local, &cfg.excludes);
        let actions = plan::plan(&local, &remote, &state);
        let pending = summarize(&actions);

        if !m.approved {
            return Ok(("wartet".into(), "Erster Abgleich: bitte bestätigen".into(), pending, false));
        }
        if let Some(n) = plan::too_many_deletes(&actions, state.len()) {
            let mut allow = self.allow_deletes.lock().await;
            if let Some(i) = allow.iter().position(|r| r.eq_ignore_ascii_case(&m.remote)) {
                allow.remove(i);
            } else {
                return Ok(("löschstopp".into(), format!("{n} Dateien würden gelöscht. Angehalten, bis du es bestätigst."), pending, false));
            }
        }
        if actions.is_empty() {
            return Ok(("synchron".into(), String::new(), pending, false));
        }

        let top_id = match top {
            Some(t) => t.item_id.clone(),
            None => self.api.create_folder(&m.remote, ROOT).await?.item_id,
        };
        let mut folders: HashMap<String, String> = remote.iter().filter(|r| r.dir).map(|r| (plan::key(&r.rel), r.item_id.clone())).collect();
        folders.insert(String::new(), top_id.clone());
        let mut state = state;
        let mut did = false;
        let mut errors = 0usize;
        let mut last_err = String::new();

        // erst Ordner und Löschungen, dann Downloads (klein zuerst), dann Uploads (klein zuerst)
        let mut actions = actions;
        actions.sort_by_key(|a| match a {
            Action::Forget { .. } | Action::Record { .. } => (0, 0),
            Action::MkdirLocal { .. } | Action::MkdirRemote { .. } => (1, 0),
            Action::TrashLocal { .. } | Action::TrashRemote { .. } => (2, 0),
            Action::Conflict { size, .. } | Action::Download { size, .. } => (3, *size),
            Action::Upload { size, .. } => (4, *size),
        });
        let remote_by_key: HashMap<String, &Remote> = remote.iter().map(|r| (plan::key(&r.rel), r)).collect();

        for a in &actions {
            if Config::load().paused {
                break;
            }
            let r = self.apply(a, m, &top_id, &mut folders, &remote_by_key, &mut state, cfg).await;
            match r {
                Ok(moved) => did |= moved,
                Err(e) => {
                    errors += 1;
                    last_err = e.to_string();
                }
            }
            // Stand regelmäßig sichern, damit ein Abbruch nichts doppelt macht
            save_state(&m.remote, &state)?;
        }
        // Stand der hochgeladenen Dateien aus der Cloud nachtragen (itemId, updatedAt)
        if actions.iter().any(|a| matches!(a, Action::Upload { .. } | Action::MkdirRemote { .. } | Action::Conflict { .. })) {
            let (tree2, _) = self.api.tree().await?;
            if let Some(t) = top_folder(&tree2, &m.remote) {
                for r in remote_under(&tree2, &t.item_id) {
                    if let Some(e) = state.get_mut(&plan::key(&r.rel)) {
                        if e.item_id.is_empty() {
                            e.item_id = r.item_id.clone();
                            e.updated = r.updated.clone();
                            e.size = if r.dir { 0 } else { e.size };
                        }
                    }
                }
            }
            save_state(&m.remote, &state)?;
        }
        if errors > 0 {
            return Ok(("fehler".into(), format!("{errors} nicht geschafft: {last_err}"), pending, did));
        }
        Ok(("synchron".into(), String::new(), Summary::default(), did))
    }

    async fn ensure_remote(&self, rel_dir: &str, folders: &mut HashMap<String, String>) -> anyhow::Result<String> {
        let k = plan::key(rel_dir);
        if let Some(id) = folders.get(&k) {
            return Ok(id.clone());
        }
        let (parent, name) = match rel_dir.rfind('/') {
            Some(i) => (&rel_dir[..i], &rel_dir[i + 1..]),
            None => ("", rel_dir),
        };
        let pid = Box::pin(self.ensure_remote(parent, folders)).await?;
        let f = self.api.create_folder(name, &pid).await?;
        folders.insert(k, f.item_id.clone());
        Ok(f.item_id)
    }

    #[allow(clippy::too_many_arguments)]
    async fn apply(&self, a: &Action, m: &Mapping, _top: &str, folders: &mut HashMap<String, String>, remote: &HashMap<String, &Remote>, state: &mut BTreeMap<String, Entry>, cfg: &Config) -> anyhow::Result<bool> {
        let path = |rel: &str| m.local.join(rel);
        let name_of = |rel: &str| rel.rsplit('/').next().unwrap_or(rel).to_string();
        let parent_of = |rel: &str| rel.rfind('/').map(|i| rel[..i].to_string()).unwrap_or_default();
        let record_local = |state: &mut BTreeMap<String, Entry>, rel: &str, item_id: &str, updated: &str, dir: bool| -> anyhow::Result<()> {
            let md = std::fs::metadata(path(rel))?;
            state.insert(plan::key(rel), Entry { rel: rel.into(), dir, item_id: item_id.into(), size: if dir { 0 } else { md.len() }, mtime: if dir { 0 } else { mtime_of(&md) }, updated: updated.into() });
            Ok(())
        };
        match a {
            Action::Forget { key } => {
                state.remove(key);
                Ok(false)
            }
            Action::Record { rel } => {
                let r = remote.get(&plan::key(rel)).ok_or_else(|| anyhow::anyhow!("weg"))?;
                record_local(state, rel, &r.item_id, &r.updated, r.dir)?;
                Ok(false)
            }
            Action::MkdirLocal { rel } => {
                std::fs::create_dir_all(path(rel))?;
                let r = remote.get(&plan::key(rel)).ok_or_else(|| anyhow::anyhow!("weg"))?;
                record_local(state, rel, &r.item_id, &r.updated, true)?;
                Ok(false)
            }
            Action::MkdirRemote { rel } => {
                let id = self.ensure_remote(rel, folders).await?;
                record_local(state, rel, &id, "", true)?;
                Ok(false)
            }
            Action::TrashLocal { rel, .. } => {
                let p = path(rel);
                if p.exists() {
                    trash::delete(&p).map_err(|e| anyhow::anyhow!("Papierkorb: {e}"))?;
                }
                state.retain(|k, _| k != &plan::key(rel) && !k.starts_with(&format!("{}/", plan::key(rel))));
                self.push_recent(&name_of(rel), "trash", &format!("~/{}", m.local.file_name().and_then(|n| n.to_str()).unwrap_or(""))).await;
                Ok(true)
            }
            Action::TrashRemote { rel, item_id, .. } => {
                match self.api.trash(item_id).await {
                    Ok(()) => {}
                    Err(e) if e.downcast_ref::<crate::api::ApiError>().map(|x| x.status == 404).unwrap_or(false) => {}
                    Err(e) => return Err(e),
                }
                state.retain(|k, _| k != &plan::key(rel) && !k.starts_with(&format!("{}/", plan::key(rel))));
                self.push_recent(&name_of(rel), "trash", &format!("/{}", m.remote)).await;
                Ok(true)
            }
            Action::Download { rel, item_id, size } => {
                self.set_current(Some(Current { name: name_of(rel), dir: "down".into(), path: format!("/{}/{} → {}", m.remote, parent_of(rel), m.local.join(parent_of(rel)).display()), done: 0, total: *size })).await;
                let p = path(rel);
                if p.exists() {
                    // ältere lokale Fassung nicht überschreiben, sondern in den Papierkorb
                    let _ = trash::delete(&p);
                }
                self.api.download(item_id, &p, &self.progress).await?;
                let r = remote.get(&plan::key(rel)).ok_or_else(|| anyhow::anyhow!("weg"))?;
                record_local(state, rel, &r.item_id, &r.updated, false)?;
                self.push_recent(&name_of(rel), "down", &shorten(&m.local.join(parent_of(rel)))).await;
                Ok(true)
            }
            Action::Conflict { rel, item_id, size } => {
                let copy = plan::conflict_name(rel, &today());
                std::fs::rename(path(rel), path(&copy))?;
                self.set_current(Some(Current { name: name_of(rel), dir: "down".into(), path: format!("/{}/{}", m.remote, parent_of(rel)), done: 0, total: *size })).await;
                self.api.download(item_id, &path(rel), &self.progress).await?;
                let r = remote.get(&plan::key(rel)).ok_or_else(|| anyhow::anyhow!("weg"))?;
                record_local(state, rel, &r.item_id, &r.updated, false)?;
                self.push_recent(&name_of(&copy), "conflict", &shorten(&m.local.join(parent_of(rel)))).await;
                Ok(true)
            }
            Action::Upload { rel, size, replace } => {
                let p = path(rel);
                let before = std::fs::metadata(&p)?;
                self.set_current(Some(Current { name: name_of(rel), dir: "up".into(), path: format!("{} → /{}/{}", shorten(&m.local.join(parent_of(rel))), m.remote, parent_of(rel)), done: 0, total: *size })).await;
                let parent = self.ensure_remote(&parent_of(rel), folders).await?;
                self.api.upload(&p, &name_of(rel), &parent, replace.as_deref(), cfg.upload_limit, &self.progress).await?;
                // während des Uploads geändert: Stand nicht merken, nächster Lauf lädt die neue Fassung
                let after = std::fs::metadata(&p)?;
                if mtime_of(&after) == mtime_of(&before) && after.len() == before.len() {
                    // updatedAt kennt erst der nächste Baum: leer lassen, wird nach dem Lauf nachgetragen
                    state.insert(plan::key(rel), Entry { rel: rel.clone(), dir: false, item_id: String::new(), size: after.len(), mtime: mtime_of(&after), updated: String::new() });
                }
                self.push_recent(&name_of(rel), "up", &format!("/{}/{}", m.remote, parent_of(rel)).trim_end_matches('/').to_string()).await;
                Ok(true)
            }
        }
    }

    /// Plan für den ersten Abgleich bestätigen
    pub fn approve(remote: &str) -> anyhow::Result<()> {
        let mut c = Config::load();
        for m in c.mappings.iter_mut().filter(|m| m.remote.eq_ignore_ascii_case(remote)) {
            m.approved = true;
        }
        c.save()
    }
}

/// „IMG_1 (2).jpg“ → „img_1.jpg“
fn strip_copy(name: &str) -> String {
    let n = name.to_lowercase();
    let (stem, ext) = match n.rfind('.') {
        Some(i) if i > 0 => (n[..i].to_string(), n[i..].to_string()),
        _ => (n.clone(), String::new()),
    };
    let stem = match stem.rfind(" (") {
        Some(i) if stem.ends_with(')') && stem[i + 2..stem.len() - 1].chars().all(|c| c.is_ascii_digit()) && stem.len() > i + 3 => stem[..i].to_string(),
        _ => stem,
    };
    format!("{stem}{ext}")
}

fn shorten(p: &Path) -> String {
    let h = crate::config::home();
    match p.strip_prefix(&h) {
        Ok(r) => format!("~/{}", r.display()).trim_end_matches('/').to_string(),
        Err(_) => p.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn it(id: &str, kind: &str, name: &str, parent: &str, size: u64) -> RemoteItem {
        RemoteItem { item_id: id.into(), kind: kind.into(), name: name.into(), parent_id: parent.into(), size, content_type: String::new(), updated_at: "t".into() }
    }

    #[test]
    fn remote_paths_under_top() {
        let t = vec![it("b", "folder", "Bilder", "root", 0), it("y", "folder", "2026", "b", 0), it("f", "file", "a.jpg", "y", 5), it("d", "folder", "Dokumente", "root", 0), it("x", "file", "x.pdf", "d", 1)];
        let mut r: Vec<String> = remote_under(&t, "b").into_iter().map(|r| r.rel).collect();
        r.sort();
        assert_eq!(r, vec!["2026", "2026/a.jpg"]);
        assert_eq!(top_folder(&t, "bilder").map(|i| i.item_id.as_str()), Some("b"));
    }

    #[test]
    fn copy_suffix() {
        assert_eq!(strip_copy("IMG_1 (2).jpg"), "img_1.jpg");
        assert_eq!(strip_copy("Urlaub (Teil).jpg"), "urlaub (teil).jpg");
        assert_eq!(strip_copy("a"), "a");
    }

    #[test]
    fn scan_skips_excluded_and_fresh() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("2026/.cache")).unwrap();
        std::fs::write(d.path().join("2026/a.jpg"), b"12345").unwrap();
        std::fs::write(d.path().join("2026/.cache/x"), b"1").unwrap();
        std::fs::write(d.path().join("neu.jpg"), b"1").unwrap();
        let old = filetime::FileTime::from_unix_time(1_700_000_000, 0);
        filetime::set_file_mtime(d.path().join("2026/a.jpg"), old).unwrap();
        let mut l = scan_local(d.path(), &[".cache".into()]);
        l.sort_by(|a, b| a.rel.cmp(&b.rel));
        let rels: Vec<&str> = l.iter().map(|x| x.rel.as_str()).collect();
        assert_eq!(rels, vec!["2026", "2026/a.jpg"]);
        assert_eq!(l[1].size, 5);
    }

    #[test]
    fn date() {
        assert_eq!(today().len(), 10);
    }
}
