#!/bin/sh
# Federate Network installer (macOS + Linux)
#
#   curl -fsSL https://federate.network/install.sh | bash
#
# What it does, in order:
#   1. downloads the federate CLI binary for this OS/arch
#   2. installs it to /usr/local/bin/federate (asks for sudo)
#   3. runs `sudo federate setup`:
#        - local verifying DNS resolver as a system service (127.0.0.1)
#          answering every TLD in the signed root zone, present and future
#        - system DNS pointed at it (previous settings saved for uninstall)
#        - fed:// links registered to open in your browser
#        - live self-test (resolve + fetch home.fed)
#
# Undo everything: sudo federate dns uninstall && federate handler uninstall

set -eu

BASE="https://federate.network/dl"
BIN_DIR="/usr/local/bin"

os=$(uname -s)
arch=$(uname -m)
case "$os" in
    Darwin) os="darwin" ;;
    Linux)  os="linux" ;;
    *) echo "unsupported OS: $os (macOS and Linux only; Windows: see install.ps1)" >&2; exit 1 ;;
esac
case "$arch" in
    arm64|aarch64) arch="arm64" ;;
    x86_64|amd64)  arch="x86_64" ;;
    *) echo "unsupported architecture: $arch" >&2; exit 1 ;;
esac

url="$BASE/federate-$os-$arch.tar.gz"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

echo "[..] downloading $url"
curl -fsSL "$url" -o "$tmp/federate.tar.gz" || {
    echo "[!!] no prebuilt binary for $os-$arch yet." >&2
    echo "     build from source instead:" >&2
    echo "       git clone https://github.com/federatenetwork/federate" >&2
    echo "       cargo build --release -p federate-cli" >&2
    echo "       sudo ./target/release/federate setup" >&2
    exit 1
}
tar -xzf "$tmp/federate.tar.gz" -C "$tmp"

echo "[..] installing to $BIN_DIR/federate (sudo)"
sudo install -m 755 "$tmp/federate" "$BIN_DIR/federate"

echo "[..] running machine setup (sudo)"
sudo "$BIN_DIR/federate" setup

echo
echo "[ok] Federate Network installed. Open http://home.fed"
