# Star Swarm

A team space game where a human commands a fleet of AI-flown ships. Ships break rocks, collect crystals, buy upgrades and fight other teams; the commander sets orders, the ships fly themselves.

## Rules

- Every team has a base (100 HP by default). A team wins when every other base is destroyed. A team without a base gets no new ships.
- Crystals from broken rocks buy six upgrades: attack, health, fire rate, shields, speed, regeneration. Shields take damage before the hull and grow back on their own; the hull is repaired only next to the team's own base.
- Ships have two tiers. Tier 2 costs 60 crystals, multiplies hull, shields and damage by 1.5, and raises the upgrade cap from level 3 to level 5.

Built for people and for bots: humans, scripted bots and trained policies all drive ships through the same observation and action interface.

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

.venv/bin/python train/ppo.py --name win --updates 100 \
    --reward win=20 --reward lose=20 --reward base_hit=0.2 --reward base_lost=0.05 \
    --reward kill=3 --reward hit=0.02 --reward death=3     # writes runs/win/
cargo run --release -p swarm-env --example throughput     # environment speed alone
```

To see a trained policy fly your fleet against scripted teams, or to measure it over full matches:

```sh
.venv/bin/python train/export.py runs/win/policy.pt runs/win/policy.swp       # weights file for the game
cargo run --release -p swarm-gui -- --model runs/win/policy.swp                 # watch it play
cargo run --release -p swarm-sim --example versus -- runs/win/policy.swp 20     # 20 matches, numbers per team
.venv/bin/python train/watch.py runs/win/policy.pt model.replay                 # one training-style match as a replay
```

The run first measures the scripted bot on a fixed set of matches, then trains, then plays the trained policy on the same matches. Each line of `runs/<name>/metrics.jsonl` is one measurement for team 0: how often it won, how often it lost its base, crystals per ship per minute, upgrade levels, kills and deaths. In training, team 0 is flown by the policy and the other teams by the scripted bot.
