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

# base HP, turret damage, updates, entropy bonus at the start and at the end of the stage, share
# of the starter reward. The bonus is kept low: with more of it the ships being trained act
# close to at random, and a ship that thrusts at random never comes to rest inside its base.
# All three teams of every match fly on the policy being trained and all are learned from
# (OPPONENT=mirror; "self" plays older copies, "scripted" the bots). Each update is 96 matches
# x 512 decisions (77 s of play) x 27 ships: about 1.3 million samples. Long stretches, and credit that reaches some 15 s back directly (lam 0.99),
# so that a flight somewhere or a fight is learned from what came of it.
stages=("100 0 30 0.01 0.01 1" "300 5 50 0.01 0.005 1" "600 10 75 0.002 0.0005 0.8" "1000 15 150 0.0005 0.0003 0.5")
for i in "${!stages[@]}"; do
  [ $((i + 1)) -ge "$first" ] || continue
  read -r hp turret updates entropy_start entropy_end share <<<"${stages[$i]}"
  run="$name-s$((i + 1))"
  scale() { awk "BEGIN{print $1 * $starter * $share}"; }
  # The reward for a whole base stays the same however many hit points it has.
  # At full starter reward: 0.4 for every crystal picked up (a small pack pays about 4.6, an
  # XXL one about 50). Rocks are shot for the crystals in them, so breaking one pays next to
  # nothing by itself: 0.03 per size, and 0.02 per point of damage to a small rock (0.4 for
  # breaking one), just enough for a new ship to find out that shooting rocks leads somewhere.
  # A match is won by destroying the other two bases and by nothing else: one that runs out
  # of time pays nobody anything.
  # Growing pays at every stage: 0.2 for a crystal handed over, 4 for an upgrade, a drone or a
  # tier bought and 20 more for the tier, 60 to every ship of the team when the base gains a level. Rocks pay nothing to
  # a ship with a full hold unless a teammate with room is within 500.
  # Hull is precious: every point lost costs 0.3 (shields are free to spend), so flying into
  # rocks pays only while the shields hold. A pack fades to nothing in 10 s, so the 0.2 a
  # crystal is paid on what is left of it.
  # Fighting: dying costs 10; damage to an enemy ship pays 0.1 a point, the same as shooting
  # a small rock, and twice that once its shields are down, when its crystals start to fall out.
  # No more than that: with three teams, a fight is time not spent mining and raising the base,
  # and the team that stays out of it gains. So fighting has to pay for itself; 0.3 more while the
  # team is attacking and the attacker has the upper hand; a kill pays 8.
  $py train/ppo.py --name "$run" ${init:+--init "$init"} --updates "$updates" --value-warmup "${WARMUP:-0}" --entropy-coef "$entropy_start" --entropy-end "$entropy_end" --log-every 5 --eval-envs 64 \
    --steps "${STEPS:-512}" --lam "${LAM:-0.99}" --envs "${ENVS:-96}" --hidden "${HIDDEN:-1024}" --minibatch "${MINIBATCH:-16384}" \
    --opponent "${OPPONENT:-mirror}" --base-hp "$hp" --turret-damage "$turret" \
    --reward win=50 --reward lose=50 --reward hurt=0.02 --reward hull=0.3 \
    --reward time=0.00083 \
    --reward deposit=0.2 --reward base_level=60 --reward buy=4 --reward tier=20 \
    --reward death=10 --reward hit=0.1 --reward bare=0.1 --reward edge=0.3 --reward kill=5 \
    --reward base_hit="$(awk "BEGIN{print 50/$hp}")" --reward base_lost="$(awk "BEGIN{print 20/$hp}")" \
    --reward collect="$(scale 0.4)" --reward rock="$(scale 0.03)" --reward rock_hit="$(scale 0.02)"
  init="runs/$run/policy.pt"
done
