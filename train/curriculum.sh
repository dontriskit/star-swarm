#!/usr/bin/env bash
# Teaches a policy to win the full game by playing earlier copies of itself. Bases start
# weak and undefended and harden to the real ones; each stage starts from the previous
# stage's policy. The rewards are winning, losing, damage to bases, damage taken, the time a
# match takes (a full twenty minutes costs 30, and ending without a winner another 30),
# crystals handed over at the base (0.1 each) and every new base level (25 to each ship), and a
# starter reward for collecting crystals, breaking rocks and buying upgrades that shrinks
# stage by stage. Set STARTER=0 to switch the starter reward off altogether.
# Usage: train/curriculum.sh NAME [START_POLICY.pt [FIRST_STAGE]]
# FIRST_STAGE (1 to 4) resumes an interrupted run from that stage, starting from START_POLICY.pt.
set -euo pipefail
name=${1:?usage: train/curriculum.sh NAME [START_POLICY.pt [FIRST_STAGE]]}
init=${2:-}
first=${3:-1}
starter=${STARTER:-1}
py=.venv/bin/python

# base HP, turret damage, updates, entropy bonus at the end of the stage, share of the starter reward
stages=("100 0 60 0.01 1" "300 5 100 0.01 1" "600 10 150 0.01 0.5" "1000 15 300 0.002 0.2")
for i in "${!stages[@]}"; do
  [ $((i + 1)) -ge "$first" ] || continue
  read -r hp turret updates entropy_end share <<<"${stages[$i]}"
  run="$name-s$((i + 1))"
  scale() { awk "BEGIN{print $1 * $starter * $share}"; }
  # The reward for a whole base stays the same however many hit points it has.
  # At full starter reward: 0.15 for every crystal picked up (a small pack pays about 1.7, an
  # XXL one about 20), 0.2 per size for a rock broken, 1 for anything bought.
  $py train/ppo.py --name "$run" ${init:+--init "$init"} --updates "$updates" --entropy-end "$entropy_end" --log-every 20 --eval-envs 64 \
    --opponent self --base-hp "$hp" --turret-damage "$turret" \
    --reward win=50 --reward lose=50 --reward hurt=0.02 \
    --reward time=0.00083 --reward draw=30 \
    --reward deposit=0.1 --reward base_level=25 \
    --reward base_hit="$(awk "BEGIN{print 50/$hp}")" --reward base_lost="$(awk "BEGIN{print 20/$hp}")" \
    --reward collect="$(scale 0.15)" --reward rock="$(scale 0.2)" --reward buy="$(scale 1)"
  init="runs/$run/policy.pt"
done
