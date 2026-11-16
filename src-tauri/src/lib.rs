//! PlexCloud für Linux: Fenster, Tray-Symbol, Hintergrund-Abgleich und die Befehle für die Oberfläche.

mod api;
mod auth;
mod config;
mod engine;
mod plan;
mod tray;

use config::{Config, Mapping};
use engine::Engine;
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Manager, State};
use tokio::sync::mpsc;

pub const REPO: &str = "peter1965p/PlexCloud-Desktop";

pub struct App {
    engine: Arc<Engine>,
    trigger: mpsc::Sender<()>,
    update: tokio::sync::Mutex<Option<UpdateInfo>>,
}

#[derive(Serialize, Clone, Debug)]
pub struct UpdateInfo {
    version: String,
    url: String,
}

#[derive(Serialize)]
struct View {
    status: engine::Status,
    profile: Option<auth::Profile>,
    config: Config,
    version: &'static str,
    update: Option<UpdateInfo>,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[tauri::command]
async fn view(app: State<'_, App>) -> Result<View, String> {
    let mut status = app.engine.snapshot().await;
    status.signed_in = app.engine.api.auth.signed_in().await;
    status.paused = Config::load().paused;
    Ok(View { status, profile: app.engine.api.auth.profile().await, config: Config::load(), version: env!("CARGO_PKG_VERSION"), update: app.update.lock().await.clone() })
}

#[tauri::command]
async fn login(handle: AppHandle, app: State<'_, App>, provider: String) -> Result<auth::Profile, String> {
    let h = handle.clone();
    let p = app
        .engine
        .api
        .auth
        .login(&provider, move |url| {
            use tauri_plugin_opener::OpenerExt;
            let _ = h.opener().open_url(url, None::<&str>);
        })
        .await
        .map_err(err)?;
    if let Some(w) = handle.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
    let _ = app.trigger.try_send(());
    tray::refresh(&handle);
    Ok(p)
}

#[tauri::command]
async fn logout(handle: AppHandle, app: State<'_, App>) -> Result<(), String> {
    app.engine.api.auth.logout().await.map_err(err)?;
    tray::refresh(&handle);
    Ok(())
}

#[tauri::command]
async fn sync_now(app: State<'_, App>) -> Result<(), String> {
    let _ = app.trigger.try_send(());
    Ok(())
}

#[tauri::command]
async fn set_paused(handle: AppHandle, app: State<'_, App>, paused: bool) -> Result<(), String> {
    let mut c = Config::load();
    c.paused = paused;
    c.save().map_err(err)?;
    tray::refresh(&handle);
    if !paused {
        let _ = app.trigger.try_send(());
    }
    Ok(())
}

#[tauri::command]
async fn approve(app: State<'_, App>, remote: String) -> Result<(), String> {
    Engine::approve(&remote).map_err(err)?;
    let _ = app.trigger.try_send(());
    Ok(())
}

#[tauri::command]
async fn allow_deletes(app: State<'_, App>, remote: String) -> Result<(), String> {
    app.engine.allow_deletes.lock().await.push(remote);
    let _ = app.trigger.try_send(());
    Ok(())
}

#[tauri::command]
async fn set_accent(handle: AppHandle, accent: String) -> Result<(), String> {
    if !Config::valid_accent(&accent) {
        return Err("Bitte eine Farbe wie #FF7A2F angeben.".into());
    }
    let mut c = Config::load();
    c.accent = accent.to_uppercase();
    c.save().map_err(err)?;
    tray::refresh(&handle);
    Ok(())
}

#[derive(serde::Deserialize)]
struct Settings {
    upload_limit: u64,
    excludes: Vec<String>,
    autostart: bool,
    auto_update: bool,
}

#[tauri::command]
async fn save_settings(s: Settings) -> Result<(), String> {
    let mut c = Config::load();
    let autostart_changed = c.autostart != s.autostart;
    c.upload_limit = s.upload_limit;
    c.excludes = s.excludes.into_iter().map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect();
    c.autostart = s.autostart;
    c.auto_update = s.auto_update;
    c.save().map_err(err)?;
    if autostart_changed {
        autostart(s.autostart).map_err(err)?;
    }
    Ok(())
}

#[tauri::command]
async fn set_mapping(app: State<'_, App>, remote: String, local: String, enabled: bool) -> Result<(), String> {
    let remote = remote.trim().trim_matches('/').to_string();
    if remote.is_empty() || remote.contains('/') {
        return Err("Bitte einen Ordner aus dem Stamm der Cloud angeben, z. B. Musik.".into());
    }
    let local = expand(&local);
    if !local.is_absolute() || local == config::home() || !local.starts_with(config::home()) {
        return Err("Bitte einen Ordner unterhalb deines Home-Ordners angeben, nicht das Home selbst.".into());
    }
    let mut c = Config::load();
    // Ordner darf nicht in einem anderen verbundenen Ordner liegen (sonst doppelt)
    if c.mappings.iter().any(|m| !m.remote.eq_ignore_ascii_case(&remote) && (local.starts_with(&m.local) || m.local.starts_with(&local))) {
        return Err("Dieser Ordner überschneidet sich mit einer anderen Zuordnung.".into());
    }
    match c.mappings.iter_mut().find(|m| m.remote.eq_ignore_ascii_case(&remote)) {
        Some(m) => {
            if m.local != local {
                // anderer Ordner: neu bestätigen, alten Stand vergessen (sonst sähe alles wie gelöscht aus)
                m.approved = false;
                engine::forget_state(&m.remote);
            }
            m.local = local;
            m.enabled = enabled;
        }
        None => c.mappings.push(Mapping { remote, local, enabled, approved: false }),
    }
    c.save().map_err(err)?;
    let _ = app.trigger.try_send(());
    Ok(())
}

#[tauri::command]
async fn remove_mapping(remote: String) -> Result<(), String> {
    let mut c = Config::load();
    c.mappings.retain(|m| !m.remote.eq_ignore_ascii_case(&remote));
    engine::forget_state(&remote);
    c.save().map_err(err)
}

fn expand(p: &str) -> std::path::PathBuf {
    let p = p.trim();
    match p.strip_prefix("~/") {
        Some(r) => config::home().join(r),
        None if p == "~" => config::home(),
        None => std::path::PathBuf::from(p),
    }
}

/// Autostart als systemd-Benutzerdienst, gebunden an die grafische Sitzung (Plasma)
fn autostart(on: bool) -> anyhow::Result<()> {
    let unit_dir = dirs::config_dir().unwrap_or_default().join("systemd/user");
    let unit = unit_dir.join("plexcloud.service");
    if on {
        let exe = std::env::current_exe()?;
        std::fs::create_dir_all(&unit_dir)?;
        std::fs::write(
            &unit,
            format!("[Unit]\nDescription=PlexCloud Abgleich\nPartOf=graphical-session.target\nAfter=graphical-session.target\n\n[Service]\nExecStart={} --hidden\nRestart=on-failure\nRestartSec=10\n\n[Install]\nWantedBy=graphical-session.target\n", exe.display()),
        )?;
        std::process::Command::new("systemctl").args(["--user", "daemon-reload"]).status()?;
        std::process::Command::new("systemctl").args(["--user", "enable", "plexcloud.service"]).status()?;
    } else {
        let _ = std::process::Command::new("systemctl").args(["--user", "disable", "plexcloud.service"]).status();
        let _ = std::fs::remove_file(unit);
        let _ = std::process::Command::new("systemctl").args(["--user", "daemon-reload"]).status();
    }
    Ok(())
}

fn newer(a: &str, b: &str) -> bool {
    let p = |s: &str| s.split('.').map(|x| x.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    p(a) > p(b)
}

/// Neueste Version auf GitHub; None, wenn diese aktuell ist
async fn check_update(http: &reqwest::Client) -> Option<UpdateInfo> {
    let j: serde_json::Value = http.get(format!("https://api.github.com/repos/{REPO}/releases/latest")).header("User-Agent", "PlexCloud-Linux").send().await.ok()?.json().await.ok()?;
    let tag = j["tag_name"].as_str()?.trim_start_matches('v').to_string();
    newer(&tag, env!("CARGO_PKG_VERSION")).then(|| UpdateInfo { version: tag, url: j["html_url"].as_str().unwrap_or("").to_string() })
}

/// Lokale Ordner beobachten: Änderung → nach 5 s Ruhe abgleichen
fn watch(trigger: mpsc::Sender<()>) -> Option<notify::RecommendedWatcher> {
    use notify::{RecursiveMode, Watcher};
    let (ev_tx, mut ev_rx) = mpsc::unbounded_channel::<()>();
    let mut w = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(e) = res {
            if !matches!(e.kind, notify::EventKind::Access(_)) && e.paths.iter().any(|p| !p.to_string_lossy().ends_with(".plexcloud-part")) {
                let _ = ev_tx.send(());
            }
        }
    })
    .ok()?;
    for m in Config::load().mappings.iter().filter(|m| m.enabled && m.local.is_dir()) {
        let _ = w.watch(&m.local, RecursiveMode::Recursive);
    }
    tauri::async_runtime::spawn(async move {
        while ev_rx.recv().await.is_some() {
            while let Ok(Some(())) = tokio::time::timeout(Duration::from_secs(5), ev_rx.recv()).await {}
            let _ = trigger.try_send(());
        }
    });
    Some(w)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let hidden = std::env::args().any(|a| a == "--hidden");
    let http = reqwest::Client::builder().connect_timeout(Duration::from_secs(15)).build().expect("HTTP-Client");
    let auth = Arc::new(auth::Auth::new(http.clone()));
    let engine = Arc::new(Engine::new(Arc::new(api::Api::new(auth, http.clone()))));
    let (trigger, rx) = mpsc::channel::<()>(4);
    let rx = std::sync::Mutex::new(Some(rx));

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .manage(App { engine: engine.clone(), trigger: trigger.clone(), update: tokio::sync::Mutex::new(None) })
        .invoke_handler(tauri::generate_handler![view, login, logout, sync_now, set_paused, approve, allow_deletes, set_accent, save_settings, set_mapping, remove_mapping])
        .setup(move |app| {
            // erster Start: Einstellungen anlegen und Autostart einrichten
            if !config::dir().join("config.json").exists() {
                let c = Config::default();
                let _ = c.save();
                if c.autostart {
                    let _ = autostart(true);
                }
            }
            tray::create(app.handle())?;
            if let Some(w) = app.get_webview_window("main") {
                if !hidden {
                    let _ = w.show();
                }
            }

            let handle = app.handle().clone();
            let eng = engine.clone();
            let trig = trigger.clone();
            let mut rx = rx.lock().unwrap().take().expect("einmal");
            tauri::async_runtime::spawn(async move {
                let mut watcher = watch(trig.clone());
                let mut watched = Config::load().mappings;
                loop {
                    let _ = eng.run().await;
                    tray::refresh(&handle);
                    // Zuordnungen geändert: Beobachter neu aufsetzen
                    let now = Config::load().mappings;
                    if now != watched {
                        drop(watcher.take());
                        watcher = watch(trig.clone());
                        watched = now;
                    }
                    let interval = Config::load().interval.clamp(30, 3600);
                    let _ = tokio::time::timeout(Duration::from_secs(interval), rx.recv()).await;
                }
            });

            // während eines Laufs das Tray jede Sekunde nachführen (Fortschritt)
            let handle = app.handle().clone();
            let eng = engine.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    if eng.snapshot().await.running {
                        tray::refresh(&handle);
                    }
                }
            });

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    if Config::load().auto_update {
                        let u = check_update(&http).await;
                        let st: State<App> = handle.state();
                        *st.update.lock().await = u;
                    }
                    tokio::time::sleep(Duration::from_secs(6 * 3600)).await;
                }
            });
            Ok(())
        })
        .on_window_event(|w, e| {
            // Schließen = ins Tray, der Abgleich läuft weiter
            if let tauri::WindowEvent::CloseRequested { api, .. } = e {
                let _ = w.hide();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("PlexCloud konnte nicht starten");
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_compare() {
        assert!(super::newer("0.1.10", "0.1.9"));
        assert!(!super::newer("0.1.0", "0.1.0"));
        assert!(super::expand("~/Musik").ends_with("Musik"));
    }
}
