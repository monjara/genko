#!/usr/bin/env bash

set -euo pipefail

build_flag="--release"
target_dir="release"
target_triple=""
can_code_sign=false
keychain_name="soukou-signing.keychain-db"
keychain_password=""
signing_identity=""
notarization_key_file=""

help_info() {
  echo "
Usage: ${0##*/} [options] [target]
Build a macOS .app bundle and .dmg for Soukou.

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

cleanup() {
  if [[ -n "$notarization_key_file" && -f "$notarization_key_file" ]]; then
    rm -f "$notarization_key_file"
  fi

  if security list-keychains | grep -q "$keychain_name"; then
    security delete-keychain "$keychain_name" || true
  fi
}

trap cleanup EXIT

decode_secret_to_file() {
  local secret_value=$1
  local output_path=$2

  if [[ "$secret_value" == *"-----BEGIN "* ]]; then
    printf '%s' "$secret_value" > "$output_path"
  else
    printf '%s' "$secret_value" | base64 --decode > "$output_path"
  fi
}

setup_signing() {
  if [[ -z "${MACOS_CERTIFICATE:-}" || -z "${MACOS_CERTIFICATE_PASSWORD:-}" ]]; then
    echo "MACOS_CERTIFICATE or MACOS_CERTIFICATE_PASSWORD is missing; skipping codesign."
    return
  fi

  local certificate_file
  certificate_file="$(mktemp /tmp/soukou-certificate.XXXXXX.p12)"
  decode_secret_to_file "$MACOS_CERTIFICATE" "$certificate_file"

  keychain_password="$(openssl rand -hex 24)"

  security create-keychain -p "$keychain_password" "$keychain_name"
  security set-keychain-settings -lut 21600 "$keychain_name"
  security unlock-keychain -p "$keychain_password" "$keychain_name"
  security list-keychains -d user -s "$keychain_name" login.keychain-db
  security import "$certificate_file" \
    -k "$keychain_name" \
    -P "$MACOS_CERTIFICATE_PASSWORD" \
    -T /usr/bin/codesign \
    -T /usr/bin/security
  security set-key-partition-list \
    -S apple-tool:,apple:,codesign: \
    -s \
    -k "$keychain_password" \
    "$keychain_name"

  rm -f "$certificate_file"

  if [[ -n "${APPLE_TEAM_ID:-}" ]]; then
    signing_identity="$(
      security find-identity -v -p codesigning "$keychain_name" \
        | grep "Developer ID Application" \
        | grep "$APPLE_TEAM_ID" \
        | head -n 1 \
        | sed -E 's/.*"(.+)"/\1/'
    )"
  else
    signing_identity="$(
      security find-identity -v -p codesigning "$keychain_name" \
        | grep "Developer ID Application" \
        | head -n 1 \
        | sed -E 's/.*"(.+)"/\1/'
    )"
  fi

  if [[ -z "$signing_identity" ]]; then
    echo "Developer ID Application identity was not found in imported certificate." >&2
    exit 1
  fi

  can_code_sign=true
}

sign_path() {
  local path_to_sign=$1

  if [[ "$can_code_sign" != true ]]; then
    return
  fi

  /usr/bin/codesign \
    --force \
    --deep \
    --timestamp \
    --options runtime \
    --sign "$signing_identity" \
    "$path_to_sign" \
    -v
}

notarize_and_staple() {
  local path_to_notarize=$1

  if [[ "$can_code_sign" != true ]]; then
    return
  fi

  if [[ -z "${APPLE_NOTARIZATION_KEY:-}" || -z "${APPLE_NOTARIZATION_KEY_ID:-}" || -z "${APPLE_NOTARIZATION_ISSUER_ID:-}" ]]; then
    echo "Notarization secrets are missing; skipping notarization." >&2
    return
  fi

  notarization_key_file="$(mktemp /tmp/soukou-notary-key.XXXXXX.p8)"
  decode_secret_to_file "$APPLE_NOTARIZATION_KEY" "$notarization_key_file"

  xcrun notarytool submit \
    --wait \
    --key "$notarization_key_file" \
    --key-id "$APPLE_NOTARIZATION_KEY_ID" \
    --issuer "$APPLE_NOTARIZATION_ISSUER_ID" \
    "$path_to_notarize"

  xcrun stapler staple "$path_to_notarize"

  rm -f "$notarization_key_file"
  notarization_key_file=""
}

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

setup_signing

echo "Building Soukou for $target_triple"
cargo build ${build_flag} --package soukou --target "$target_triple"

echo "Bundling Soukou.app"
cargo bundle ${build_flag} \
  --package soukou \
  --target "$target_triple"

app_path="target/${target_triple}/${target_dir}/bundle/osx/Soukou.app"

if [[ ! -d "$app_path" ]]; then
  echo "Expected app bundle was not created: $app_path" >&2
  exit 1
fi

if [[ "$target_dir" != "debug" ]]; then
  echo "Signing Soukou.app"
  sign_path "$app_path"
fi

bundle_directory="target/${target_triple}/${target_dir}"
dmg_staging_dir="${bundle_directory}/dmg"
dmg_path="${bundle_directory}/Soukou-${arch_suffix}.dmg"

rm -rf "$dmg_staging_dir"
mkdir -p "$dmg_staging_dir"
cp -R "$app_path" "$dmg_staging_dir/"
ln -s /Applications "${dmg_staging_dir}/Applications"

echo "Creating DMG at $dmg_path"
rm -f "$dmg_path"
hdiutil create \
  -volname "Soukou" \
  -srcfolder "$dmg_staging_dir" \
  -ov \
  -format UDZO \
  "$dmg_path"

if [[ "$target_dir" != "debug" ]]; then
  echo "Signing DMG"
  sign_path "$dmg_path"

  echo "Notarizing DMG"
  notarize_and_staple "$dmg_path"

  echo "Stapling Soukou.app"
  xcrun stapler staple "$app_path"
fi

echo "Created app bundle: $app_path"
echo "Created disk image: $dmg_path"
