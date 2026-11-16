//! Selbst-Update aus den GitHub-Releases (wie die Handy-App): neue Version finden, Paket laden,
//! Prüfsumme kontrollieren, Programm austauschen, neu starten. Geht nur, wenn das Programm dem Benutzer gehört
//! (~/.local/bin); liegt es unter /usr/bin (pacman), meldet die App das Update nur.

use crate::REPO;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};

/// Exit-Code für „bitte mit der neuen Version neu starten“ (systemd: RestartForceExitStatus=75)
pub const RESTART_CODE: i32 = 75;

#[derive(serde::Serialize, Clone, Debug)]
pub struct UpdateInfo {
    pub version: String,
    pub url: String,
    /// kann sich selbst aktualisieren (Programm gehört dem Benutzer)
    pub self_update: bool,
    #[serde(skip)]
    tarball: Option<String>,
    #[serde(skip)]
    checksum: Option<String>,
}

pub fn newer(a: &str, b: &str) -> bool {
    let p = |s: &str| s.split('.').map(|x| x.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    p(a) > p(b)
}

/// Darf diese Programmdatei ersetzt werden? Nur wenn sie im Home liegt und beschreibbar ist.
pub fn can_replace(exe: &Path) -> bool {
    let home = crate::config::home();
    exe.starts_with(&home) && exe.parent().map(|d| std::fs::metadata(d).map(|m| !m.permissions().readonly()).unwrap_or(false)).unwrap_or(false) && !exe.components().any(|c| c.as_os_str() == "target")
}

pub async fn check(http: &reqwest::Client) -> Option<UpdateInfo> {
    let j: serde_json::Value = http.get(format!("https://api.github.com/repos/{REPO}/releases/latest")).header("User-Agent", "PlexCloud-Linux").send().await.ok()?.json().await.ok()?;
    let tag = j["tag_name"].as_str()?.trim_start_matches('v').to_string();
    if !newer(&tag, env!("CARGO_PKG_VERSION")) {
        return None;
    }
    let assets = j["assets"].as_array().cloned().unwrap_or_default();
    let find = |suffix: &str| assets.iter().find(|a| a["name"].as_str().map(|n| n.ends_with(suffix)).unwrap_or(false)).and_then(|a| a["browser_download_url"].as_str()).map(String::from);
    let exe = std::env::current_exe().ok();
    Some(UpdateInfo {
        url: j["html_url"].as_str().unwrap_or("").to_string(),
        self_update: exe.as_deref().map(can_replace).unwrap_or(false),
        tarball: find("-x86_64.tar.gz"),
        checksum: find("-x86_64.tar.gz.sha256"),
        version: tag,
    })
}

/// Paket laden, prüfen, Programm ersetzen. Danach muss neu gestartet werden.
pub async fn install(http: &reqwest::Client, u: &UpdateInfo) -> anyhow::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    if !can_replace(&exe) {
        anyhow::bail!("Programm liegt nicht im Benutzerordner; bitte über das Paket aktualisieren.");
    }
    let (Some(tar_url), Some(sum_url)) = (&u.tarball, &u.checksum) else { anyhow::bail!("Release ohne Paket oder Prüfsumme") };
    let want = http.get(sum_url).header("User-Agent", "PlexCloud-Linux").send().await?.error_for_status()?.text().await?;
    let want = want.split_whitespace().next().unwrap_or("").to_lowercase();
    let data = http.get(tar_url).header("User-Agent", "PlexCloud-Linux").send().await?.error_for_status()?.bytes().await?;
    let got = format!("{:x}", Sha256::digest(&data));
    if want.len() != 64 || got != want {
        anyhow::bail!("Prüfsumme stimmt nicht, Update verworfen.");
    }
    let bin = extract_binary(&data)?;
    let tmp = exe.with_file_name(".plexcloud-desktop.new");
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o755).open(&tmp)?;
        f.write_all(&bin)?;
        f.sync_all()?;
    }
    // rename über die laufende Datei ist unter Linux sicher: der laufende Prozess behält die alte
    std::fs::rename(&tmp, &exe)?;
    Ok(exe)
}

fn extract_binary(tgz: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut ar = tar::Archive::new(flate2::read::GzDecoder::new(tgz));
    for e in ar.entries()? {
        let mut e = e?;
        let p = e.path()?.to_path_buf();
        if p.file_name().and_then(|n| n.to_str()) == Some("plexcloud-desktop") {
            let mut v = Vec::new();
            e.read_to_end(&mut v)?;
            if v.len() < 1_000_000 || &v[..4] != b"\x7fELF" {
                anyhow::bail!("Paket enthält kein gültiges Programm");
            }
            return Ok(v);
        }
    }
    anyhow::bail!("Programm nicht im Paket gefunden")
}

/// Läuft dieser Prozess als unser systemd-Dienst? (INVOCATION_ID allein reicht nicht: auch Terminals und Startmenü-Starts haben sie)
pub fn under_service() -> bool {
    std::fs::read_to_string("/proc/self/cgroup").map(|c| c.lines().any(|l| l.ends_with("/plexcloud.service"))).unwrap_or(false)
}

/// Neu starten: unter systemd mit Code 75 beenden (systemd startet neu), sonst neuen Prozess starten
pub fn restart(exe: &Path, hidden: bool) -> ! {
    if !under_service() {
        // kurz warten, bis dieser Prozess weg ist: sonst hält die neue Instanz ihn für schon laufend und beendet sich
        use std::os::unix::process::CommandExt;
        // eigene Sitzung (setsid), damit der Neustart das Ende dieses Prozesses überlebt
        let r = std::process::Command::new("setsid")
            .arg("-f")
            .arg("sh")
            .arg("-c")
            .arg(if hidden { "sleep 1; exec \"$0\" --hidden" } else { "sleep 1; exec \"$0\"" })
            .arg(exe)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .process_group(0)
            .status();
        eprintln!("PlexCloud: Neustart angestoßen ({r:?})");
        std::process::exit(0);
    }
    std::process::exit(RESTART_CODE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_a_service_in_tests() {
        assert!(!under_service());
    }

    #[test]
    fn version_compare() {
        assert!(newer("0.1.10", "0.1.9"));
        assert!(newer("1.0.0", "0.9.9"));
        assert!(!newer("0.1.0", "0.1.0"));
    }

    #[test]
    fn only_user_owned_binaries_update_themselves() {
        assert!(!can_replace(Path::new("/usr/bin/plexcloud-desktop")));
        assert!(!can_replace(&crate::config::home().join("Dev/x/src-tauri/target/release/plexcloud-desktop")));
    }

    #[test]
    fn extract_rejects_non_elf() {
        let mut b = tar::Builder::new(flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast()));
        let data = vec![0u8; 2_000_000];
        let mut h = tar::Header::new_gnu();
        h.set_size(data.len() as u64);
        h.set_mode(0o755);
        h.set_cksum();
        b.append_data(&mut h, "plexcloud-desktop", &data[..]).unwrap();
        let tgz = b.into_inner().unwrap().finish().unwrap();
        assert!(extract_binary(&tgz).is_err());
    }
}
