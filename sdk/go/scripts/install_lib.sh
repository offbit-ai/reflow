#!/usr/bin/env bash
# Download a release of libreflow_rt_capi from GitHub Releases and unpack
# it into the Go module so cgo can find the library at compile time.
#
# Usage:   ./scripts/install_lib.sh [version]
# Default version: matches the directory's go.mod major.minor (override
# with $REFLOW_GO_VERSION, e.g. v0.2.0).
#
# Triple is auto-detected from `uname`; override with $TRIPLE if you
# need to install for a different platform (cross-compile).

set -euo pipefail

VERSION="${1:-${REFLOW_GO_VERSION:-}}"
if [[ -z "$VERSION" ]]; then
  echo "error: pass version as first arg or set REFLOW_GO_VERSION" >&2
  echo "       e.g. $0 v0.2.0" >&2
  exit 1
fi
# Strip the leading `v` if present, then re-add for tag.
VERSION="${VERSION#v}"

TRIPLE="${TRIPLE:-}"
if [[ -z "$TRIPLE" ]]; then
  case "$(uname -s)-$(uname -m)" in
    Darwin-arm64)  TRIPLE=aarch64-apple-darwin ;;
    Darwin-x86_64) TRIPLE=x86_64-apple-darwin ;;
    Linux-x86_64)  TRIPLE=x86_64-unknown-linux-gnu ;;
    Linux-aarch64) TRIPLE=aarch64-unknown-linux-gnu ;;
    MINGW*-x86_64|MSYS*-x86_64) TRIPLE=x86_64-pc-windows-msvc ;;
    *) echo "unsupported platform $(uname -s) $(uname -m); set TRIPLE manually" >&2; exit 1 ;;
  esac
fi

# Map rustc triple → Go GOOS_GOARCH dir
case "$TRIPLE" in
  aarch64-apple-darwin)      GOPAIR=darwin_arm64 ;;
  x86_64-apple-darwin)       GOPAIR=darwin_amd64 ;;
  x86_64-unknown-linux-gnu)  GOPAIR=linux_amd64 ;;
  aarch64-unknown-linux-gnu) GOPAIR=linux_arm64 ;;
  x86_64-pc-windows-msvc)    GOPAIR=windows_amd64 ;;
  *) echo "unknown triple $TRIPLE" >&2; exit 1 ;;
esac

DEST_LIB="$(dirname "$0")/../lib/$GOPAIR"
DEST_INC="$(dirname "$0")/../include"
mkdir -p "$DEST_LIB" "$DEST_INC"

URL="https://github.com/offbit-ai/reflow/releases/download/sdk/go/v${VERSION}/reflow-rt-capi-${TRIPLE}-v${VERSION}.tar.gz"
echo "fetching $URL"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
curl -fL --progress-bar "$URL" -o "$TMP/dist.tgz"

tar -xzf "$TMP/dist.tgz" -C "$TMP"
# Tarball layout: lib/<file>, include/<file>
cp -v "$TMP/lib/"* "$DEST_LIB/"
cp -v "$TMP/include/"* "$DEST_INC/"

echo
echo "installed libreflow_rt_capi v${VERSION} for $TRIPLE → $DEST_LIB"
echo "installed reflow_rt.h → $DEST_INC"
