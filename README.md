# Star Swarm

A team space game where a human commands a fleet of AI-flown ships. Ships break rocks, collect crystals, buy upgrades and fight other teams; the commander sets orders, the ships fly themselves.

Built for people and for bots: humans, scripted bots and trained policies all drive ships through the same observation and action interface.

## Layout

- `crates/swarm-sim`: the deterministic simulation core. No GUI, no I/O. Same seed and same actions give the same match on the same binary and machine.

Planned (see the issues): a commander GUI on [gpui-kit](https://gpui-kit.com/docs), a batched headless environment, and PPO training in PyTorch with ONNX inference in Rust.

## Run

```sh
cargo test                                  # determinism and behaviour tests
cargo run --release --example bench -- 1    # one match of scripted bots, seed 1
```
