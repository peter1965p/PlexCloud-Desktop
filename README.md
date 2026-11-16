# PlexCloud für Linux

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

## Entwickeln

```sh
npm install
npx tauri dev          # App mit Live-Reload
cd src-tauri && cargo test --lib
scripts/release.sh 0.1.1
```

Tauri 2 (Rust) + Vue 3. Einstellungen: `~/.config/plexcloud/`, Abgleichstand: `~/.local/share/plexcloud/`.
