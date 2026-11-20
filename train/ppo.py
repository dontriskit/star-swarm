"""PPO for Star Swarm ships: one shared policy flies every ship.

Usage (from the repo root, after `maturin develop --release` in crates/swarm-py):

    .venv/bin/python train/ppo.py --name mine --updates 300

Writes runs/<name>/metrics.jsonl and runs/<name>/policy.pt.
"""

import argparse
import json
import pathlib
import time

import numpy as np
import torch
from torch import nn

from swarm_py import OBS_LEN, Env

# Sizes of the four action heads: thrust, turn, fire, buy.
HEADS = (2, 3, 2, 9)


class Policy(nn.Module):
    def __init__(self, hidden: int = 256):
        super().__init__()
        self.trunk = nn.Sequential(nn.Linear(OBS_LEN, hidden), nn.ReLU(), nn.Linear(hidden, hidden), nn.ReLU())
        self.logits = nn.Linear(hidden, sum(HEADS))
        self.value = nn.Linear(hidden, 1)

    def forward(self, obs):
        h = self.trunk(obs)
        return self.logits(h).split(HEADS, dim=-1), self.value(h).squeeze(-1)

    def act(self, obs, greedy: bool = False):
        logits, value = self(obs)
        dists = [torch.distributions.Categorical(logits=l) for l in logits]
        actions = torch.stack([l.argmax(-1) if greedy else d.sample() for l, d in zip(logits, dists)], dim=-1)
        return actions, self.log_prob(dists, actions), value

    @staticmethod
    def log_prob(dists, actions):
        return sum(d.log_prob(actions[..., i]) for i, d in enumerate(dists))

    def evaluate(self, obs, actions):
        logits, value = self(obs)
        dists = [torch.distributions.Categorical(logits=l) for l in logits]
        return self.log_prob(dists, actions), sum(d.entropy() for d in dists), value


def pack(actions: torch.Tensor) -> bytes:
    """One byte per agent, the encoding Env.step expects."""
    a = actions
    return (a[:, 0] | (a[:, 1] << 1) | (a[:, 2] << 3) | (a[:, 3] << 4)).to(torch.uint8).cpu().numpy().tobytes()


def observe(env: Env, device) -> torch.Tensor:
    return torch.tensor(np.frombuffer(env.observe(), dtype=np.float32), device=device).view(env.agents, OBS_LEN)


def make_env(args, envs: int, seed: int) -> Env:
    return Env(
        envs,
        seed=seed,
        frame_skip=args.frame_skip,
        teams=args.teams,
        ships_per_team=args.ships_per_team,
        rock_count=args.rocks,
        world_size=args.world_size,
        max_ticks=args.max_ticks,
        rewards={name: float(weight) for name, weight in (pair.split("=") for pair in args.reward)},
    )


def summarize(stats) -> dict:
    """Per-ship averages over finished matches."""
    s = np.array(stats, dtype=np.float64)
    minutes = s[:, 0].mean()
    return {
        "matches": len(stats),
        "crystals_per_min": float(s[:, 1].mean() / minutes),
        "levels": float(s[:, 2].mean()),
        "kills": float(s[:, 3].mean()),
        "deaths": float(s[:, 4].mean()),
    }


@torch.no_grad()
def play(args, choose, envs: int, seed: int, device) -> dict:
    """Plays one round of matches in fresh environments; `choose(env, obs)` returns action bytes."""
    env = make_env(args, envs, seed)
    stats = []
    while len(stats) < envs:
        actions = choose(env, None if choose is scripted else observe(env, device))
        env.step(actions)
        stats.extend(env.take_stats())
    return summarize(stats[:envs])


def scripted(env: Env, _obs) -> bytes:
    return env.scripted()


