//! Einstellungen (~/.config/plexcloud/config.json) und die Systemordner aus xdg-user-dirs.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Ein Cloud-Ordner im Stamm von PlexCloud, verbunden mit einem lokalen Ordner.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Mapping {
    /// Name des Ordners im Stamm der Cloud, z. B. "Bilder"
    pub remote: String,
    pub local: PathBuf,
    pub enabled: bool,
    /// Erster Abgleich bestätigt (vorher zeigt die App nur, was passieren würde)
    #[serde(default)]
    pub approved: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Config {
    pub accent: String,
    pub mappings: Vec<Mapping>,
    pub paused: bool,
    /// KB/s, 0 = unbegrenzt
    pub upload_limit: u64,
    pub excludes: Vec<String>,
    pub autostart: bool,
    pub auto_update: bool,
    /// Sekunden zwischen zwei Abgleichen, wenn sich lokal nichts rührt
    pub interval: u64,
}

impl Default for Config {
    fn default() -> Self {
        let d = UserDirs::load();
        let m = |remote: &str, local: PathBuf| Mapping { remote: remote.into(), local, enabled: true, approved: false };
        Config {
            accent: "#FF7A2F".into(),
            mappings: vec![m("Bilder", d.pictures), m("Videos", d.videos), m("Dokumente", d.documents)],
            paused: false,
            upload_limit: 0,
            excludes: vec![".cache".into(), "node_modules".into(), ".git".into(), "*.tmp".into(), "*.part".into(), "~$*".into(), ".~lock.*".into(), ".directory".into()],
            autostart: true,
            auto_update: true,
            interval: 120,
        }
    }
}

pub fn dir() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| home().join(".config")).join("plexcloud")
}

pub fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

impl Config {
    pub fn load() -> Config {
        std::fs::read_to_string(dir().join("config.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        write_private(&dir().join("config.json"), serde_json::to_string_pretty(self)?.as_bytes())
    }

    /// Farbe nur als #RRGGBB annehmen
    pub fn valid_accent(s: &str) -> bool {
        s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())
    }
}

/// Datei atomar schreiben, nur für den Benutzer lesbar (Tokens, Einstellungen)
pub fn write_private(path: &Path, data: &[u8]) -> anyhow::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    let tmp = path.with_extension("tmp");
    {
        let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    std::fs::rename(tmp, path)?;
    Ok(())
}

/// Systemordner wie in ~/.config/user-dirs.dirs (deutsch: Bilder, Videos, Dokumente)
pub struct UserDirs {
    pub pictures: PathBuf,
    pub videos: PathBuf,
    pub documents: PathBuf,
}

impl UserDirs {
    pub fn load() -> UserDirs {
        let h = home();
        let text = std::fs::read_to_string(dirs::config_dir().unwrap_or_else(|| h.join(".config")).join("user-dirs.dirs")).unwrap_or_default();
        Self::parse(&text, &h)
    }

    pub fn parse(text: &str, home: &Path) -> UserDirs {
        let get = |key: &str, fallback: &str| -> PathBuf {
            for line in text.lines() {
                let line = line.trim();
                if let Some(v) = line.strip_prefix(key).and_then(|r| r.strip_prefix('=')) {
                    let v = v.trim().trim_matches('"');
                    let p = if let Some(rest) = v.strip_prefix("$HOME") { home.join(rest.trim_start_matches('/')) } else { PathBuf::from(v) };
                    if p.is_absolute() && p != home {
                        return p;
                    }
                }
            }
            home.join(fallback)
        };
        UserDirs { pictures: get("XDG_PICTURES_DIR", "Bilder"), videos: get("XDG_VIDEOS_DIR", "Videos"), documents: get("XDG_DOCUMENTS_DIR", "Dokumente") }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_dirs_from_file() {
        let h = Path::new("/home/p");
        let d = UserDirs::parse("# x\nXDG_PICTURES_DIR=\"$HOME/Bilder\"\nXDG_VIDEOS_DIR=\"/data/Filme\"\nXDG_DOCUMENTS_DIR=\"$HOME/\"\n", h);
        assert_eq!(d.pictures, PathBuf::from("/home/p/Bilder"));
        assert_eq!(d.videos, PathBuf::from("/data/Filme"));
        // auf $HOME selbst zeigt KDE, wenn der Ordner abgeschaltet ist: dann der Standardname
        assert_eq!(d.documents, PathBuf::from("/home/p/Dokumente"));
    }

    #[test]
    fn accent_check() {
        assert!(Config::valid_accent("#FF7A2F"));
        assert!(!Config::valid_accent("FF7A2F"));
        assert!(!Config::valid_accent("#FF7A2G"));
    }
}
