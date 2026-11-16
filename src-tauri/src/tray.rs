//! Tray-Symbol (KDE StatusNotifier über libayatana): Wolke in der Akzentfarbe, Menü mit den wichtigsten Aktionen.

use crate::config::Config;
use crate::App;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

const ID: &str = "plexcloud";

fn hex(s: &str) -> [u8; 3] {
    let v = u32::from_str_radix(s.trim_start_matches('#'), 16).unwrap_or(0xFF7A2F);
    [(v >> 16) as u8, (v >> 8) as u8, v as u8]
}

/// Wolke als Bild berechnen (64×64, kantengeglättet), damit sie jede Akzentfarbe annehmen kann.
/// paused: grau; error: kleiner roter Punkt unten rechts.
pub fn cloud(accent: &str, paused: bool, error: bool) -> Image<'static> {
    let n = 64usize;
    let [r, g, b] = if paused { [0x8C, 0x9A, 0xB8] } else { hex(accent) };
    let mut px = vec![0u8; n * n * 4];
    // Wolke aus drei Kreisen und einem Sockel
    let circles = [(22.0f32, 36.0f32, 12.0f32), (36.0, 28.0, 16.0), (48.0, 38.0, 10.0)];
    let inside = |x: f32, y: f32| -> bool {
        circles.iter().any(|(cx, cy, cr)| (x - cx).powi(2) + (y - cy).powi(2) <= cr * cr) || (x >= 14.0 && x <= 52.0 && y >= 34.0 && y <= 48.0 && {
            // Sockel unten abgerundet
            let left = (x - 18.0).min(0.0);
            let right = (x - 48.0).max(0.0);
            let dy = (y - 44.0).max(0.0);
            (left * left + dy * dy <= 16.0) && (right * right + dy * dy <= 16.0)
        })
    };
    for y in 0..n {
        for x in 0..n {
            // 4×4 Unterabtastung für weiche Kanten
            let mut hit = 0u32;
            for sy in 0..4 {
                for sx in 0..4 {
                    if inside(x as f32 + (sx as f32 + 0.5) / 4.0, y as f32 + (sy as f32 + 0.5) / 4.0) {
                        hit += 1;
                    }
                }
            }
            let i = (y * n + x) * 4;
            let mut a = (hit * 255 / 16) as u8;
            let (mut cr, mut cg, mut cb) = (r, g, b);
            if error {
                let d = ((x as f32 - 52.0).powi(2) + (y as f32 - 52.0).powi(2)).sqrt();
                if d <= 10.0 {
                    (cr, cg, cb, a) = (0xFF, 0x5C, 0x5C, 255);
                } else if d <= 13.0 {
                    a = 0;
                }
            }
            px[i..i + 4].copy_from_slice(&[cr, cg, cb, a]);
        }
    }
    Image::new_owned(px, n as u32, n as u32)
}

fn show(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn open_path(app: &AppHandle, p: &std::path::Path) {
    use tauri_plugin_opener::OpenerExt;
    let _ = app.opener().open_path(p.to_string_lossy(), None::<&str>);
}

fn menu(app: &AppHandle, paused: bool, line: &str) -> tauri::Result<Menu<tauri::Wry>> {
    let status = MenuItem::with_id(app, "status", line, false, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Fenster öffnen", true, None::<&str>)?;
    let pics = MenuItem::with_id(app, "pictures", "Bilder-Ordner öffnen", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", if paused { "Synchronisierung fortsetzen" } else { "Synchronisierung pausieren" }, true, None::<&str>)?;
    let now = MenuItem::with_id(app, "now", "Jetzt abgleichen", !paused, None::<&str>)?;
    let web = MenuItem::with_id(app, "web", "PlexCloud im Browser", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Beenden", true, None::<&str>)?;
    let sep = || PredefinedMenuItem::separator(app);
    Menu::with_items(app, &[&status, &sep()?, &open, &pics, &now, &pause, &web, &sep()?, &quit])
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let c = Config::load();
    TrayIconBuilder::with_id(ID)
        .icon(cloud(&c.accent, c.paused, false))
        .tooltip("PlexCloud")
        .menu(&menu(app, c.paused, "PlexCloud startet …")?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id().as_ref() {
            "open" => show(app),
            "pictures" => {
                let c = Config::load();
                let p = c.mappings.iter().find(|m| m.remote.eq_ignore_ascii_case("Bilder")).map(|m| m.local.clone()).unwrap_or_else(|| crate::config::UserDirs::load().pictures);
                open_path(app, &p);
            }
            "pause" => {
                let mut c = Config::load();
                c.paused = !c.paused;
                let _ = c.save();
                if !c.paused {
                    let _ = app.state::<App>().trigger.try_send(());
                }
                refresh(app);
            }
            "now" => {
                let _ = app.state::<App>().trigger.try_send(());
            }
            "web" => {
                use tauri_plugin_opener::OpenerExt;
                let _ = app.opener().open_url("https://www.plexora.eu/dateien", None::<&str>);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                show(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Symbol, Tooltip und Menü an den aktuellen Stand anpassen
pub fn refresh(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let c = Config::load();
        let s = app.state::<App>().engine.snapshot().await;
        let line = if !s.signed_in {
            "Nicht angemeldet".to_string()
        } else if c.paused {
            "Pausiert".to_string()
        } else if s.running {
            s.current.as_ref().map(|c| format!("{} {}", if c.dir == "up" { "↑" } else { "↓" }, c.name)).unwrap_or_else(|| "Gleicht ab …".into())
        } else if !s.error.is_empty() || s.mappings.iter().any(|m| m.state == "fehler" || m.state == "löschstopp") {
            "Problem – Fenster öffnen".to_string()
        } else if s.mappings.iter().any(|m| m.state == "wartet") {
            "Erster Abgleich wartet auf dich".to_string()
        } else {
            "Alles synchron".to_string()
        };
        let problem = !s.error.is_empty() || s.mappings.iter().any(|m| m.state == "fehler" || m.state == "löschstopp");
        if let Some(t) = app.tray_by_id(ID) {
            let _ = t.set_icon(Some(cloud(&c.accent, c.paused, problem)));
            let _ = t.set_tooltip(Some(format!("PlexCloud · {line}")));
            if let Ok(m) = menu(&app, c.paused, &line) {
                let _ = t.set_menu(Some(m));
            }
        }
    });
}
