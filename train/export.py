"""Writes a trained policy as a flat weights file the game can load.

    .venv/bin/python train/export.py runs/mine-2/policy.pt runs/mine-2/policy.swp
    cargo run --release -p swarm-gui -- --model runs/mine-2/policy.swp
"""

import struct
import sys

import torch

from ppo import HEADS, MEMORY, Policy

source, out = sys.argv[1], sys.argv[2]
saved = torch.load(source, map_location="cpu")
policy = Policy(saved["hidden"])
# A policy saved before ships had a memory gets a blank one, which changes nothing.
policy.load_state_dict(saved["model"], strict=False)

# In the order the game reads them: the two layers that read the observation, the memory cell
# (what comes in, then what it held), the layer that adds the memory back, and the logits.
layers = [
    (policy.trunk[0].weight, policy.trunk[0].bias),
    (policy.trunk[2].weight, policy.trunk[2].bias),
    (policy.memory.weight_ih, policy.memory.bias_ih),
    (policy.memory.weight_hh, policy.memory.bias_hh),
    (policy.recall.weight, policy.recall.bias),
    (policy.logits.weight, policy.logits.bias),
]
with open(out, "wb") as f:
    f.write(b"SWPOL011")
    f.write(struct.pack("<III", saved["obs_len"], saved["hidden"], MEMORY))
    f.write(struct.pack("<I", len(HEADS)) + struct.pack(f"<{len(HEADS)}I", *HEADS))
    for weight, bias in layers:
        # Row-major weights (outputs x inputs), then biases, little-endian float32.
        f.write(weight.detach().numpy().astype("<f4").tobytes())
        f.write(bias.detach().numpy().astype("<f4").tobytes())

# A fixed input fed twice, and the logits of the second time, to check another implementation
# against this one, memory included.
probe = torch.sin(torch.arange(saved["obs_len"], dtype=torch.float32) * 0.37).unsqueeze(0)
with torch.no_grad():
    _, _, state = policy(probe, policy.blank(1, "cpu"))
    logits = torch.cat(policy(probe, state)[0], dim=-1)[0]
print(f"{out}: obs {saved['obs_len']}, hidden {saved['hidden']}, memory {MEMORY}, heads {HEADS}")
print("probe logits:", " ".join(f"{v:.4f}" for v in logits.tolist()))
