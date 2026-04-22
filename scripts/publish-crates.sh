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
  # `-s` silences progress, drop `-f`/`-S` so 4xx doesn't spew errors.
  local code
  code=$(curl -s -o /dev/null -w '%{http_code}' \
           "https://crates.io/api/v1/crates/$crate/$version" 2>/dev/null || true)
  [[ "$code" == "200" ]]
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

  # Check whether this is a first-time publish of the crate name (triggers
  # the stricter "new crate" rate limit on crates.io) or a version update.
  local is_new_crate=false
  local head_code
  head_code=$(curl -s -o /dev/null -w '%{http_code}' \
                "https://crates.io/api/v1/crates/$crate" 2>/dev/null || true)
  if [[ "$head_code" == "404" ]]; then is_new_crate=true; fi

  # cargo publish waits internally for the new version to appear in the
  # sparse index before returning, so downstream tiers resolve the fresh
  # version. On HTTP 429 we retry with a cool-down — see the loop below.
  local tmp; tmp=$(mktemp)
  local max_retries=4 attempt=0
  while (( attempt < max_retries )); do
    if cargo publish -p "$crate" 2>&1 | tee "$tmp"; then
      rm -f "$tmp"
      ok "published $crate $version"
      break
    fi

    if grep -qE '429 Too Many Requests|too many new crates' "$tmp"; then
      # crates.io's new-crate limit is 1 per 10 minutes. Wait 10 min and retry.
      warn "rate-limited by crates.io; sleeping 10 min before retry ($((attempt+1))/$max_retries)"
      sleep 600
      ((attempt++))
      continue
    fi

    rm -f "$tmp"
    err "cargo publish failed for $crate (non-rate-limit error)"
    return 1
  done
  if (( attempt == max_retries )); then
    err "gave up on $crate after $max_retries rate-limit retries"
    return 1
  fi

  # Between successful new-crate publishes, pace at 20s so we don't burn
  # the 5-crate burst budget on a short first run; actual refill is 1 per
  # 10 min, so the retry loop above is what keeps the full workspace
  # publishable in one go.
  if [[ "$is_new_crate" == true ]]; then
    say "sleeping 20s between new-crate publishes"
    sleep 20
  fi
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
