//! Was beim Abgleich passieren soll – rein und ohne Netz/Dateisystem, damit testbar.
//!
//! Drei Stände je Pfad: lokal (L), Cloud (R) und der Stand nach dem letzten Abgleich (S).
//! Pfade werden ohne Groß-/Kleinschreibung verglichen, weil PlexCloud „a.jpg“ und „A.jpg“ als gleich ansieht.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq)]
pub struct Local {
    /// Pfad relativ zum lokalen Ordner, so wie er auf der Platte heißt
    pub rel: String,
    pub dir: bool,
    pub size: u64,
    pub mtime: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Remote {
    pub rel: String,
    pub dir: bool,
    pub item_id: String,
    pub size: u64,
    pub updated: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Entry {
    pub rel: String,
    #[serde(default)]
    pub dir: bool,
    pub item_id: String,
    pub size: u64,
    pub mtime: i64,
    pub updated: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Neu hochladen, oder als neue Version von `replace`
    Upload { rel: String, size: u64, replace: Option<String> },
    Download { rel: String, item_id: String, size: u64 },
    TrashRemote { rel: String, item_id: String, dir: bool },
    TrashLocal { rel: String, dir: bool },
    /// Beide Seiten geändert: lokale Fassung als Konfliktkopie behalten, Cloud-Fassung holen
    Conflict { rel: String, item_id: String, size: u64 },
    MkdirLocal { rel: String },
    MkdirRemote { rel: String },
    /// Beide Seiten gleich: nur den Stand merken
    Record { rel: String },
    Forget { key: String },
}

impl Action {
    pub fn is_delete(&self) -> bool {
        matches!(self, Action::TrashRemote { .. } | Action::TrashLocal { .. })
    }
}

pub fn key(rel: &str) -> String {
    rel.to_lowercase()
}

fn parent_keys(k: &str) -> impl Iterator<Item = &str> {
    k.match_indices('/').map(move |(i, _)| &k[..i])
}

pub fn plan(local: &[Local], remote: &[Remote], state: &BTreeMap<String, Entry>) -> Vec<Action> {
    let l: BTreeMap<String, &Local> = local.iter().map(|x| (key(&x.rel), x)).collect();
    let r: BTreeMap<String, &Remote> = remote.iter().map(|x| (key(&x.rel), x)).collect();
    let keys: BTreeSet<&String> = l.keys().chain(r.keys()).chain(state.keys()).collect();
    let mut out = Vec::new();

    for k in keys {
        let (lo, re, st) = (l.get(k), r.get(k), state.get(k));
        let act = match (lo, re, st) {
            (Some(lo), Some(re), _) if lo.dir || re.dir => {
                if lo.dir && re.dir {
                    if st.is_none() { Some(Action::Record { rel: lo.rel.clone() }) } else { None }
                } else {
                    // Datei hier, Ordner dort: nicht anfassen, im Status melden
                    None
                }
            }
            (Some(lo), Some(re), Some(s)) => {
                let lch = lo.size != s.size || lo.mtime != s.mtime;
                let rch = re.item_id != s.item_id || re.updated != s.updated || re.size != s.size;
                match (lch, rch) {
                    (false, false) => None,
                    (true, false) => Some(Action::Upload { rel: lo.rel.clone(), size: lo.size, replace: Some(re.item_id.clone()) }),
                    (false, true) => Some(Action::Download { rel: lo.rel.clone(), item_id: re.item_id.clone(), size: re.size }),
                    (true, true) if lo.size == re.size => Some(Action::Record { rel: lo.rel.clone() }),
                    (true, true) => Some(Action::Conflict { rel: lo.rel.clone(), item_id: re.item_id.clone(), size: re.size }),
                }
            }
            (Some(lo), Some(re), None) => {
                if lo.size == re.size { Some(Action::Record { rel: lo.rel.clone() }) } else { Some(Action::Conflict { rel: lo.rel.clone(), item_id: re.item_id.clone(), size: re.size }) }
            }
            (Some(lo), None, Some(s)) => {
                // in der Cloud gelöscht: lokal in den Papierkorb, außer lokal seitdem geändert
                if !lo.dir && (lo.size != s.size || lo.mtime != s.mtime) {
                    Some(Action::Upload { rel: lo.rel.clone(), size: lo.size, replace: None })
                } else {
                    Some(Action::TrashLocal { rel: lo.rel.clone(), dir: lo.dir })
                }
            }
            (Some(lo), None, None) => Some(if lo.dir { Action::MkdirRemote { rel: lo.rel.clone() } } else { Action::Upload { rel: lo.rel.clone(), size: lo.size, replace: None } }),
            (None, Some(re), Some(s)) => {
                // lokal gelöscht: in der Cloud in den Papierkorb, außer dort seitdem geändert
                if !re.dir && (re.item_id != s.item_id || re.updated != s.updated) {
                    Some(Action::Download { rel: re.rel.clone(), item_id: re.item_id.clone(), size: re.size })
                } else {
                    Some(Action::TrashRemote { rel: re.rel.clone(), item_id: re.item_id.clone(), dir: re.dir })
                }
            }
            (None, Some(re), None) => Some(if re.dir { Action::MkdirLocal { rel: re.rel.clone() } } else { Action::Download { rel: re.rel.clone(), item_id: re.item_id.clone(), size: re.size } }),
            (None, None, Some(_)) => Some(Action::Forget { key: k.clone() }),
            (None, None, None) => None,
        };
        out.extend(act);
    }

    // Ordner nicht löschen, wenn darin noch etwas bleibt oder neu dazukommt; dann den Ordner auf der anderen Seite anlegen
    let keep: BTreeSet<String> = out
        .iter()
        .filter_map(|a| match a {
            Action::Upload { rel, .. } | Action::Download { rel, .. } | Action::Conflict { rel, .. } | Action::MkdirLocal { rel } | Action::MkdirRemote { rel } => Some(key(rel)),
            _ => None,
        })
        .flat_map(|k| parent_keys(&k).map(String::from).collect::<Vec<_>>())
        .collect();
    let out: Vec<Action> = out
        .into_iter()
        .map(|a| match a {
            Action::TrashRemote { rel, dir: true, .. } if keep.contains(&key(&rel)) => Action::MkdirLocal { rel },
            Action::TrashLocal { rel, dir: true } if keep.contains(&key(&rel)) => Action::MkdirRemote { rel },
            a => a,
        })
        .collect();

    // Löschungen unterhalb eines gelöschten Ordners erledigt der Ordner mit
    let gone: BTreeSet<String> = out
        .iter()
        .filter_map(|a| match a {
            Action::TrashRemote { rel, dir: true, .. } | Action::TrashLocal { rel, dir: true } => Some(key(rel)),
            _ => None,
        })
        .collect();
    out.into_iter()
        .filter(|a| match a {
            Action::TrashRemote { rel, .. } | Action::TrashLocal { rel, .. } => !parent_keys(&key(rel)).any(|p| gone.contains(p)),
            _ => true,
        })
        .collect()
}

/// Sicherung gegen Massenlöschung (z. B. Platte nicht eingehängt, Ordner versehentlich geleert)
pub fn too_many_deletes(actions: &[Action], known: usize) -> Option<usize> {
    let n = actions.iter().filter(|a| a.is_delete()).count();
    (n > 20 && n * 10 > known * 3).then_some(n)
}

/// „Bericht.odt“ → „Bericht (Konflikt 2026-11-16).odt“
pub fn conflict_name(rel: &str, date: &str) -> String {
    let (dir, name) = match rel.rfind('/') {
        Some(i) => (&rel[..=i], &rel[i + 1..]),
        None => ("", rel),
    };
    match name.rfind('.') {
        Some(i) if i > 0 => format!("{dir}{} (Konflikt {date}){}", &name[..i], &name[i..]),
        _ => format!("{dir}{name} (Konflikt {date})"),
    }
}

/// Einfache Muster wie in den Einstellungen: „.cache“, „*.tmp“, „~$*“; gilt für jeden Teil des Pfads
pub fn excluded(rel: &str, patterns: &[String]) -> bool {
    rel.split('/').any(|part| {
        part.ends_with(".plexcloud-part") || patterns.iter().any(|p| wildcard(p, part))
    })
}

fn wildcard(p: &str, s: &str) -> bool {
    let parts: Vec<&str> = p.split('*').collect();
    if parts.len() == 1 {
        return p == s;
    }
    let mut rest = s;
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            if !rest.starts_with(part) {
                return false;
            }
            rest = &rest[part.len()..];
        } else if i == parts.len() - 1 {
            return rest.ends_with(part);
        } else if let Some(at) = rest.find(part) {
            rest = &rest[at + part.len()..];
        } else {
            return false;
        }
    }
    true
}

