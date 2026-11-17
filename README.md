# PlexCloud für Linux und Windows

Desktop-App für [PlexCloud](https://www.plexora.eu/plexcloud) (Plexora). Cloud-Ordner landen in den **echten Systemordnern**, nicht in einem Sammelordner:

| PlexCloud    | Rechner (xdg-user-dirs) |
|--------------|-------------------------|
| `/Bilder`    | `~/Bilder`              |
| `/Videos`    | `~/Videos`              |
| `/Dokumente` | `~/Dokumente`           |

Weitere Cloud-Ordner lassen sich beliebigen Ordnern zuordnen (z. B. `/Musik ⇄ ~/Musik`).

## Installieren (Arch / CachyOS)

```sh
sudo pacman -S --needed webkit2gtk-4.1 libayatana-appindicator
curl -fsSL https://github.com/peter1965p/PlexCloud-Desktop/releases/latest/download/install.sh | bash
```

Installiert nach `~/.local/bin` (ohne root). Von dort **aktualisiert sich die App selbst** aus den GitHub-Releases: Prüfsumme kontrollieren, austauschen, neu starten.
Alternativ systemweit über `packaging/PKGBUILD` (`makepkg -si`); dann meldet die App neue Versionen nur.

## Installieren (Windows 10/11)

1. `PlexCloud_<Version>_x64-setup.exe` aus dem [neuesten Release](https://github.com/peter1965p/PlexCloud-Desktop/releases/latest) laden und starten.
2. Windows zeigt **„Der Computer wurde durch Windows geschützt“**. Das liegt daran, dass PlexCloud (noch) nicht mit einem gekauften Zertifikat signiert ist, nicht an der App.
   Auf **„Weitere Informationen“** klicken, dann auf **„Trotzdem ausführen“**.
3. Der Installer braucht keine Adminrechte und installiert nur für dich. PlexCloud startet danach automatisch mit Windows.

Diese Bestätigung kommt nur einmal. Updates spielt die App selbst ein, ohne erneute Nachfrage.

## Entwickeln

```sh
npm install
npx tauri dev          # App mit Live-Reload
cd src-tauri && cargo test --lib
scripts/release.sh 0.1.1
```

Tauri 2 (Rust) + Vue 3. Einstellungen: `~/.config/plexcloud/`, Abgleichstand: `~/.local/share/plexcloud/`.
