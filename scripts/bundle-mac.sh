#!/usr/bin/env bash

set -euo pipefail

build_flag="--release"
target_dir="release"
target_triple=""
can_code_sign=false
keychain_name="soukou-signing.keychain-db"
keychain_password=""
signing_identity=""
signing_identity_hash=""
notarization_key_file=""
app_icon_source="crates/soukou/resources/AppIcon.icns"
app_icon_name="AppIcon.icns"

help_info() {
  echo "
Usage: ${0##*/} [options] [target]
Build a macOS .app bundle and .dmg for 草稿.

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
    local normalized_secret
    normalized_secret="$(printf '%s' "$secret_value" | tr -d ' \t\r\n')"

    if printf '%s' "$normalized_secret" | base64 --decode > "$output_path" 2>/dev/null; then
      :
    else
      printf '%s' "$normalized_secret" | base64 -D > "$output_path"
    fi
  fi
}

setup_signing() {
  if [[ -z "${MACOS_CERTIFICATE:-}" || -z "${MACOS_CERTIFICATE_PASSWORD:-}" ]]; then
    echo "MACOS_CERTIFICATE or MACOS_CERTIFICATE_PASSWORD is missing; skipping codesign."
    return
  fi

  local certificate_file
  certificate_file="$(mktemp /tmp/soukou-certificate.XXXXXX)"
  decode_secret_to_file "$MACOS_CERTIFICATE" "$certificate_file"

  if ! openssl pkcs12 -in "$certificate_file" -passin "pass:$MACOS_CERTIFICATE_PASSWORD" -nokeys >/dev/null 2>&1; then
    echo "MACOS_CERTIFICATE is not a valid PKCS#12 archive, or MACOS_CERTIFICATE_PASSWORD does not match." >&2
    rm -f "$certificate_file"
    exit 1
  fi

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

  local identity_line
  if [[ -n "${APPLE_TEAM_ID:-}" ]]; then
    identity_line="$(
      security find-identity -v -p codesigning "$keychain_name" \
        | grep "Developer ID Application" \
        | grep "$APPLE_TEAM_ID" \
        | head -n 1
    )"
  else
    identity_line="$(
      security find-identity -v -p codesigning "$keychain_name" \
        | grep "Developer ID Application" \
        | head -n 1
    )"
  fi

  signing_identity="$(printf '%s\n' "$identity_line" | sed -E 's/.*"(.+)"/\1/')"
  signing_identity_hash="$(printf '%s\n' "$identity_line" | awk '{print $2}')"

  if [[ -z "$signing_identity" || -z "$signing_identity_hash" ]]; then
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
    --keychain "$keychain_name" \
    --timestamp \
    --options runtime \
    --sign "$signing_identity_hash" \
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

  notarization_key_file="$(mktemp /tmp/soukou-notary-key.XXXXXX)"
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

configure_app_bundle_icon() {
  local bundle_path=$1
  local resources_path="${bundle_path}/Contents/Resources"
  local info_plist_path="${bundle_path}/Contents/Info.plist"

  mkdir -p "$resources_path"
  cp "$app_icon_source" "${resources_path}/${app_icon_name}"

  /usr/libexec/PlistBuddy -c "Set :CFBundleIconFile AppIcon" "$info_plist_path" \
    || /usr/libexec/PlistBuddy -c "Add :CFBundleIconFile string AppIcon" "$info_plist_path"
  /usr/libexec/PlistBuddy -c "Set :CFBundleIconName AppIcon" "$info_plist_path" \
    || /usr/libexec/PlistBuddy -c "Add :CFBundleIconName string AppIcon" "$info_plist_path"
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

echo "Building 草稿 for $target_triple"
cargo build ${build_flag} --package soukou --target "$target_triple"

echo "Bundling 草稿.app"
cargo bundle ${build_flag} \
  --package soukou \
  --target "$target_triple"

app_path="target/${target_triple}/${target_dir}/bundle/osx/草稿.app"

if [[ ! -d "$app_path" ]]; then
  echo "Expected app bundle was not created: $app_path" >&2
  exit 1
fi

echo "Configuring app bundle icon"
configure_app_bundle_icon "$app_path"

if [[ "$target_dir" != "debug" ]]; then
  echo "Signing 草稿.app"
  sign_path "$app_path"
fi

bundle_directory="target/${target_triple}/${target_dir}"
dmg_staging_dir="${bundle_directory}/dmg"
dmg_path="${bundle_directory}/草稿-${arch_suffix}.dmg"

rm -rf "$dmg_staging_dir"
mkdir -p "$dmg_staging_dir"
cp -R "$app_path" "$dmg_staging_dir/"
ln -s /Applications "${dmg_staging_dir}/Applications"

echo "Creating DMG at $dmg_path"
rm -f "$dmg_path"
hdiutil create \
  -volname "草稿" \
  -srcfolder "$dmg_staging_dir" \
  -ov \
  -format UDZO \
  "$dmg_path"

if [[ "$target_dir" != "debug" ]]; then
  echo "Signing DMG"
  sign_path "$dmg_path"

  echo "Notarizing DMG"
  notarize_and_staple "$dmg_path"

  echo "Stapling 草稿.app"
  xcrun stapler staple "$app_path"
fi

echo "Created app bundle: $app_path"
echo "Created disk image: $dmg_path"
