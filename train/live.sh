#!/usr/bin/env bash
# Keeps a game-readable copy (policy.swp) next to the newest policy.pt under runs/,
# so `swarm-gui --watch runs` can show training as it happens.
# Usage: train/live.sh [RUNS_DIR]   (stop with Ctrl-C)
set -uo pipefail
dir=${1:-runs}
while true; do
  newest=$(ls -t "$dir"/*/policy.pt 2>/dev/null | head -1)
  if [ -n "$newest" ]; then
    out="${newest%.pt}.swp"
    if [ ! -e "$out" ] || [ "$newest" -nt "$out" ]; then
      # Write to a temporary name and rename, so the game never reads a half-written file.
      CUDA_VISIBLE_DEVICES= .venv/bin/python train/export.py "$newest" "$out.tmp" >/dev/null 2>&1 && mv "$out.tmp" "$out"
    fi
  fi
  sleep 10
done
