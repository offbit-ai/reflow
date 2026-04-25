#!/usr/bin/env bash
# Repo-local dev: symlink the just-built target/<profile>/libreflow_rt_capi.*
# into sdk/go/{lib,include}/ so cgo finds them with the same paths the
# published module uses.
#
# Usage:   ./scripts/link_dev_lib.sh [profile]
# Default profile: debug (override to `release`).

set -euo pipefail

PROFILE="${1:-debug}"
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"

TARGET_DIR="$ROOT/target/$PROFILE"
INC_SRC="$ROOT/crates/reflow_rt_capi/include"

# Detect host triple → Go GOOS_GOARCH
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64)  GOPAIR=darwin_arm64;  EXT=dylib ;;
  Darwin-x86_64) GOPAIR=darwin_amd64;  EXT=dylib ;;
  Linux-x86_64)  GOPAIR=linux_amd64;   EXT=so ;;
  Linux-aarch64) GOPAIR=linux_arm64;   EXT=so ;;
  MINGW*-x86_64|MSYS*-x86_64) GOPAIR=windows_amd64; EXT=dll ;;
  *) echo "unsupported host" >&2; exit 1 ;;
esac

LIB_SRC="$TARGET_DIR/libreflow_rt_capi.$EXT"
if [[ ! -f "$LIB_SRC" ]]; then
  echo "error: $LIB_SRC missing — build it first:" >&2
  echo "       cargo build -p reflow_rt_capi${PROFILE/debug/} ${PROFILE/debug/}${PROFILE/release/--release}" >&2
  echo "       (use cargo build -p reflow_rt_capi for debug, --release for release)" >&2
  exit 1
fi

LIB_DST_DIR="$HERE/../lib/$GOPAIR"
INC_DST_DIR="$HERE/../include"
mkdir -p "$LIB_DST_DIR" "$INC_DST_DIR"

ln -sfn "$LIB_SRC" "$LIB_DST_DIR/libreflow_rt_capi.$EXT"
ln -sfn "$INC_SRC/reflow_rt.h" "$INC_DST_DIR/reflow_rt.h"

echo "linked $LIB_DST_DIR/libreflow_rt_capi.$EXT → $LIB_SRC"
echo "linked $INC_DST_DIR/reflow_rt.h → $INC_SRC/reflow_rt.h"
