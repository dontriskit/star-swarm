"""PPO for Star Swarm ships: one shared policy flies every ship.

Usage (from the repo root, after `maturin develop --release` in crates/swarm-py):

    .venv/bin/python train/ppo.py --name win --updates 300 \\
        --reward win=50 --reward lose=50 --reward base_hit=0.05 --reward base_lost=0.02 --reward hurt=0.02

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

# Sizes of the six action heads: thrust, turn, fire, boost, buy, vote on the team plan.
HEADS = (2, 3, 2, 2, 12, 5)
PLANS = ("farm", "defend", "attack_a", "attack_b")
REWARDS = ("collect", "rock", "buy", "hit", "kill", "crash", "death", "base_hit", "base_lost", "win", "lose", "hurt")
CLASSES = ("basic", "lancer", "twin", "scatter")
UPGRADES = ("attack", "health", "fire_rate", "shield", "speed", "regen", "agility", "ammo")


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


def forget(policy: Policy, how: str, device, hidden: int):
    """Ways to loosen a loaded policy's grip on what it learned, before training it on something new."""
    fresh = Policy(hidden).to(device)
    with torch.no_grad():
        if how == "head":
            # Keep what the network sees (the trunk), forget what it does with it.
            policy.logits.load_state_dict(fresh.logits.state_dict())
            policy.value.load_state_dict(fresh.value.state_dict())
        elif how == "shrink":
            # Shrink and perturb: pull every weight halfway to zero and add a fresh random network on top.
            for old, new in zip(policy.parameters(), fresh.parameters()):
                old.mul_(0.5).add_(new, alpha=0.5)
        elif how != "none":
            raise ValueError(f"unknown --forget: {how}")


def pack(actions: torch.Tensor) -> bytes:
    """One little-endian uint16 per agent, the encoding Env.step expects."""
    a = actions
    code = a[:, 0] | (a[:, 1] << 1) | (a[:, 2] << 3) | (a[:, 3] << 4) | (a[:, 4] << 5) | (a[:, 5] << 9)
    return code.cpu().numpy().astype("<u2").tobytes()


def observe(env: Env, device) -> torch.Tensor:
    return torch.tensor(np.frombuffer(env.observe(), dtype=np.float32), device=device).view(env.agents, OBS_LEN)


def make_env(args, envs: int, seed: int, learners: int | None = None) -> Env:
    return Env(
        envs,
        seed=seed,
        frame_skip=args.frame_skip,
        teams=args.teams,
        ships_per_team=args.ships_per_team,
        rock_count=args.rocks,
        radius=args.radius,
        max_ticks=args.max_ticks,
        # Nothing is rewarded unless asked for with --reward.
        rewards={**dict.fromkeys(REWARDS, 0.0), **{name: float(weight) for name, weight in (pair.split("=") for pair in args.reward)}},
        learners=learners,
        base_hp=args.base_hp,
        turret_damage=args.turret_damage,
    )


def controlled(env: Env, device) -> torch.Tensor:
    """True for the agents whose actions the environment takes from us."""
    return torch.tensor(np.frombuffer(env.controlled(), dtype=np.uint8), device=device).bool()


def summarize(stats) -> dict:
    """Team 0 over finished matches: how often it won or lost its base, and per-ship averages."""
    s = np.array(stats, dtype=np.float64)
    minutes = s[:, 0].mean()
    return {
        "matches": len(stats),
        "win_rate": float(s[:, 5].mean()),
        "loss_rate": float(s[:, 6].mean()),
        "minutes": float(minutes),
        "crystals_per_min": float(s[:, 1].mean() / minutes),
        "levels": float(s[:, 2].mean()),
        "kills": float(s[:, 3].mean()),
        "deaths": float(s[:, 4].mean()),
        # What the ships turned into: share of each class and mean level of each upgrade at the end.
        "classes": {name: round(float(s[:, 9 + i].mean()), 3) for i, name in enumerate(CLASSES)},
        "upgrades": {name: round(float(s[:, 13 + i].mean()), 2) for i, name in enumerate(UPGRADES)},
        "tiers": [round(float(s[:, 21 + i].mean()), 3) for i in range(3)],
        # How much of a match the team spent under each plan it voted in.
        "plans": {name: round(float(s[:, 24 + i].mean()), 3) for i, name in enumerate(PLANS)},
    }


