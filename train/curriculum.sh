#!/usr/bin/env bash
# Teaches a policy to win the full game by playing earlier copies of itself. Bases start
# weak and undefended and harden to the real ones; each stage starts from the previous
# stage's policy. The rewards are winning, losing, damage to bases, damage taken, the time a
# match takes (a full twenty minutes costs 30, and ending without a winner another 30),
# crystals handed over at the base (0.1 each) and every new base level (25 to each ship), and a
# starter reward for collecting crystals, breaking rocks and buying upgrades that shrinks
# stage by stage. Set STARTER=0 to switch the starter reward off altogether.
# ENVS, HIDDEN and MINIBATCH set the number of matches played side by side, the width of the
# network and the batch size (defaults 2048, 1024, 16384: a network that keeps the GPU busy).
# Usage: train/curriculum.sh NAME [START_POLICY.pt [FIRST_STAGE]]
# FIRST_STAGE (1 to 4) resumes an interrupted run from that stage, starting from START_POLICY.pt.
set -euo pipefail
name=${1:?usage: train/curriculum.sh NAME [START_POLICY.pt [FIRST_STAGE]]}
init=${2:-}
first=${3:-1}
starter=${STARTER:-1}
py=.venv/bin/python

# base HP, turret damage, updates, entropy bonus at the end of the stage, share of the starter reward
# Each update is 2048 matches x 64 decisions: about 900 thousand samples for the ships being trained.
stages=("100 0 30 0.01 1" "300 5 50 0.01 1" "600 10 75 0.01 0.8" "1000 15 150 0.002 0.5")
for i in "${!stages[@]}"; do
  [ $((i + 1)) -ge "$first" ] || continue
  read -r hp turret updates entropy_end share <<<"${stages[$i]}"
  run="$name-s$((i + 1))"
  scale() { awk "BEGIN{print $1 * $starter * $share}"; }
  # The reward for a whole base stays the same however many hit points it has.
  # At full starter reward: 0.2 for every crystal picked up (a small pack pays about 2.3, an
  # XXL one about 25), 0.1 per size for a rock broken, 0.1 per point of damage to a small rock
  # (1 for breaking one), 1 for anything bought.
  # Hull is precious: every point lost costs 0.3 (shields are free to spend), so flying into
  # rocks pays only while the shields hold. A pack fades to nothing in 10 s, so the 0.2 a
  # crystal is paid on what is left of it.
  # Fighting: dying costs 10; damage to an enemy ship pays 0.25 a point, more than shooting
  # a rock ever does, so an enemy in the line of fire is the better target; 0.3 more while the
  # team is attacking and the attacker has the upper hand; a kill pays 8.
  $py train/ppo.py --name "$run" ${init:+--init "$init"} --updates "$updates" --entropy-end "$entropy_end" --log-every 5 --eval-envs 64 \
    --envs "${ENVS:-2048}" --hidden "${HIDDEN:-1024}" --minibatch "${MINIBATCH:-16384}" \
    --opponent self --base-hp "$hp" --turret-damage "$turret" \
    --reward win=50 --reward lose=50 --reward hurt=0.02 --reward hull=0.3 \
    --reward time=0.00083 --reward draw=30 \
    --reward deposit=0.1 --reward base_level=25 \
    --reward death=10 --reward hit=0.25 --reward edge=0.3 --reward kill=8 \
    --reward base_hit="$(awk "BEGIN{print 50/$hp}")" --reward base_lost="$(awk "BEGIN{print 20/$hp}")" \
    --reward collect="$(scale 0.2)" --reward rock="$(scale 0.1)" --reward buy="$(scale 1)" --reward rock_hit="$(scale 0.1)"
  init="runs/$run/policy.pt"
done
