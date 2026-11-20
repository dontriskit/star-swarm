"""Plays one match with a trained policy and saves it as a replay for the GUI.

    .venv/bin/python train/watch.py runs/mine-2/policy.pt model.replay
    cargo run --release -p swarm-gui -- --replay model.replay
"""

import argparse
import struct

import numpy as np
import torch

from ppo import Policy, observe, pack
from swarm_py import Env

p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
p.add_argument("policy")
p.add_argument("out")
p.add_argument("--seed", type=int, default=7)
p.add_argument("--frame-skip", type=int, default=3)
p.add_argument("--teams", type=int, default=3)
p.add_argument("--ships-per-team", type=int, default=5)
p.add_argument("--rocks", type=int, default=420)
p.add_argument("--radius", type=float, default=1500.0)
p.add_argument("--max-ticks", type=int, default=18000)
args = p.parse_args()

saved = torch.load(args.policy, map_location="cpu")
policy = Policy(saved["hidden"])
policy.load_state_dict(saved["model"])
env = Env(1, seed=args.seed, frame_skip=args.frame_skip, teams=args.teams, ships_per_team=args.ships_per_team,
          rock_count=args.rocks, radius=args.radius, max_ticks=args.max_ticks)

frames = bytearray()
stats = []
with torch.no_grad():
    while not stats:
        actions = pack(policy.act(observe(env, "cpu"), greedy=True)[0])
        env.step(actions)
        # The environment holds an action for frame_skip ticks and buys only on the first:
        # clear the four buy bits (5 to 8) of every 16-bit action for the held ticks.
        held = (np.frombuffer(actions, dtype="<u2") & 0x1F).astype("<u2").tobytes()
        frames += actions + held * (args.frame_skip - 1)
        stats = env.take_stats()

with open(args.out, "wb") as f:
    f.write(b"SWRP0005")
    f.write(struct.pack("<QfHHII", args.seed, args.radius, args.teams, args.ships_per_team, args.rocks, args.max_ticks))
    f.write(struct.pack("<Q", len(frames) // 2))
    f.write(frames)
    f.write(struct.pack("<I", 0))  # no state hashes: playback is not verified

minutes, collected, levels, kills, deaths = stats[0][:5]
print(f"{args.out}: {minutes:.0f} min, {collected / minutes:.1f} crystals per ship per minute, {levels:.1f} upgrade levels per ship")
