#!/usr/bin/env bash
# Downloads the GLiNER model used by redactor's optional "Deep scan".
#
# The app never downloads anything by itself: run this once, by hand. The
# files come from a pinned Hugging Face revision and are checked against
# the SHA-256 hashes below before they are moved into place.
#
# Model: knowledgator/gliner-pii-edge-v1.0 (Apache-2.0), quantized ONNX.
#
# Usage: scripts/fetch-model.sh [destination dir]
#        default: $REDACTOR_CONFIG_DIR, $XDG_CONFIG_HOME/redactor or ~/.config/redactor
set -euo pipefail

REPO=knowledgator/gliner-pii-edge-v1.0
REVISION=9b7f39b0a2da971a5beea78d35f1539d4009c891
NAME=gliner-pii-edge-v1.0

CONFIG=${REDACTOR_CONFIG_DIR:-${XDG_CONFIG_HOME:-$HOME/.config}/redactor}
DEST=${1:-$CONFIG/models/$NAME}

# path in the repo | local name | sha256
FILES=(
  "onnx/model_quint8.onnx|model.onnx|988acb03456b26e2d9f2521016d820310c2ed64deb4a846297d3289f0c2eb7e4"
  "tokenizer.json|tokenizer.json|84b3a9b18f04a0ccd03b72d9f871b7e0bec40fd7021ef50bc30a7c3693c11205"
  "gliner_config.json|gliner_config.json|77e6b57335c4bfd461e9041682196dd6c373a0b09bbd9269ef9e95b807915340"
)

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

for entry in "${FILES[@]}"; do
  IFS='|' read -r remote local expected <<<"$entry"
  url="https://huggingface.co/$REPO/resolve/$REVISION/$remote"
  echo "downloading $remote"
  curl -fL --proto '=https' --tlsv1.2 --progress-bar -o "$tmp/$local" "$url"
  actual=$(shasum -a 256 "$tmp/$local" | cut -d' ' -f1)
  if [ "$actual" != "$expected" ]; then
    echo "checksum mismatch for $remote" >&2
    echo "  expected $expected" >&2
    echo "  actual   $actual" >&2
    exit 1
  fi
done

mkdir -p "$DEST"
mv "$tmp"/* "$DEST"/
chmod 600 "$DEST"/*
echo "model ready in $DEST"
