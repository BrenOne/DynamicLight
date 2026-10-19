#!/usr/bin/env bash
# Installs the desktop app, or the standalone CLI with --cli.
set -euo pipefail

fail() { printf 'DynamicLight: %s\n' "$*" >&2; exit 1; }
version=latest
appimage=false
cli=false
download_only=false
directory=$PWD
while [ "$#" -gt 0 ]; do
  case "$1" in
    --version|--directory)
      [ "$#" -ge 2 ] || fail "$1 requires a value"
      case "$1" in --version) version=${2#v} ;; --directory) directory=$2 ;; esac
      shift 2 ;;
    --appimage) appimage=true; shift ;;
    --cli) cli=true; shift ;;
    --download-only) download_only=true; shift ;;
    --help)
      echo 'Usage: bash install.sh [--version VERSION] [--appimage | --cli] [--download-only] [--directory DIR]'
      echo '--cli installs command-line controls in ~/.local/bin; requires the desktop app running with Local automation enabled.'
      echo 'Installs for the current user on macOS; uses apt on Linux, or --appimage for a user-local app.'
      echo '--directory selects the download destination with --download-only.'
      exit 0 ;;
    *) fail "Unknown option: $1" ;;
  esac
done

os=$(uname -s)
arch=$(uname -m)
case "$os:$arch" in
  Darwin:arm64|Darwin:aarch64) asset=dynamiclight-desktop-macos-arm64.dmg ;;
  Darwin:x86_64)
    # A terminal running under Rosetta should still install the native app.
    if [ "$(sysctl -in sysctl.proc_translated 2>/dev/null || true)" = 1 ]; then
      asset=dynamiclight-desktop-macos-arm64.dmg
    else
      asset=dynamiclight-desktop-macos-x64.dmg
    fi ;;
  Linux:x86_64|Linux:amd64)
    if "$appimage"; then asset=dynamiclight-desktop-linux-x64.AppImage
    else asset=dynamiclight-desktop-linux-x64.deb; fi ;;
  *) fail "No desktop package for $os/$arch. See https://github.com/mskutin/DynamicLight/releases" ;;
esac
if "$appimage" && [ "$os" != Linux ]; then fail '--appimage is only supported on Linux'; fi
if "$cli"; then
  "$appimage" && fail '--cli and --appimage cannot be combined'
  case "$asset" in
    *macos-arm64*) asset=dynamiclight-cli-macos-arm64 ;;
    *macos-x64*) asset=dynamiclight-cli-macos-x64 ;;
    *linux-x64*) asset=dynamiclight-cli-linux-x64 ;;
  esac
fi
command -v curl >/dev/null || fail 'Install curl first'
if command -v sha256sum >/dev/null; then hash_command=sha256sum
elif command -v shasum >/dev/null; then hash_command=shasum
else fail 'Install sha256sum or shasum first'; fi

scratch=$(mktemp -d)
mounted=false
cleanup() {
  if "$mounted"; then hdiutil detach "$scratch/mount" -quiet || true; fi
  rm -rf -- "$scratch"
}
trap cleanup EXIT
fetch() { curl --fail --silent --show-error --location --retry 3 --proto '=https' --proto-redir '=https' "$1" -o "$2"; }
releases=https://github.com/mskutin/DynamicLight/releases
if [ "$version" = latest ]; then
  fetch "$releases/latest/download/release-version.txt" "$scratch/version"
  version=$(tr -d '\r\n' < "$scratch/version")
fi
[[ "$version" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z]+([.-][0-9A-Za-z]+)*)?$ ]] || fail 'Invalid release version'
base="$releases/download/v$version"
fetch "$base/SHA256SUMS" "$scratch/SHA256SUMS"
verified_download() {
  local name=$1 expected actual
  expected=$(awk -v name="$name" '$2 == name { print $1 }' "$scratch/SHA256SUMS")
  [[ "$expected" =~ ^[0-9a-f]{64}$ ]] || fail "Missing or ambiguous SHA-256 for $name"
  fetch "$base/$name" "$scratch/$name"
  if [ "$hash_command" = sha256sum ]; then actual=$(sha256sum "$scratch/$name")
  else actual=$(shasum -a 256 "$scratch/$name"); fi
  actual=${actual%% *}
  [ "$actual" = "$expected" ] || fail "SHA-256 mismatch for $name; nothing installed"
}
verified_download "$asset"
if "$appimage"; then verified_download 69-dynamiclight.rules; fi
if "$download_only"; then
  mkdir -p "$directory"
  cp "$scratch/$asset" "$directory/$asset"
  if "$appimage"; then cp "$scratch/69-dynamiclight.rules" "$directory/69-dynamiclight.rules"; fi
  printf 'Verified DynamicLight %s downloaded to %s\n' "$version" "$directory"
  exit 0
