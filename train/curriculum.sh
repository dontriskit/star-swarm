#!/usr/bin/env bash
# Teaches a policy to win the full game by playing earlier copies of itself. Bases start
# weak and undefended and harden to the real ones; each stage starts from the previous
# stage's policy. The only rewards are winning, losing, damage to bases and damage taken.
# Usage: train/curriculum.sh NAME [START_POLICY.pt [FIRST_STAGE]]
# FIRST_STAGE (1 to 4) resumes an interrupted run from that stage, starting from START_POLICY.pt.
set -euo pipefail
name=${1:?usage: train/curriculum.sh NAME [START_POLICY.pt [FIRST_STAGE]]}
init=${2:-}
first=${3:-1}
py=.venv/bin/python

# base HP, turret damage, updates, entropy bonus at the end of the stage
stages=("100 0 60 0.01" "300 5 100 0.01" "600 10 150 0.01" "1000 15 300 0.002")
for i in "${!stages[@]}"; do
  [ $((i + 1)) -ge "$first" ] || continue
  read -r hp turret updates entropy_end <<<"${stages[$i]}"
  run="$name-s$((i + 1))"
  # The reward for a whole base stays the same however many hit points it has.
  $py train/ppo.py --name "$run" ${init:+--init "$init"} --updates "$updates" --entropy-end "$entropy_end" --log-every 20 \
    --opponent self --base-hp "$hp" --turret-damage "$turret" \
    --reward win=50 --reward lose=50 --reward hurt=0.02 \
    --reward base_hit="$(awk "BEGIN{print 50/$hp}")" --reward base_lost="$(awk "BEGIN{print 20/$hp}")"
  init="runs/$run/policy.pt"
done
