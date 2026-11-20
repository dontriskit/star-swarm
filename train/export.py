"""Writes a trained policy as a flat weights file the game can load.

    .venv/bin/python train/export.py runs/mine-2/policy.pt runs/mine-2/policy.swp
    cargo run --release -p swarm-gui -- --model runs/mine-2/policy.swp
"""

import struct
import sys

import torch

from ppo import HEADS, Policy

source, out = sys.argv[1], sys.argv[2]
saved = torch.load(source, map_location="cpu")
policy = Policy(saved["hidden"])
policy.load_state_dict(saved["model"])

layers = [policy.trunk[0], policy.trunk[2], policy.logits]
with open(out, "wb") as f:
    f.write(b"SWPOL009")
    f.write(struct.pack("<II", saved["obs_len"], saved["hidden"]))
    f.write(struct.pack("<I", len(HEADS)) + struct.pack(f"<{len(HEADS)}I", *HEADS))
    for layer in layers:
        # Row-major weights (outputs x inputs), then biases, little-endian float32.
        f.write(layer.weight.detach().numpy().astype("<f4").tobytes())
        f.write(layer.bias.detach().numpy().astype("<f4").tobytes())

# A fixed input and its logits, to check another implementation against this one.
probe = torch.sin(torch.arange(saved["obs_len"], dtype=torch.float32) * 0.37)
with torch.no_grad():
    logits = torch.cat(policy(probe)[0])
print(f"{out}: obs {saved['obs_len']}, hidden {saved['hidden']}, heads {HEADS}")
print("probe logits:", " ".join(f"{v:.4f}" for v in logits.tolist()))