@torch.no_grad()
def play(args, choose, envs: int, seed: int, device) -> dict:
    """Plays one round of matches in fresh environments; `choose(env, obs)` returns action bytes.

    Team 0 is flown by `choose`, every other team by the scripted bot.
    """
    env = make_env(args, envs, seed, learners=0 if choose is scripted else 1)
    stats = []
    while len(stats) < envs:
        actions = choose(env, None if choose is scripted else observe(env, device))
        env.step(actions)
        # Only the first match of every slot counts; taking whichever matches end
        # first would favour short ones.
        stats.extend(s for s in env.take_stats() if s[8] == 0)
    return summarize(stats)


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

    # Against "self" every team takes actions from us: team 0 from the policy being trained,
    # the others from a frozen earlier copy of it. Against "scripted" only team 0 is ours.
    self_play = args.opponent == "self"
    env = make_env(args, args.envs, args.seed, learners=args.teams if self_play else 1)
    team = torch.arange(env.agents, device=device) % env.ships // args.ships_per_team
    ours = (team == 0).nonzero().squeeze(-1)
    theirs = (team != 0).nonzero().squeeze(-1)
    n, steps = len(ours), args.steps
    everyone = torch.zeros(env.agents, len(HEADS), dtype=torch.long, device=device)
    rival = Policy(args.hidden).to(device)
    past = []
    policy = Policy(args.hidden).to(device)
    if args.init:
        policy.load_state_dict(torch.load(args.init, map_location=device)["model"])
        forget(policy, args.forget, device, args.hidden)
    optim = torch.optim.Adam(policy.parameters(), lr=args.lr, eps=1e-5)

    obs_buf = torch.zeros(steps, n, OBS_LEN, device=device)
    act_buf = torch.zeros(steps, n, len(HEADS), dtype=torch.long, device=device)
    logp_buf = torch.zeros(steps, n, device=device)
    val_buf = torch.zeros(steps, n, device=device)
    rew_buf = torch.zeros(steps, n, device=device)
    done_buf = torch.zeros(steps, n, device=device)

    full = observe(env, device)
    obs = full[ours]
    stats, started, samples = [], time.time(), 0
    for update in range(1, args.updates + 1):
        # Exploration bonus fades linearly, so the policy ends up decisive.
        entropy_coef = args.entropy_coef + (args.entropy_end - args.entropy_coef) * (update - 1) / max(args.updates - 1, 1)
        if self_play and (update - 1) % args.snapshot_every == 0:
            # Keep a few earlier versions and play this stretch against one of them, mostly the newest.
            past = (past + [{k: v.clone() for k, v in policy.state_dict().items()}])[-args.snapshots:]
            rival.load_state_dict(past[-1] if torch.rand(()) < 0.5 else past[int(torch.randint(len(past), ()))])
        with torch.no_grad():
            for t in range(steps):
                actions, logp, value = policy.act(obs)
                everyone[ours] = actions
                if self_play:
                    everyone[theirs] = rival.act(full[theirs])[0]
                rewards, dones = env.step(pack(everyone))
                obs_buf[t], act_buf[t], logp_buf[t], val_buf[t] = obs, actions, logp, value
                rew_buf[t] = torch.tensor(np.frombuffer(rewards, dtype=np.float32), device=device)[ours]
                done_buf[t] = torch.tensor(np.frombuffer(dones, dtype=np.uint8), device=device)[ours].float()
                full = observe(env, device)
                obs = full[ours]
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
    p.add_argument("--name", default="win")
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
    p.add_argument("--gamma", type=float, default=0.995)
    p.add_argument("--lam", type=float, default=0.95)
    p.add_argument("--clip", type=float, default=0.2)
    p.add_argument("--value-coef", type=float, default=0.5)
    p.add_argument("--entropy-coef", type=float, default=0.01)
    p.add_argument("--entropy-end", type=float, default=None, help="entropy coefficient at the last update (default: no fading)")
    p.add_argument("--init", default=None, metavar="POLICY.PT", help="start from a saved policy")
    p.add_argument("--forget", default="none", choices=["none", "head", "shrink"],
                   help="with --init: reset the action and value heads, or shrink every weight and add noise")
    p.add_argument("--log-every", type=int, default=10)
    # The match: the full game, three teams of five, up to five minutes, a decision every 0.1 s.
    p.add_argument("--frame-skip", type=int, default=3)
    p.add_argument("--teams", type=int, default=3)
    p.add_argument("--ships-per-team", type=int, default=5)
    p.add_argument("--rocks", type=int, default=420)
    p.add_argument("--radius", type=float, default=1500.0, help="radius of the planet the match is played around")
    p.add_argument("--max-ticks", type=int, default=18000)
    p.add_argument("--base-hp", type=float, default=1000.0)
    p.add_argument("--turret-damage", type=float, default=15.0, help="damage per base turret shot; 0 switches turrets off")
    p.add_argument("--opponent", default="self", choices=["self", "scripted"],
                   help="who flies the other teams in training: earlier copies of the policy, or the scripted bot")
    p.add_argument("--snapshot-every", type=int, default=10, help="updates between new frozen copies for self-play")
    p.add_argument("--snapshots", type=int, default=6, help="how many frozen copies to keep")
    p.add_argument("--reward", action="append", default=[], metavar="NAME=WEIGHT", help="override a reward weight, e.g. --reward buy=4")
    args = p.parse_args()
    if args.entropy_end is None:
        args.entropy_end = args.entropy_coef
    train(args)
