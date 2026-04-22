#!/usr/bin/env bash
#
# publish-crates.sh — publish the Reflow workspace crates to crates.io in
# dependency-tier order.
#
# Assumes `cargo login` has already been run on this machine. Stops on first
# failure. Already-published versions are skipped automatically.
#
# Usage:
#   scripts/publish-crates.sh               # real publish, bottom-up
#   scripts/publish-crates.sh --dry-run     # cargo publish --dry-run for each
#   scripts/publish-crates.sh --start <crate>   # resume at a specific crate
#   scripts/publish-crates.sh --only  <crate>   # publish a single crate
#

set -euo pipefail

DRY_RUN=false
START_FROM=""
ONLY=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)   DRY_RUN=true;               shift ;;
    --start)     START_FROM="${2:?}";        shift 2 ;;
    --only)      ONLY="${2:?}";              shift 2 ;;
    -h|--help)
      awk '/^#!/ {next} /^[^#]/ {exit} {sub(/^# ?/, ""); print}' "$0"
      exit 0
      ;;
    *) echo "unknown flag: $1" >&2; exit 2 ;;
  esac
done

# Colors (disabled if not a tty).
if [[ -t 1 ]]; then
  C_R='\033[0;31m'; C_G='\033[0;32m'; C_Y='\033[1;33m'; C_B='\033[0;34m'; C_N='\033[0m'
else
  C_R=''; C_G=''; C_Y=''; C_B=''; C_N=''
fi
say()  { printf "${C_B}==>${C_N} %s\n" "$*"; }
ok()   { printf "${C_G}✓${C_N} %s\n" "$*"; }
warn() { printf "${C_Y}!${C_N} %s\n" "$*"; }
err()  { printf "${C_R}✗${C_N} %s\n" "$*" >&2; }

# Tiers — each tier depends only on earlier tiers.
TIER_0=(
  reflow_actor_macro
  reflow_graph
  reflow_assets
  reflow_pixel
  reflow_vector
  reflow_sdf
  reflow_shader
  reflow_dsp
  reflow_media_types
  reflow_tracing_protocol
)
TIER_1=( reflow_actor  reflow_litert  reflow_media_codec )
TIER_2=( reflow_network  reflow_asset_registry  reflow_cv_ops  reflow_api_services )
TIER_3=( reflow_ml_ops )
TIER_4=( reflow_taskpacks )
TIER_5=( reflow_components )
TIER_6=( reflow_rt )

ALL_TIERS=(
  "tier-0:${TIER_0[*]}"
  "tier-1:${TIER_1[*]}"
  "tier-2:${TIER_2[*]}"
  "tier-3:${TIER_3[*]}"
  "tier-4:${TIER_4[*]}"
  "tier-5:${TIER_5[*]}"
  "tier-6:${TIER_6[*]}"
)

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

crate_version() {
  local crate="$1"
  awk -F '"' '
    /^\[package\]/ { in_pkg = 1; next }
    /^\[/          { in_pkg = 0 }
    in_pkg && /^version[[:space:]]*=/ { print $2; exit }
  ' "crates/$crate/Cargo.toml"
}

is_published() {
  local crate="$1" version="$2"
  # crates.io returns 200 if the (crate,version) exists, 404 otherwise.
  local code
  code=$(curl -fsS -o /dev/null -w '%{http_code}' \
           "https://crates.io/api/v1/crates/$crate/$version" || true)
  [[ "$code" == "200" ]]
}

wait_for_index() {
  local crate="$1" version="$2"
  local tries=0
  # up to ~20 s; sparse index usually propagates in seconds.
  while (( tries < 10 )); do
    if is_published "$crate" "$version"; then return 0; fi
    sleep 2
    ((tries++))
  done
  warn "$crate $version not visible in index after $((tries*2))s — continuing, downstream publish may need a manual retry"
}

publish_one() {
  local crate="$1"
  local version
  version=$(crate_version "$crate")

  if [[ -z "$version" ]]; then
    err "could not read version for $crate"; return 1
  fi

  say "publishing $crate $version"

  if $DRY_RUN; then
    cargo publish -p "$crate" --dry-run --allow-dirty
    ok "$crate $version (dry-run)"
    return 0
  fi

  if is_published "$crate" "$version"; then
    warn "$crate $version already on crates.io — skipping"
    return 0
  fi

  cargo publish -p "$crate"
  wait_for_index "$crate" "$version"
  ok "published $crate $version"
}

should_skip_until_start() {
  [[ -n "$START_FROM" ]]
}

main() {
  # Flatten tiers into a single ordered list, optionally filtered by --only / --start.
  local started=false
  for tier_entry in "${ALL_TIERS[@]}"; do
    local tier_name="${tier_entry%%:*}"
    local tier_crates="${tier_entry#*:}"
    say "$tier_name"
    local tier_did_anything=false

    for crate in $tier_crates; do
      if [[ -n "$ONLY" && "$crate" != "$ONLY" ]]; then continue; fi
      if [[ -n "$START_FROM" && "$started" == false ]]; then
        if [[ "$crate" == "$START_FROM" ]]; then
          started=true
        else
          continue
        fi
      fi

      publish_one "$crate"
      tier_did_anything=true
    done

    # Between tiers we have already waited for each crate's index visibility;
    # no extra delay needed unless the user wants it.
    if [[ "$tier_did_anything" == false ]]; then
      printf "   (no crates in this tier for this run)\n"
    fi
  done

  ok "all done"
}

main
