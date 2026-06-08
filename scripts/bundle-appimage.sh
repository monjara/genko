#!/usr/bin/env bash

set -euo pipefail

target_triple="${1:-x86_64-unknown-linux-gnu}"

case "$target_triple" in
  x86_64-unknown-linux-gnu)
    archive_arch="x86_64"
    linuxdeploy_arch="x86_64"
    ;;
  aarch64-unknown-linux-gnu)
    archive_arch="aarch64"
    linuxdeploy_arch="aarch64"
    ;;
  *)
    echo "Unsupported target: $target_triple" >&2
    exit 1
    ;;
esac

binary_path="target/${target_triple}/release/soukou"
appdir_path="target/${target_triple}/release/AppDir"
desktop_file_source="crates/soukou/resources/linux/soukou.desktop"
icon_source="crates/soukou/resources/AppIcon.iconset/icon_512x512.png"
icon_staging_path="target/${target_triple}/release/soukou.png"
linuxdeploy_path="tools/linuxdeploy-${linuxdeploy_arch}.AppImage"
output_name="草稿-${archive_arch}.AppImage"
appimage_path="target/${target_triple}/release/${output_name}"

if [[ ! -x "$linuxdeploy_path" ]]; then
  echo "linuxdeploy AppImage is missing or not executable: $linuxdeploy_path" >&2
  exit 1
fi

rustup target add "$target_triple"

echo "Building 草稿 for $target_triple"
cargo build --release --package soukou --target "$target_triple"

if [[ ! -f "$binary_path" ]]; then
  echo "Expected binary was not created: $binary_path" >&2
  exit 1
fi

rm -rf "$appdir_path"
mkdir -p "$appdir_path"
mkdir -p "${appdir_path}/usr/share/doc/soukou"
cp "$icon_source" "$icon_staging_path"
cp LICENSE.md "${appdir_path}/usr/share/doc/soukou/LICENSE.md"
cp THIRD_PARTY_NOTICES.md "${appdir_path}/usr/share/doc/soukou/THIRD_PARTY_NOTICES.md"
cp THIRD_PARTY_LICENSES.json "${appdir_path}/usr/share/doc/soukou/THIRD_PARTY_LICENSES.json"

export APPIMAGE_EXTRACT_AND_RUN=1
export ARCH="$archive_arch"
export OUTPUT="$appimage_path"
export LDAI_OUTPUT="$output_name"
export LDAI_NO_APPSTREAM=1

"$linuxdeploy_path" \
  --appdir "$appdir_path" \
  --executable "$binary_path" \
  --desktop-file "$desktop_file_source" \
  --icon-file "$icon_staging_path" \
  --output appimage

if [[ ! -f "$appimage_path" ]]; then
  echo "Expected AppImage was not created: $appimage_path" >&2
  exit 1
fi

echo "Created AppImage: $appimage_path"
