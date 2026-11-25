#!/usr/bin/env bash
# Build an AppImage from the tested Elaris prefix tree.
# Usage: make-appimage.sh <prefix> <output.AppImage> <appimagetool>
set -euo pipefail

prefix="$1"
out="$2"
appimagetool="$3"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
appdir="$work/Elaris.AppDir"
mkdir -p "$appdir/usr"
cp -a "$prefix/bin" "$prefix/lib" "$prefix/libexec" "$prefix/share" "$appdir/usr/"
cp "$prefix/share/applications/org.elarisbrowser.Elaris.desktop" "$appdir/"
cp "$prefix/share/icons/hicolor/512x512/apps/org.elarisbrowser.Elaris.png" "$appdir/"
cat > "$appdir/AppRun" <<'RUN'
#!/usr/bin/env sh
set -eu
HERE="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
export ELARIS_LADYBIRD_RESOURCE_DIR="$HERE/usr/share/Lagom"
exec "$HERE/usr/bin/elaris" "$@"
RUN
chmod +x "$appdir/AppRun" "$appimagetool"
ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$appimagetool" --no-appstream "$appdir" "$out"
