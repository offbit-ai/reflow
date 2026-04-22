#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODELS_DIR="${1:-"$ROOT_DIR/models"}"

mkdir -p "$MODELS_DIR"

download() {
  local name="$1"
  local url="$2"
  local sha="$3"
  local out="$MODELS_DIR/$name"

  if [[ ! -f "$out" ]]; then
    echo "Downloading $name"
    curl -L --fail --show-error --output "$out" "$url"
  else
    echo "Using existing $name"
  fi

  echo "$sha  $out" | shasum -a 256 -c -
}

download \
  "palm_detection_full.tflite" \
  "https://storage.googleapis.com/mediapipe-assets/palm_detection_full.tflite" \
  "1b14e9422c6ad006cde6581a46c8b90dd573c07ab7f3934b5589e7cea3f89a54"

download \
  "hand_landmark_full.tflite" \
  "https://storage.googleapis.com/mediapipe-assets/hand_landmark_full.tflite" \
  "11c272b891e1a99ab034208e23937a8008388cf11ed2a9d776ed3d01d0ba00e3"

echo "Models ready in $MODELS_DIR"
