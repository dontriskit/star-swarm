"""Teaches a fresh policy to fly like the scripted bot, as a starting point for train/ppo.py.

Matches are played from start to finish so every phase is seen: mining, flying home, docking,
handing crystals over, shopping, fighting. Half of the matches are flown by the scripted bot
and half by the policy being taught, and in both the lesson is what the scripted bot would
have done in that spot (so the policy also learns how to get out of its own mistakes).

    .venv/bin/python train/pretrain.py --name pre --steps 12000
    train/curriculum.sh globe runs/pre/policy.pt 3
"""

import argparse
import json
import time
from pathlib import Path

import numpy as np
import torch
import torch.nn.functional as F

from ppo import HEADS, MEMORY, OBS_LEN, Policy, observe, pack
from swarm_py import Env

# Where each head's choice sits in the 16-bit action: shift and mask, in the order of HEADS.
BITS = ((0, 1), (1, 3), (3, 1), (4, 1), (5, 31), (10, 7), (13, 1), (14, 1))


def unpack(raw: bytes, device) -> torch.Tensor:
    code = torch.from_numpy(np.frombuffer(raw, dtype="<u2").astype(np.int64)).to(device)
    return torch.stack([(code >> shift) & mask for shift, mask in BITS], dim=-1)


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--name", default="pre")
    p.add_argument("--seed", type=int, default=7)
    p.add_argument("--envs", type=int, default=256)
    p.add_argument("--steps", type=int, default=18000, help="decisions per match side; 18000 is one whole 45-minute match")
    p.add_argument("--hidden", type=int, default=1024)
    p.add_argument("--lr", type=float, default=1e-3)
    p.add_argument("--every", type=int, default=32, help="decisions gathered between lessons")
    p.add_argument("--minibatch", type=int, default=16384)
    p.add_argument("--own-share", type=float, default=0.5, help="share of matches flown by the policy itself")
    p.add_argument("--init", default=None, metavar="POLICY.PT", help="start from a saved policy instead of a fresh one")
    args = p.parse_args()

    device = "cuda" if torch.cuda.is_available() else "cpu"
    torch.manual_seed(args.seed)
    out = Path("runs") / args.name
    out.mkdir(parents=True, exist_ok=True)
    env = Env(args.envs, seed=args.seed, learners=3)
    policy = Policy(args.hidden).to(device)
    if args.init:
        policy.load_state_dict(torch.load(args.init, map_location=device)["model"], strict=False)
    optim = torch.optim.Adam(policy.parameters(), lr=args.lr)
    # The first matches are the scripted bot's, the rest the policy's own.
    own = (torch.arange(env.agents, device=device) // env.ships) >= int(args.envs * (1 - args.own_share))
    seen_obs, seen_act = [], []
    started = time.time()
    for step in range(1, args.steps + 1):
        obs = observe(env, device)
        expert = unpack(env.scripted(), device)
        with torch.no_grad():
            # Taught without its memory: the scripted bot has none to imitate.
            mine = policy.act(obs)[0]
        env.step(pack(torch.where(own.unsqueeze(-1), mine, expert)))
        seen_obs.append(obs)
        seen_act.append(expert)
        if step % args.every:
            continue
        obs_all, act_all = torch.cat(seen_obs), torch.cat(seen_act)
        seen_obs, seen_act = [], []
        order = torch.randperm(len(obs_all), device=device)
        for at in range(0, len(order), args.minibatch):
            idx = order[at : at + args.minibatch]
            logits = policy(obs_all[idx])[0]
            loss = sum(F.cross_entropy(l, act_all[idx, i]) for i, l in enumerate(logits))
            optim.zero_grad()
            loss.backward()
            optim.step()
        if step % (args.every * 16) == 0 or step == args.steps:
            with torch.no_grad():
                logits = policy(obs_all)[0]
                same = [round((l.argmax(-1) == act_all[:, i]).float().mean().item(), 3) for i, l in enumerate(logits)]
            line = {"step": step, "game_minutes": round(step * 0.15 / 60, 1), "loss": round(loss.item(), 4), "agrees": same, "seconds": round(time.time() - started)}
            print(json.dumps(line), flush=True)
            torch.save({"model": policy.state_dict(), "hidden": args.hidden, "obs_len": OBS_LEN, "heads": HEADS, "memory": MEMORY}, out / "policy.pt")


if __name__ == "__main__":
    main()