fi

if "$cli"; then
  [ "$(id -u)" != 0 ] || fail 'Run the CLI installer as your normal user, without sudo'
  destination="$HOME/.local/bin"
  mkdir -p "$destination"
  staged=$(mktemp "$destination/.dynamiclight.XXXXXX")
  if ! install -m 755 "$scratch/$asset" "$staged" || ! mv -f "$staged" "$destination/dynamiclight"; then
    rm -f -- "$staged"; fail 'Could not install CLI'
  fi
  printf 'Installed DynamicLight CLI %s at %s/dynamiclight\n' "$version" "$destination"
  echo 'Add ~/.local/bin to PATH if needed. Enable Local automation in desktop Settings, then run dynamiclight status.'
  exit 0
fi

# Never launch the desktop app or change capture consent.
case "$os" in
  Darwin)
    [ "$(id -u)" != 0 ] || fail 'Run this installer as your normal user, without sudo'
    apps="$HOME/Applications"
    mkdir -p "$apps" "$scratch/mount"
    hdiutil attach "$scratch/$asset" -mountpoint "$scratch/mount" -nobrowse -readonly -quiet
    mounted=true
    [ -d "$scratch/mount/DynamicLight.app" ] || fail 'The disk image does not contain DynamicLight.app'
    # Copy into a new bundle first, so updates cannot retain removed bundle files.
    staged=$(mktemp -d "$apps/.dynamiclight-install.XXXXXX")
    if ! ditto "$scratch/mount/DynamicLight.app" "$staged/DynamicLight.app"; then
      rm -rf -- "$staged"; fail 'Could not copy the app'
    fi
    if [ -e "$apps/DynamicLight.app" ]; then mv "$apps/DynamicLight.app" "$staged/previous.app"; fi
    if ! mv "$staged/DynamicLight.app" "$apps/DynamicLight.app"; then
      if [ -e "$staged/previous.app" ]; then mv "$staged/previous.app" "$apps/DynamicLight.app"; fi
      fail "Could not install the app; staged files retained at $staged"
    fi
    rm -rf -- "$staged"
    echo "Installed DynamicLight $version in $apps. Open it from Finder."
    ;;
  Linux)
    if "$appimage"; then
      [ "$(id -u)" != 0 ] || fail 'Run the AppImage installer as your normal user, without sudo'
      destination="$HOME/.local/opt/dynamiclight-desktop"
      mkdir -p "$destination"
      install -m 755 "$scratch/$asset" "$destination/DynamicLight.AppImage.new"
      mv -f "$destination/DynamicLight.AppImage.new" "$destination/DynamicLight.AppImage"
      install -m 644 "$scratch/69-dynamiclight.rules" "$destination/69-dynamiclight.rules"
      printf 'Installed DynamicLight %s at %s/DynamicLight.AppImage\n' "$version" "$destination"
      echo 'USB access and audio/FUSE dependencies require the one-time setup in https://github.com/mskutin/DynamicLight/blob/main/docs/linux.md'
      printf 'USB rule downloaded to %s/69-dynamiclight.rules\n' "$destination"
    else
      command -v apt-get >/dev/null || fail 'This system has no apt-get. Use --appimage and follow docs/linux.md for runtime dependencies'
      chmod 755 "$scratch"
      chmod 644 "$scratch/$asset"
      if [ "$(id -u)" = 0 ]; then apt-get install -y "$scratch/$asset"
      else sudo apt-get install -y "$scratch/$asset"; fi
      echo "Installed DynamicLight $version. Reconnect your strip and open the app from the applications menu."
    fi ;;
esac
