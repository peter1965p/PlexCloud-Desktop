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
cd packaging && makepkg -si
```

Braucht `webkit2gtk-4.1`, `gtk3` und `libayatana-appindicator` (Tray-Symbol in Plasma).

## Entwickeln

```sh
npm install
npx tauri dev          # App mit Live-Reload
cd src-tauri && cargo test --lib
scripts/release.sh 0.1.1
```

Tauri 2 (Rust) + Vue 3. Einstellungen: `~/.config/plexcloud/`, Abgleichstand: `~/.local/share/plexcloud/`.
