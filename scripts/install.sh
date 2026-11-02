#!/bin/sh
# Install the latest model-downloader on a Mac. Run it in Terminal:
#
#   curl -fsSL https://raw.githubusercontent.com/ThuoBrian/model-downloader/main/scripts/install.sh | sh
#
# It downloads the universal (Apple Silicon + Intel) build from the latest
# GitHub release, checks its checksum, and puts it in ~/.local/bin. Files
# downloaded with curl aren't quarantined, so Gatekeeper doesn't block it.
# Run it again to update.

set -eu

repo="ThuoBrian/model-downloader"
asset="model-downloader-macos.tar.gz"
url="https://github.com/$repo/releases/latest/download/$asset"
bin_dir="$HOME/.local/bin"

if [ "$(uname -s)" != "Darwin" ]; then
    echo "This installer is for macOS. On Windows, download model-downloader.exe from the releases page." >&2
    exit 1
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

echo "Downloading model-downloader..."
curl -fsSL "$url" -o "$tmp/$asset"
curl -fsSL "$url.sha256" -o "$tmp/$asset.sha256"
(cd "$tmp" && shasum -a 256 -c "$asset.sha256" >/dev/null) || {
    echo "The download doesn't match its checksum, so nothing was installed. Try again." >&2
    exit 1
}

tar -xzf "$tmp/$asset" -C "$tmp"
mkdir -p "$bin_dir"
install -m 755 "$tmp/model-downloader" "$bin_dir/model-downloader"
echo "Installed: $bin_dir/model-downloader"

case ":$PATH:" in
    *":$bin_dir:"*)
        echo "Run it with: model-downloader"
        ;;
    *)
        echo
        echo "$bin_dir isn't on your PATH yet. Add it by running:"
        echo "  echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.zprofile && source ~/.zprofile"
        echo "Then run: model-downloader"
        ;;
esac