def train(args):
    device = torch.device("cuda" if torch.cuda.is_available() and not args.cpu else "cpu")
    torch.manual_seed(args.seed)
    out = pathlib.Path("runs") / args.name
    out.mkdir(parents=True, exist_ok=True)
    log = (out / "metrics.jsonl").open("w")

    def record(row: dict):
        log.write(json.dumps(row) + "\n")
        log.flush()
        print(json.dumps(row), flush=True)

    baseline = play(args, scripted, args.eval_envs, 1_000_000, device)
    record({"kind": "scripted", **baseline})

    env = make_env(args, args.envs, args.seed)
    n, steps = env.agents, args.steps
    policy = Policy(args.hidden).to(device)
    if args.init:
        policy.load_state_dict(torch.load(args.init, map_location=device)["model"])
    optim = torch.optim.Adam(policy.parameters(), lr=args.lr, eps=1e-5)

    obs_buf = torch.zeros(steps, n, OBS_LEN, device=device)
    act_buf = torch.zeros(steps, n, len(HEADS), dtype=torch.long, device=device)
    logp_buf = torch.zeros(steps, n, device=device)
    val_buf = torch.zeros(steps, n, device=device)
    rew_buf = torch.zeros(steps, n, device=device)
    done_buf = torch.zeros(steps, n, device=device)

    obs = observe(env, device)
    stats, started, samples = [], time.time(), 0
    for update in range(1, args.updates + 1):
        # Exploration bonus fades linearly, so the policy ends up decisive.
        entropy_coef = args.entropy_coef + (args.entropy_end - args.entropy_coef) * (update - 1) / max(args.updates - 1, 1)
        with torch.no_grad():
            for t in range(steps):
                actions, logp, value = policy.act(obs)
                rewards, dones = env.step(pack(actions))
                obs_buf[t], act_buf[t], logp_buf[t], val_buf[t] = obs, actions, logp, value
                rew_buf[t] = torch.tensor(np.frombuffer(rewards, dtype=np.float32), device=device)
                done_buf[t] = torch.tensor(np.frombuffer(dones, dtype=np.uint8), device=device).float()
                obs = observe(env, device)
            stats.extend(env.take_stats())

            # Generalised advantage estimation; a finished match cuts the return.
            adv = torch.zeros(steps, n, device=device)
            last = torch.zeros(n, device=device)
            next_value = policy(obs)[1]
            for t in reversed(range(steps)):
                alive = 1.0 - done_buf[t]
                delta = rew_buf[t] + args.gamma * next_value * alive - val_buf[t]
                last = delta + args.gamma * args.lam * alive * last
                adv[t] = last
                next_value = val_buf[t]
            returns = adv + val_buf

        # A destroyed ship's actions do nothing until it respawns; leave those samples out.
        keep = (obs_buf[..., 0] > 0.5).reshape(-1).nonzero().squeeze(-1)
        flat = lambda x: x.reshape(steps * n, *x.shape[2:])
        b_obs, b_act, b_logp, b_adv, b_ret = (flat(x) for x in (obs_buf, act_buf, logp_buf, adv, returns))
        for _ in range(args.epochs):
            order = keep[torch.randperm(len(keep), device=device)]
            for start in range(0, len(order), args.minibatch):
                idx = order[start : start + args.minibatch]
                logp, entropy, value = policy.evaluate(b_obs[idx], b_act[idx])
                a = b_adv[idx]
                a = (a - a.mean()) / (a.std() + 1e-8)
                ratio = (logp - b_logp[idx]).exp()
                policy_loss = -torch.min(ratio * a, ratio.clamp(1 - args.clip, 1 + args.clip) * a).mean()
                value_loss = 0.5 * (value - b_ret[idx]).pow(2).mean()
                loss = policy_loss + args.value_coef * value_loss - entropy_coef * entropy.mean()
                optim.zero_grad(set_to_none=True)
                loss.backward()
                nn.utils.clip_grad_norm_(policy.parameters(), 0.5)
                optim.step()

        samples += steps * n
        if update % args.log_every == 0 or update == args.updates:
            row = {
                "kind": "train",
                "update": update,
                "samples": samples,
                "samples_per_s": round(samples / (time.time() - started)),
                "reward_per_decision": float(rew_buf.mean()),
                "value_loss": value_loss.item(),
                "entropy": entropy.mean().item(),
            }
            if stats:
                row.update(summarize(stats))
                stats = []
            record(row)
            torch.save({"model": policy.state_dict(), "hidden": args.hidden, "obs_len": OBS_LEN, "heads": HEADS}, out / "policy.pt")

    sampled = play(args, lambda e, o: pack(policy.act(o)[0]), args.eval_envs, 1_000_000, device)
    greedy = play(args, lambda e, o: pack(policy.act(o, greedy=True)[0]), args.eval_envs, 1_000_000, device)
    record({"kind": "eval", "scripted": baseline, "policy_sampled": sampled, "policy_greedy": greedy})


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--name", default="mine")
    p.add_argument("--seed", type=int, default=1)
    p.add_argument("--cpu", action="store_true")
    p.add_argument("--updates", type=int, default=300)
    p.add_argument("--envs", type=int, default=512)
    p.add_argument("--eval-envs", type=int, default=256)
    p.add_argument("--steps", type=int, default=64, help="decisions per agent per update")
    p.add_argument("--epochs", type=int, default=4)
    p.add_argument("--minibatch", type=int, default=8192)
    p.add_argument("--hidden", type=int, default=256)
    p.add_argument("--lr", type=float, default=3e-4)
    p.add_argument("--gamma", type=float, default=0.99)
    p.add_argument("--lam", type=float, default=0.95)
    p.add_argument("--clip", type=float, default=0.2)
    p.add_argument("--value-coef", type=float, default=0.5)
    p.add_argument("--entropy-coef", type=float, default=0.01)
    p.add_argument("--entropy-end", type=float, default=None, help="entropy coefficient at the last update (default: no fading)")
    p.add_argument("--init", default=None, metavar="POLICY.PT", help="start from a saved policy")
    p.add_argument("--log-every", type=int, default=10)
    # The match: one team mining, three minutes long, a decision every 0.1 s.
    p.add_argument("--frame-skip", type=int, default=3)
    p.add_argument("--teams", type=int, default=1)
    p.add_argument("--ships-per-team", type=int, default=4)
    p.add_argument("--rocks", type=int, default=40)
    p.add_argument("--world-size", type=float, default=1400.0)
    p.add_argument("--max-ticks", type=int, default=5400)
    p.add_argument("--reward", action="append", default=[], metavar="NAME=WEIGHT", help="override a reward weight, e.g. --reward buy=4")
    args = p.parse_args()
    if args.entropy_end is None:
        args.entropy_end = args.entropy_coef
    train(args)
