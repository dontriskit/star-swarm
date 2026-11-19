# Star Swarm

A team space game where a human commands a fleet of AI-flown ships. Ships break rocks, collect crystals, buy upgrades and fight other teams; the commander sets orders, the ships fly themselves.

Built for people and for bots: humans, scripted bots and trained policies all drive ships through the same observation and action interface.

## Layout

- `crates/swarm-sim`: the deterministic simulation core. No GUI, no I/O. Same seed and same actions give the same match on the same binary and machine. Also holds the scripted bot, the scripted commander and the replay format.
- `crates/swarm-gui`: the commander GUI on [gpui-kit](https://gpui-kit.com/docs). You lead the blue team against two scripted commanders, or watch a saved replay.

Planned (see the issues): a batched headless environment, PPO training in PyTorch, and ONNX inference in Rust so the fleet flies on a trained model.

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
