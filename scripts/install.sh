#!/bin/sh
# Install the latest Model Downloader app on a Mac. Run it in Terminal:
#
#   curl -fsSL https://raw.githubusercontent.com/ThuoBrian/model-downloader/main/scripts/install.sh | sh
#
# It downloads the universal (Apple Silicon + Intel) app from the latest
# GitHub release, checks its checksum, and puts it in ~/Applications. The
# app isn't signed, so the quarantine flag is cleared to let it open.
# Run it again to update.

set -eu

repo="ThuoBrian/model-downloader"
asset="Model-Downloader-macos.zip"
url="https://github.com/$repo/releases/latest/download/$asset"
app_dir="$HOME/Applications"
app="$app_dir/Model Downloader.app"

if [ "$(uname -s)" != "Darwin" ]; then
    echo "This installer is for macOS. On Windows, download model-downloader.exe from the releases page." >&2
    exit 1
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

echo "Downloading Model Downloader..."
curl -fsSL "$url" -o "$tmp/$asset"
curl -fsSL "$url.sha256" -o "$tmp/$asset.sha256"
(cd "$tmp" && shasum -a 256 -c "$asset.sha256" >/dev/null) || {
    echo "The download doesn't match its checksum, so nothing was installed. Try again." >&2
    exit 1
}

ditto -x -k "$tmp/$asset" "$tmp"
mkdir -p "$app_dir"
rm -rf "$app"
mv "$tmp/Model Downloader.app" "$app"
xattr -dr com.apple.quarantine "$app" 2>/dev/null || true
echo "Installed: $app"

# Versions before 0.6.0 were a command-line tool in ~/.local/bin.
old="$HOME/.local/bin/model-downloader"
if [ -f "$old" ]; then
    rm -f "$old"
    echo "Removed the old command-line version: $old"
fi

echo "Open it from Launchpad, or from Applications in your home folder in Finder."
