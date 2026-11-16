#!/usr/bin/env bash
# Installiert PlexCloud für Linux in den Benutzerordner (~/.local), ohne root.
# So kann sich die App später selbst aus GitHub aktualisieren.
#
#   scripts/install.sh                      neueste Version von GitHub
#   scripts/install.sh paket.tar.gz         ein lokales Paket (z. B. releases/…)
set -euo pipefail
die() { echo "❌ $*" >&2; exit 1; }
REPO=peter1965p/PlexCloud-Desktop
for lib in libwebkit2gtk-4.1.so.0 libayatana-appindicator3.so.1; do
  ldconfig -p | grep -F "$lib" >/dev/null || die "$lib fehlt. Bitte: sudo pacman -S webkit2gtk-4.1 libayatana-appindicator"
done

TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
if [[ -n "${1:-}" ]]; then
  cp "$1" "$TMP/p.tar.gz"
else
  URL="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | grep -o '"browser_download_url": *"[^"]*x86_64.tar.gz"' | cut -d'"' -f4)"
  [[ -n "$URL" ]] || die "Kein Release gefunden."
  curl -fsSL "$URL" -o "$TMP/p.tar.gz"
  curl -fsSL "$URL.sha256" -o "$TMP/p.sha256"
  [[ "$(sha256sum "$TMP/p.tar.gz" | cut -d' ' -f1)" == "$(cut -d' ' -f1 "$TMP/p.sha256")" ]] || die "Prüfsumme stimmt nicht."
fi
tar -xzf "$TMP/p.tar.gz" -C "$TMP"

BIN="$HOME/.local/bin"; APPS="${XDG_DATA_HOME:-$HOME/.local/share}/applications"; ICONS="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/512x512/apps"
mkdir -p "$BIN" "$APPS" "$ICONS"
install -m755 "$TMP/plexcloud-desktop" "$BIN/plexcloud-desktop.new" && mv -f "$BIN/plexcloud-desktop.new" "$BIN/plexcloud-desktop"
sed "s|^Exec=.*|Exec=$BIN/plexcloud-desktop|" "$TMP/plexcloud.desktop" > "$APPS/plexcloud.desktop"
install -m644 "$TMP/plexcloud.png" "$ICONS/plexcloud.png"
command -v update-desktop-database >/dev/null && update-desktop-database -q "$APPS" || true
echo "✅ $("$BIN/plexcloud-desktop" --version) installiert: $BIN/plexcloud-desktop"
echo "   Starten über das Startmenü (PlexCloud) oder: plexcloud-desktop"
