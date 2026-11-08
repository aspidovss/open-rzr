#!/usr/bin/env bash
# Build open-rzr for Linux (Rust + Tauri).
#   ./packaging/linux/build.sh               # binaries + tar.gz
#   ./packaging/linux/build.sh --bundles     # + .deb and .AppImage (needs tauri-cli)
# Ubuntu/Debian build deps:
#   sudo apt install build-essential curl wget file libssl-dev libxdo-dev \
#        libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev
set -euo pipefail
cd "$(dirname "$0")/../.."
VER=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)

echo "== tests (core)"
cargo test -p openrzr --release
echo "== build $VER"
cargo build --release -p openrzr -p open-rzr
./target/release/open-rzr-cli version
./target/release/open-rzr-cli --mock info >/dev/null
./target/release/orzr-cli --mock -ls >/dev/null

mkdir -p dist
STAGE="dist/open-rzr-$VER"
rm -rf "$STAGE" && mkdir -p "$STAGE"
cp target/release/open-rzr target/release/open-rzr-cli target/release/orzr-cli README.md README.ru.md LICENSE packaging/linux/99-open-rzr.rules "$STAGE/"
tar -C dist -czf "dist/open-rzr-$VER-linux-x86_64.tar.gz" "open-rzr-$VER"
echo "Archive: dist/open-rzr-$VER-linux-x86_64.tar.gz"

if [ "${1:-}" = "--bundles" ]; then
  command -v cargo-tauri >/dev/null || cargo install tauri-cli --version "^2" --locked
  (cd app && cargo tauri build --bundles deb,appimage)
  cp target/release/bundle/deb/*.deb dist/ 2>/dev/null || true
  cp target/release/bundle/appimage/*.AppImage dist/ 2>/dev/null || true
fi
(cd dist && sha256sum -- *.tar.gz *.deb *.AppImage 2>/dev/null > SHA256SUMS.txt || true; cat SHA256SUMS.txt)
