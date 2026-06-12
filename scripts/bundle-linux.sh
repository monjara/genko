#!/usr/bin/env bash

set -euo pipefail

target_triple="${1:-x86_64-unknown-linux-gnu}"

case "$target_triple" in
  x86_64-unknown-linux-gnu)
    archive_arch="x86_64"
    ;;
  aarch64-unknown-linux-gnu)
    archive_arch="aarch64"
    ;;
  *)
    echo "Unsupported target: $target_triple" >&2
    exit 1
    ;;
esac

archive_root="soukou-linux-${archive_arch}"
bundle_directory="target/${target_triple}/release/${archive_root}"
archive_path="target/${target_triple}/release/${archive_root}.tar.gz"
binary_path="target/${target_triple}/release/soukou"
desktop_file_source="crates/soukou/resources/linux/soukou.desktop"
icon_source="crates/soukou/resources/AppIcon.iconset/icon_512x512.png"

rustup target add "$target_triple"

echo "Building 草稿 for $target_triple"
cargo build --release --package soukou --target "$target_triple"

if [[ ! -f "$binary_path" ]]; then
  echo "Expected binary was not created: $binary_path" >&2
  exit 1
fi

rm -rf "$bundle_directory"
mkdir -p \
  "${bundle_directory}/bin" \
  "${bundle_directory}/share/applications" \
  "${bundle_directory}/share/icons/hicolor/512x512/apps"

cp "$binary_path" "${bundle_directory}/bin/soukou"
cp "$desktop_file_source" "${bundle_directory}/share/applications/dev.monj.soukou.desktop"
cp "$icon_source" "${bundle_directory}/share/icons/hicolor/512x512/apps/soukou.png"
cp Readme.md "${bundle_directory}/README.md"
cp LICENSE "${bundle_directory}/LICENSE"
cp THIRD_PARTY_NOTICES.md "${bundle_directory}/THIRD_PARTY_NOTICES.md"
cp THIRD_PARTY_LICENSES.json "${bundle_directory}/THIRD_PARTY_LICENSES.json"

rm -f "$archive_path"
tar -czf "$archive_path" -C "target/${target_triple}/release" "$archive_root"

echo "Created Linux archive: $archive_path"
