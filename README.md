# Star Swarm

A team space game where a human commands a fleet of AI-flown ships. Ships break rocks, collect crystals, buy upgrades and fight other teams; the commander sets orders, the ships fly themselves.

## Rules

- The match is played around a planet: ships, rocks and bases sit on a sphere of radius 1200, and distances are measured along its surface. Three teams of seven ships by default. Bases are spaced evenly around the equator, a third of the way around from each other with three teams. The game window shows the planet as a globe; turn it with the arrow buttons or by dragging with the right mouse button.
- Every team has a base (1000 HP) with a turret that shoots the nearest enemy ship in range. A team wins when every other base is destroyed; a team without a base gets no new ships. A match lasts at most 20 minutes.
- Crystals buy nothing until a ship hands them over next to its own base. Each crystal handed over is a point for the ship to spend, and counts towards the base's next level: 1000 crystals in total for level 2, 2500 for level 3, up to level 7. A level adds 200 hit points and 15% turret damage, and a ship cannot move up a tier beyond its base's level.
- Points buy upgrades, the next tier, a base repair (10 points for 10 hit points) or a drone (40 points; one per tier of the ship). A drone follows its owner, breaks rocks near it and gathers crystals for it; when the owner is destroyed the drone drifts until any ship with room flies up and takes it.
- A tier-1 ship does 1 damage a shot and has 100 hull. Rocks come in five sizes, S to XXL, with 10, 40, 90, 160 and 250 hit points. A broken rock leaves one pack of crystals: 8-15 for S, 16-40 for M, 41-70 for L, 71-110 for XL, 111-150 for XXL. A ship takes as much of a pack as it has room for. Small rocks pay best per shot, so they suit small ships.
- There are eight upgrades: attack, health, fire rate, shields, speed, shield regeneration, agility, ammo. Shields take damage first and grow back on their own; the hull is repaired only next to the team's own base.
- Ships have three tiers with upgrade caps of 3, 4 and 5 levels. Moving up costs 60 crystals, then 120, and the ship starts again without upgrades: a fresh ship of the next tier is a little worse than a fully upgraded one of the tier below, and better once upgraded again. Leaving tier 1 means picking a class: Lancer (one fast, hard-hitting, long-range laser, but 70% of the hull), Twin (two lasers), Scatter (three lasers in a fan, short range, fast fire) or Ram (160% of the hull, a weak gun, built to fly into enemies). Higher tiers are bigger.
- Ships take up room: they bounce off each other, and enemies that collide take damage by how fast they met. A Ram deals five times the ramming damage and takes a quarter; a Lancer deals half and takes one and a half times.
- Every ship picks its own mode: farm, defend the base, or attack one of the enemy bases. The mode becomes its order. A human commander's orders replace the modes of their team.
- Team radar: a ship knows where every teammate is, how damaged it is and what mode it is in, and it is told about enemies a teammate can see.
- Fog of war: beyond its own view range a ship sees nothing else, and a team knows an enemy base's health only from the last time one of its ships was near it. When you play, the window shows only what your team sees.
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

To watch training as it happens, run this next to it. The trainer streams one of its matches to `runs/<name>/live.swl` and the window replays it tick for tick, with a table of the run's latest measurements. (Without a stream, `train/live.sh runs &` keeps a game-readable copy of the newest policy and the window plays matches of its own on it.)

```sh
cargo run --release -p swarm-gui -- --watch runs      # shows one of the trainer's own matches as it is played
```

The run first measures the scripted bot on a fixed set of matches, then trains, then plays the trained policy on the same matches. Each line of `runs/<name>/metrics.jsonl` is one measurement for team 0: how often it won, how often it lost its base, crystals per ship per minute, upgrade levels, kills and deaths. It also records what the ships turned into: the share of each class and the mean level of each upgrade. In training the policy plays earlier copies of itself and is paid only for winning, losing, damage to bases and damage taken; nothing is paid for mining or upgrading.
