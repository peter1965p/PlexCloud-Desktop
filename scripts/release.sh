#!/usr/bin/env bash
# Veröffentlicht eine neue Version von PlexCloud für Linux.
#
#   scripts/release.sh 0.1.1
#
# Vorher in CHANGELOG.md einen Abschnitt "## [0.1.1]" anlegen und committen.
# Ablauf: Version setzen -> Tests -> Release-Build -> Tarball -> PKGBUILD anpassen ->
# Commit + Tag -> Push -> GitHub-Release. Installierte Apps (~/.local/bin) spielen das Update selbst ein.
set -euo pipefail
cd "$(dirname "$0")/.."
die() { echo "❌ $*" >&2; exit 1; }

VERSION="${1:-}"
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "Nutzung: $0 <version>, z. B. $0 0.1.1"
TAG="v$VERSION"
[[ -z "$(git status --porcelain)" ]] || die "Es gibt nicht committete Änderungen. Erst committen."
git rev-parse "$TAG" >/dev/null 2>&1 && die "Tag $TAG existiert schon."

NOTES="$(awk -v v="$VERSION" '
    $0 ~ "^## \\[" v "\\]" { found = 1; next }
    found && /^## \[/ { exit }
    found { print }
' CHANGELOG.md | sed -e '/./,$!d')"
[[ -n "$NOTES" ]] || die "Kein Abschnitt \"## [$VERSION]\" in CHANGELOG.md gefunden."

echo "▶ Version $VERSION setzen"
sed -i "0,/^version = \".*\"/s//version = \"$VERSION\"/" src-tauri/Cargo.toml
sed -i "s/\"version\": \"[^\"]*\"/\"version\": \"$VERSION\"/" src-tauri/tauri.conf.json package.json
sed -i "s/^pkgver=.*/pkgver=$VERSION/" packaging/PKGBUILD

echo "▶ Tests und Release-Build"
(cd src-tauri && cargo test --lib --quiet)
npx tauri build --no-bundle
BIN=src-tauri/target/release/plexcloud-desktop
[[ "$("$BIN" --version)" == "plexcloud-desktop $VERSION" ]] || die "Programm meldet nicht Version $VERSION."

echo "▶ Paket"
mkdir -p releases
STAGE="$(mktemp -d)"
cp "$BIN" "$STAGE/plexcloud-desktop"
cp packaging/plexcloud.desktop LICENSE "$STAGE/"
cp assets/icon.png "$STAGE/plexcloud.png"
OUT="releases/plexcloud-desktop-$VERSION-x86_64.tar.gz"
tar -C "$STAGE" -czf "$OUT" .
rm -rf "$STAGE"
SUM="$(sha256sum "$OUT" | cut -d' ' -f1)"
echo "$SUM  $(basename "$OUT")" > "$OUT.sha256"
sed -i "s/^sha256sums=.*/sha256sums=('$SUM')/" packaging/PKGBUILD
echo "▶ $OUT ($(du -h "$OUT" | cut -f1))"

echo "▶ Commit, Tag und Push"
git commit -qam "Version $VERSION"
git tag -a "$TAG" -m "PlexCloud Desktop $VERSION"
git push -q origin HEAD --follow-tags

echo "▶ GitHub-Release"
gh release create "$TAG" "$OUT" "$OUT.sha256" packaging/PKGBUILD scripts/install.sh --title "PlexCloud Desktop $VERSION" --notes "$NOTES"
echo "✅ PlexCloud Desktop $VERSION ist veröffentlicht."
