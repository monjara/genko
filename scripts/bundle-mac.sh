#!/usr/bin/env bash

set -euo pipefail

build_flag="--release"
target_dir="release"
target_triple=""

help_info() {
  echo "
Usage: ${0##*/} [options] [target]
Build an unsigned macOS .app bundle and .dmg for Genko.

Options:
  -d    Compile in debug mode
  -h    Display this help and exit
  "
}

while getopts 'dh' flag
do
  case "${flag}" in
    d)
      build_flag=""
      target_dir="debug"
      ;;
    h)
      help_info
      exit 0
      ;;
  esac
done

shift $((OPTIND - 1))

if [[ $# -gt 0 && -n "$1" ]]; then
  target_triple="$1"
else
  version_info="$(rustc --version --verbose)"
  host_line="$(echo "$version_info" | grep '^host:')"
  target_triple="${host_line#host: }"
fi

case "$target_triple" in
  aarch64-apple-darwin)
    arch_suffix="aarch64"
    ;;
  x86_64-apple-darwin)
    arch_suffix="x86_64"
    ;;
  *)
    echo "Unsupported target: $target_triple" >&2
    exit 1
    ;;
esac

rustup target add "$target_triple"

export CXXFLAGS="-stdlib=libc++"

echo "Building Genko for $target_triple"
cargo build ${build_flag} --package genko --target "$target_triple"

echo "Bundling Genko.app"
cargo bundle ${build_flag} \
  --package genko \
  --target "$target_triple"

app_path="target/${target_triple}/${target_dir}/bundle/osx/Genko.app"

if [[ ! -d "$app_path" ]]; then
  echo "Expected app bundle was not created: $app_path" >&2
  exit 1
fi

bundle_directory="target/${target_triple}/${target_dir}"
dmg_staging_dir="${bundle_directory}/dmg"
dmg_path="${bundle_directory}/Genko-${arch_suffix}.dmg"

rm -rf "$dmg_staging_dir"
mkdir -p "$dmg_staging_dir"
cp -R "$app_path" "$dmg_staging_dir/"
ln -s /Applications "${dmg_staging_dir}/Applications"

echo "Creating DMG at $dmg_path"
rm -f "$dmg_path"
hdiutil create \
  -volname "Genko" \
  -srcfolder "$dmg_staging_dir" \
  -ov \
  -format UDZO \
  "$dmg_path"

echo "Created app bundle: $app_path"
echo "Created disk image: $dmg_path"