/// Monatsordner („2026-11“) → Jahr; sonst None
pub fn month_to_year(name: &str) -> Option<&str> {
    let b = name.trim().as_bytes();
    (b.len() == 7 && b[4] == b'-' && b.iter().enumerate().all(|(i, c)| i == 4 || c.is_ascii_digit())).then(|| &name.trim()[..4])
}

/// Darf dieser Pfad unter Windows angelegt werden? (verbotene Zeichen, reservierte Namen, Punkt/Leerzeichen am Ende)
pub fn windows_ok(rel: &str) -> bool {
    rel.split('/').all(|seg| {
        let stem = seg.split('.').next().unwrap_or("").trim_end().to_ascii_uppercase();
        let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL") || (stem.len() == 4 && (stem.starts_with("COM") || stem.starts_with("LPT")) && stem.as_bytes()[3].is_ascii_digit());
        !seg.is_empty() && !reserved && !seg.ends_with('.') && !seg.ends_with(' ') && !seg.chars().any(|c| matches!(c, '<' | '>' | ':' | '"' | '\\' | '|' | '?' | '*') || (c as u32) < 32)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lf(rel: &str, size: u64, mtime: i64) -> Local {
        Local { rel: rel.into(), dir: false, size, mtime }
    }
    fn ld(rel: &str) -> Local {
        Local { rel: rel.into(), dir: true, size: 0, mtime: 0 }
    }
    fn rf(rel: &str, id: &str, size: u64, up: &str) -> Remote {
        Remote { rel: rel.into(), dir: false, item_id: id.into(), size, updated: up.into() }
    }
    fn rd(rel: &str, id: &str) -> Remote {
        Remote { rel: rel.into(), dir: true, item_id: id.into(), size: 0, updated: "t".into() }
    }
    fn st(entries: &[(&str, &str, u64, i64, &str)]) -> BTreeMap<String, Entry> {
        entries.iter().map(|(rel, id, size, mtime, up)| (key(rel), Entry { rel: rel.to_string(), dir: false, item_id: id.to_string(), size: *size, mtime: *mtime, updated: up.to_string() })).collect()
    }

    #[test]
    fn first_sync_matches_same_files_and_fills_gaps() {
        let a = plan(&[lf("2026/a.jpg", 10, 1), lf("2026/nur-lokal.jpg", 5, 1)], &[rd("2026", "f"), rf("2026/A.JPG", "1", 10, "t"), rf("2026/nur-cloud.jpg", "2", 7, "t")], &BTreeMap::new());
        assert!(a.contains(&Action::Record { rel: "2026/a.jpg".into() }));
        assert!(a.contains(&Action::Upload { rel: "2026/nur-lokal.jpg".into(), size: 5, replace: None }));
        assert!(a.contains(&Action::Download { rel: "2026/nur-cloud.jpg".into(), item_id: "2".into(), size: 7 }));
        assert!(a.contains(&Action::MkdirLocal { rel: "2026".into() }));
        assert!(!a.iter().any(|x| x.is_delete()));
    }

    #[test]
    fn first_sync_different_size_is_conflict_not_overwrite() {
        let a = plan(&[lf("x.odt", 10, 1)], &[rf("x.odt", "1", 11, "t")], &BTreeMap::new());
        assert_eq!(a, vec![Action::Conflict { rel: "x.odt".into(), item_id: "1".into(), size: 11 }]);
    }

    #[test]
    fn changes_go_the_right_way() {
        let s = st(&[("a", "1", 10, 1, "t1"), ("b", "2", 10, 1, "t1"), ("c", "3", 10, 1, "t1")]);
        let a = plan(&[lf("a", 12, 2), lf("b", 10, 1), lf("c", 10, 1)], &[rf("a", "1", 10, "t1"), rf("b", "2", 11, "t2"), rf("c", "3", 10, "t1")], &s);
        assert_eq!(a, vec![Action::Upload { rel: "a".into(), size: 12, replace: Some("1".into()) }, Action::Download { rel: "b".into(), item_id: "2".into(), size: 11 }]);
    }

    #[test]
    fn deletions_go_to_trash_on_the_other_side() {
        let s = st(&[("lokal-weg", "1", 10, 1, "t"), ("cloud-weg", "2", 10, 1, "t"), ("beide-weg", "3", 1, 1, "t")]);
        let a = plan(&[lf("cloud-weg", 10, 1)], &[rf("lokal-weg", "1", 10, "t")], &s);
        assert_eq!(a, vec![
            Action::Forget { key: "beide-weg".into() },
            Action::TrashLocal { rel: "cloud-weg".into(), dir: false },
            Action::TrashRemote { rel: "lokal-weg".into(), item_id: "1".into(), dir: false },
        ]);
    }

    #[test]
    fn edited_file_survives_delete_on_other_side() {
        let s = st(&[("a", "1", 10, 1, "t"), ("b", "2", 10, 1, "t")]);
        let a = plan(&[lf("a", 99, 5)], &[rf("b", "2", 10, "t9")], &s);
        assert!(a.contains(&Action::Upload { rel: "a".into(), size: 99, replace: None }));
        assert!(a.contains(&Action::Download { rel: "b".into(), item_id: "2".into(), size: 10 }));
    }

    #[test]
    fn folder_delete_covers_children_but_not_new_files() {
        let mut s = st(&[("Urlaub/a.jpg", "1", 1, 1, "t"), ("Alt/b.jpg", "2", 1, 1, "t")]);
        s.insert("urlaub".into(), Entry { rel: "Urlaub".into(), dir: true, item_id: "f1".into(), size: 0, mtime: 0, updated: "t".into() });
        s.insert("alt".into(), Entry { rel: "Alt".into(), dir: true, item_id: "f2".into(), size: 0, mtime: 0, updated: "t".into() });
        // lokal ist Urlaub weg; in der Cloud kam Urlaub/neu.jpg dazu. Alt ist lokal komplett weg.
        let a = plan(&[], &[rd("Urlaub", "f1"), rf("Urlaub/a.jpg", "1", 1, "t"), rf("Urlaub/neu.jpg", "9", 3, "t"), rd("Alt", "f2"), rf("Alt/b.jpg", "2", 1, "t")], &s);
        assert!(a.contains(&Action::TrashRemote { rel: "Alt".into(), item_id: "f2".into(), dir: true }));
        assert!(!a.contains(&Action::TrashRemote { rel: "Alt/b.jpg".into(), item_id: "2".into(), dir: false }));
        assert!(a.contains(&Action::MkdirLocal { rel: "Urlaub".into() }));
        assert!(a.contains(&Action::Download { rel: "Urlaub/neu.jpg".into(), item_id: "9".into(), size: 3 }));
        assert!(a.contains(&Action::TrashRemote { rel: "Urlaub/a.jpg".into(), item_id: "1".into(), dir: false }));
        let _ = ld("x");
    }

    #[test]
    fn mass_delete_guard() {
        let many: Vec<Action> = (0..30).map(|i| Action::TrashRemote { rel: format!("{i}"), item_id: "x".into(), dir: false }).collect();
        assert_eq!(too_many_deletes(&many, 40), Some(30));
        assert_eq!(too_many_deletes(&many, 1000), None);
        assert_eq!(too_many_deletes(&many[..5], 5), None);
    }

    #[test]
    fn helpers() {
        assert_eq!(conflict_name("Büro/Bericht.odt", "2026-11-16"), "Büro/Bericht (Konflikt 2026-11-16).odt");
        assert_eq!(conflict_name("README", "d"), "README (Konflikt d)");
        let ex: Vec<String> = vec![".cache".into(), "*.tmp".into(), "~$*".into()];
        assert!(excluded("a/.cache/x", &ex));
        assert!(excluded("x.tmp", &ex));
        assert!(excluded("Büro/~$Brief.docx", &ex));
        assert!(excluded("a/.x.jpg.plexcloud-part", &ex));
        assert!(!excluded("Büro/Brief.docx", &ex));
        assert_eq!(month_to_year("2026-11"), Some("2026"));
        assert_eq!(month_to_year("2026"), None);
        assert_eq!(month_to_year("Urlaub-11"), None);
    }

    #[test]
    fn windows_names() {
        assert!(windows_ok("2026/IMG_1.jpg"));
        assert!(!windows_ok("2026/a:b.txt"));
        assert!(!windows_ok("CON.txt"));
        assert!(!windows_ok("ordner./x"));
        assert!(windows_ok("Konsole.txt"));
    }
}
