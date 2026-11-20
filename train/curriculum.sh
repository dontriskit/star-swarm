#!/usr/bin/env bash
# Teaches a policy to win the full game in stages: bases start weak and undefended
# and harden to the real ones, each stage starting from the previous stage's policy.
# Usage: train/curriculum.sh NAME [START_POLICY.pt]
set -euo pipefail
name=${1:?usage: train/curriculum.sh NAME [START_POLICY.pt]}
init=${2:-}
py=.venv/bin/python

# base HP, turret damage, updates, entropy bonus at the end of the stage
stages=("100 0 60 0.01" "300 5 100 0.01" "600 10 150 0.01" "1000 15 300 0.001")
for i in "${!stages[@]}"; do
  read -r hp turret updates entropy_end <<<"${stages[$i]}"
  run="$name-s$((i + 1))"
  # The reward for a whole base stays the same however many hit points it has.
  $py train/ppo.py --name "$run" ${init:+--init "$init"} --updates "$updates" --entropy-end "$entropy_end" --log-every 20 \
    --base-hp "$hp" --turret-damage "$turret" \
    --reward collect=0.1 --reward rock=0.05 --reward buy=0.5 --reward crash=0.02 --reward death=1 \
    --reward kill=2 --reward hit=0.01 --reward win=50 --reward lose=50 \
    --reward base_hit="$(awk "BEGIN{print 50/$hp}")" --reward base_lost="$(awk "BEGIN{print 20/$hp}")"
  init="runs/$run/policy.pt"
done
