# Star Swarm

A team space game where a human commands a fleet of AI-flown ships. Ships break rocks, collect crystals, buy upgrades and fight other teams; the commander sets orders, the ships fly themselves.

## Rules

- The match is played around a planet: ships, rocks and bases sit on a sphere of radius 1500, about five times the area of the old flat map, and distances are measured along its surface. Bases are spaced evenly around the equator, a third of the way around from each other with three teams. The game window shows the planet as a globe; turn it with the arrow buttons or by dragging with the right mouse button.
- Every team has a base (1000 HP) with a turret that shoots the nearest enemy ship in range. A team wins when every other base is destroyed; a team without a base gets no new ships. A match lasts at most 10 minutes.
- Crystals from broken rocks buy eight upgrades: attack, health, fire rate, shields, speed, shield regeneration, agility, ammo. Shields take damage first and grow back on their own; the hull is repaired only next to the team's own base.
- Ships have three tiers with upgrade caps of 3, 4 and 5 levels. Moving up costs 60 crystals, then 120, and the ship starts again without upgrades: a fresh ship of the next tier is a little worse than a fully upgraded one of the tier below, and better once upgraded again. Leaving tier 1 means picking a class: Lancer (one fast, long-range laser), Twin (two lasers) or Scatter (three lasers in a fan, short range, fast fire). Higher tiers are bigger.
- Ships take up room: they bounce off each other, and enemies that ram each other take damage.
- A team acts on a shared plan: farm together, defend the base, or attack one of the enemy bases. Every ship votes, the plan with the most votes becomes the order of all the team's ships, and each ship sees how its team is voting. A human commander's orders replace the vote for their team.
- Once its shields are down, a ship loses the crystals in its hold before it loses hull; they fall out and anyone can pick them up.
- Shots have a range and do less damage the farther they fly (40% at the end of their range). Every laser fired costs one round of ammo, which comes back over time.
- A boost tank gives extra thrust and top speed for two seconds and refills slowly; boost cells dropped by rocks refill it at once.
- A ship has three lives. Out of lives, a tier-2 ship falls back to tier 1 and a tier-1 ship loses all its upgrades.

## Layout

- `crates/swarm-sim`: the deterministic simulation core. No GUI, no I/O. Same seed and same actions give the same match on the same binary and machine. Also holds the scripted bot, the scripted commander and the replay format.
- `crates/swarm-gui`: the commander GUI on [gpui-kit](https://gpui-kit.com/docs). You lead the blue team against two scripted commanders, or watch a saved replay.

- `crates/swarm-env`: the batched headless environment. Many matches stepped side by side on all cores; every ship is one agent; reward weights are data.
- `crates/swarm-py`: Python bindings for `swarm-env`, built with maturin.
- `train/ppo.py`: PPO in PyTorch. One shared policy flies every ship.

Planned (see the issues): combat training with self-play, ONNX inference in Rust so the fleet flies on a trained model, ships that follow orders, and a socket for external bots.

## Run

```sh
cargo run --release -p swarm-gui                      # play as commander
cargo run --release -p swarm-gui -- --seed 7          # another map
cargo run --release -p swarm-gui -- --replay FILE     # watch a saved match

cargo test                                                        # all tests
cargo run --release -p swarm-sim --example bench -- 1             # one bot match: throughput and stats
cargo run --release -p swarm-sim --example record -- 1 out.replay # record a bot match
```

In the GUI: pick an order (Kop, Atakuj, Broń, Zbiórka), then click a point on the map. With no ships selected the order goes to the whole fleet; click ships on the map or in the list to command only those. "Zapisz powtórkę" writes `star-swarm-<seed>.replay` to the current directory.

On Linux the GUI needs a Wayland or X11 session and a working Vulkan driver.

## Train

```sh
uv venv --python 3.12 .venv
uv pip install --python .venv/bin/python -r train/requirements.txt
(cd crates/swarm-py && VIRTUAL_ENV=$PWD/../../.venv ../../.venv/bin/maturin develop --release)

train/curriculum.sh win      # self-play on bases that harden stage by stage; writes runs/win-s1 ... runs/win-s4
cargo run --release -p swarm-env --example throughput     # environment speed alone
```

To see a trained policy fly your fleet against scripted teams, or to measure it over full matches:

```sh
.venv/bin/python train/export.py runs/win-s4/policy.pt runs/win-s4/policy.swp       # weights file for the game
cargo run --release -p swarm-gui -- --model runs/win-s4/policy.swp                 # watch it play
cargo run --release -p swarm-sim --example versus -- runs/win-s4/policy.swp 20     # 20 matches, numbers per team
.venv/bin/python train/watch.py runs/win-s4/policy.pt model.replay                 # one training-style match as a replay
```

To watch training as it happens, run these next to it:

```sh
train/live.sh runs &                                  # keeps a game-readable copy of the newest policy
cargo run --release -p swarm-gui -- --watch runs      # every team flies on it; reloads when a newer one is saved
```

The run first measures the scripted bot on a fixed set of matches, then trains, then plays the trained policy on the same matches. Each line of `runs/<name>/metrics.jsonl` is one measurement for team 0: how often it won, how often it lost its base, crystals per ship per minute, upgrade levels, kills and deaths. It also records what the ships turned into: the share of each class and the mean level of each upgrade. In training the policy plays earlier copies of itself and is paid only for winning, losing, damage to bases and damage taken; nothing is paid for mining or upgrading.
