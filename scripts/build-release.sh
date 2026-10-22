#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "Для установщика версия должна иметь формат x.y.z, получено: $VERSION" >&2
    exit 1
fi

TARGET="x86_64-pc-windows-gnu"
EXE="$ROOT/target/$TARGET/release/crosshair-overlay.exe"
DIST="$ROOT/dist"
ZIP="$DIST/CrosshairOverlay-$VERSION-portable.zip"
INSTALLER="$DIST/CrosshairOverlay-Setup-$VERSION.exe"
NSI="$ROOT/installer/crosshair-overlay.nsi"
MAKENSIS="${MAKENSIS:-makensis}"

cargo test --locked
cargo zigbuild --release --locked --target "$TARGET"

rm -rf "$DIST"
mkdir -p "$DIST/stage"
cp "$EXE" "$DIST/stage/crosshair-overlay.exe"
cp LICENSE "$DIST/stage/LICENSE.txt"
cp README.md "$DIST/stage/README.md"

python3 - "$ZIP" "$DIST/stage" <<'PY'
import pathlib
import sys
import zipfile

archive = pathlib.Path(sys.argv[1])
stage = pathlib.Path(sys.argv[2])
with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as bundle:
    for path in sorted(stage.iterdir()):
        bundle.write(path, arcname=path.name)
PY

rm -rf "$DIST/stage"

if ! command -v "$MAKENSIS" >/dev/null 2>&1; then
    echo "NSIS не найден. Установите makensis или задайте MAKENSIS=/path/to/makensis." >&2
    exit 1
fi

"$MAKENSIS" -V3 \
    "-DAPP_VERSION=$VERSION" \
    "-DAPP_EXE=$EXE" \
    "-DICON_FILE=$ROOT/assets/icon.ico" \
    "-DLICENSE_FILE=$ROOT/LICENSE" \
    "-DOUT_FILE=$INSTALLER" \
    "$NSI"

sha256sum "$INSTALLER" "$ZIP" | sed "s|$DIST/||" > "$DIST/SHA256SUMS.txt"

printf '\nГотово:\n  %s\n  %s\n  %s\n' "$INSTALLER" "$ZIP" "$DIST/SHA256SUMS.txt"
