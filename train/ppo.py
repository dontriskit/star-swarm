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

# Sizes of the seven action heads: thrust, turn, fire, boost, buy, mode (what the ship sets out
# to do), deposit (hand crystals over while inside the base).
HEADS = (2, 3, 2, 2, 18, 5, 2, 2)
PLANS = ("farm", "defend", "attack_a", "attack_b")
REWARDS = ("collect", "rock", "buy", "hit", "kill", "crash", "death", "base_hit", "base_lost", "win", "lose", "hurt", "time", "draw", "deposit", "base_level", "rock_hit", "edge", "hull", "bare", "place", "tier")
CLASSES = ("basic", "lancer", "twin", "scatter", "ram")
UPGRADES = ("attack", "health", "fire_rate", "shield", "speed", "regen", "agility", "ammo")


# Width of a ship's memory: what it carries from one decision to the next.
MEMORY = 256


class Policy(nn.Module):
    """Two layers read the observation; a memory cell (a GRU) carries something on from one
    decision to the next, and what it holds is added to what the layers made of the observation.
    The addition starts at zero, so a policy without a memory behaves the same once given one,
    until training finds a use for it."""

    def __init__(self, hidden: int = 256):
        super().__init__()
        self.trunk = nn.Sequential(nn.Linear(OBS_LEN, hidden), nn.ReLU(), nn.Linear(hidden, hidden), nn.ReLU())
        self.memory = nn.GRUCell(hidden, MEMORY)
        self.recall = nn.Linear(MEMORY, hidden)
        nn.init.zeros_(self.recall.weight)
        nn.init.zeros_(self.recall.bias)
        self.logits = nn.Linear(hidden, sum(HEADS))
        self.value = nn.Linear(hidden, 1)

    def blank(self, count: int, device) -> torch.Tensor:
        """The memory of ships that remember nothing yet."""
        return torch.zeros(count, MEMORY, device=device)

    def forward(self, obs, state=None):
        """Logits per head, the value, and the memory after this decision. Without a memory
        passed in, the ship decides on the observation alone and none is returned."""
        h = self.trunk(obs)
        if state is not None:
            state = self.memory(h, state)
            h = h + self.recall(state)
        return self.logits(h).split(HEADS, dim=-1), self.value(h).squeeze(-1), state

    def act(self, obs, state=None, greedy: bool = False):
        logits, value, state = self(obs, state)
        dists = [torch.distributions.Categorical(logits=l) for l in logits]
        actions = torch.stack([l.argmax(-1) if greedy else d.sample() for l, d in zip(logits, dists)], dim=-1)
        return actions, self.log_prob(dists, actions), value, state

    @staticmethod
    def log_prob(dists, actions):
        return sum(d.log_prob(actions[..., i]) for i, d in enumerate(dists))

    def evaluate(self, obs, actions, state=None):
        logits, value, state = self(obs, state)
        dists = [torch.distributions.Categorical(logits=l) for l in logits]
        return self.log_prob(dists, actions), sum(d.entropy() for d in dists), value, state


class Pilot:
    """Flies every ship of a set of matches with a policy, keeping each ship's memory between
    decisions and wiping it when its match ends."""

    def __init__(self, policy: Policy, greedy: bool):
        self.policy, self.greedy, self.state = policy, greedy, None

    def __call__(self, env: Env, obs) -> bytes:
        if self.state is None:
            self.state = self.policy.blank(len(obs), obs.device)
        actions, _, _, self.state = self.policy.act(obs, self.state, greedy=self.greedy)
        return pack(actions)

    def after(self, dones: bytes):
        ended = torch.tensor(np.frombuffer(dones, dtype=np.uint8), device=self.state.device).bool()
        self.state[ended] = 0.0


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
    code = a[:, 0] | (a[:, 1] << 1) | (a[:, 2] << 3) | (a[:, 3] << 4) | (a[:, 4] << 5) | (a[:, 5] << 10) | (a[:, 6] << 13) | (a[:, 7] << 14)
    return code.cpu().numpy().astype("<u2").tobytes()


_staging = {}


def observe(env: Env, device) -> torch.Tensor:
    """Every agent's observation as a tensor on `device`.

    The environment writes straight into one reusable page-locked tensor per environment, which
    is then copied to the GPU: one copy in all, where going through bytes took three.
    """
    held = _staging.get(id(env))
    if held is None:
        staged = torch.empty(env.agents, OBS_LEN)
        if torch.device(device).type == "cuda":
            staged = staged.pin_memory()
        held = _staging[id(env)] = (env, staged, staged.numpy())
    _, staged, view = held
    env.observe_into(view)
    # A blocking copy: the staging tensor is overwritten by the next call. On the CPU, hand out a copy.
    return staged.to(device) if torch.device(device).type == "cuda" else staged.clone()


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
        "upgrades": {name: round(float(s[:, 14 + i].mean()), 2) for i, name in enumerate(UPGRADES)},
        "tiers": [round(float(s[:, 22 + i].mean()), 3) for i in range(7)],
        # Share of ship-time spent in each mode.
        "plans": {name: round(float(s[:, 29 + i].mean()), 3) for i, name in enumerate(PLANS)},
        "base_level": round(float(s[:, 33].mean()), 2),
        "deposited_per_min": round(float(s[:, 34].mean() / minutes), 2),
        "drones": round(float(s[:, 35].mean()), 2),
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
        _, dones = env.step(actions)
        if isinstance(choose, Pilot):
            choose.after(dones)
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
    # One of the training matches is streamed for `swarm-gui --watch` to show live.
    env.stream(str(out / "live.swl"))
    team = torch.arange(env.agents, device=device) % env.ships // args.ships_per_team
    ours = (team == 0).nonzero().squeeze(-1)
    theirs = (team != 0).nonzero().squeeze(-1)
    n, steps = len(ours), args.steps
    everyone = torch.zeros(env.agents, len(HEADS), dtype=torch.long, device=device)
    rival = Policy(args.hidden).to(device)
    past = []
    policy = Policy(args.hidden).to(device)
    if args.init:
        checkpoint = torch.load(args.init, map_location=device)
        saved = checkpoint["model"]
        old_heads = tuple(checkpoint.get("heads", HEADS))
        if old_heads != HEADS:
            # The actions have grown since this policy was saved: a head got more choices, or a
            # head was added. What it learned is copied head by head; every new choice starts
            # from a zero logit, which a head of its own turns into an even split.
            for key in ("logits.weight", "logits.bias"):
                grown = torch.zeros(sum(HEADS), *saved[key].shape[1:], device=device)
                at_old = at_new = 0
                for i, size in enumerate(HEADS):
                    was = old_heads[i] if i < len(old_heads) else 0
                    grown[at_new : at_new + min(was, size)] = saved[key][at_old : at_old + min(was, size)]
                    if was == 0 and key == "logits.bias":
                        # A head that did not exist starts out mostly on its first choice ("no"),
                        # so the policy flies as it did before it had the choice.
                        grown[at_new] = 2.0
                    at_old, at_new = at_old + was, at_new + size
                saved[key] = grown
        first = "trunk.0.weight"
        if saved[first].shape[1] < OBS_LEN:
            # The observation has grown since this policy was saved. New inputs are added at the
            # end, so the old policy carries over with zero weights on them: it behaves exactly
            # as before until training teaches it to use them.
            grown = torch.zeros(saved[first].shape[0], OBS_LEN, device=device)
            grown[:, : saved[first].shape[1]] = saved[first]
            saved[first] = grown
        # A policy saved before ships had a memory comes without one: it gets a fresh one,
        # which changes nothing until training puts it to use.
        missing = policy.load_state_dict(saved, strict=False)
        assert not missing.unexpected_keys and all(k.startswith(("memory.", "recall.")) for k in missing.missing_keys), missing
        forget(policy, args.forget, device, args.hidden)
    optim = torch.optim.Adam(policy.parameters(), lr=args.lr, eps=1e-5)

    obs_buf = torch.zeros(steps, n, OBS_LEN, device=device)
    act_buf = torch.zeros(steps, n, len(HEADS), dtype=torch.long, device=device)
    logp_buf = torch.zeros(steps, n, device=device)
    val_buf = torch.zeros(steps, n, device=device)
    rew_buf = torch.zeros(steps, n, device=device)
    done_buf = torch.zeros(steps, n, device=device)
    # Each ship's memory as it was before each decision, to start a stretch from when learning.
    mem_buf = torch.zeros(steps, n, MEMORY, device=device)
    state, rival_state = policy.blank(n, device), policy.blank(len(theirs), device)

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
                mem_buf[t] = state
                actions, logp, value, state = policy.act(obs, state)
                everyone[ours] = actions
                if self_play:
                    rival_actions, _, _, rival_state = rival.act(full[theirs], rival_state)
                    everyone[theirs] = rival_actions
                rewards, dones = env.step(pack(everyone))
                obs_buf[t], act_buf[t], logp_buf[t], val_buf[t] = obs, actions, logp, value
                rew_buf[t] = torch.tensor(np.frombuffer(rewards, dtype=np.float32), device=device)[ours]
                ended = torch.tensor(np.frombuffer(dones, dtype=np.uint8), device=device).float()
                done_buf[t] = ended[ours]
                # A new match starts with nothing remembered.
                state = state * (1.0 - ended[ours]).unsqueeze(-1)
                rival_state = rival_state * (1.0 - ended[theirs]).unsqueeze(-1)
                full = observe(env, device)
                obs = full[ours]
            stats.extend(env.take_stats())

            # Generalised advantage estimation; a finished match cuts the return.
            adv = torch.zeros(steps, n, device=device)
            last = torch.zeros(n, device=device)
            next_value = policy(obs, state)[1]
            for t in reversed(range(steps)):
                alive = 1.0 - done_buf[t]
                delta = rew_buf[t] + args.gamma * next_value * alive - val_buf[t]
                last = delta + args.gamma * args.lam * alive * last
                adv[t] = last
                next_value = val_buf[t]
            returns = adv + val_buf

        # Learning goes over stretches of consecutive decisions of one ship, so that the memory
        # can be taught: each stretch starts from the memory the ship had there and is played
        # through again. A destroyed ship's actions do nothing until it respawns; those
        # decisions are played through but not learned from.
        span = min(args.span, steps)
        starts = torch.arange(0, steps - span + 1, span, device=device)
        pairs = torch.cartesian_prod(starts, torch.arange(n, device=device))
        alive_buf = (obs_buf[..., 0] > 0.5).float()
        per_batch = max(args.minibatch // span, 1)
        offsets = torch.arange(span, device=device).unsqueeze(1)
        for _ in range(args.epochs):
            order = pairs[torch.randperm(len(pairs), device=device)]
            for start in range(0, len(order), per_batch):
                t0, who = order[start : start + per_batch].unbind(1)
                rows = t0.unsqueeze(0) + offsets  # [span, batch]: the steps of each stretch
                cols = who.unsqueeze(0).expand_as(rows)
                h = mem_buf[t0, who]
                logps, entropies, values = [], [], []
                for k in range(span):
                    logp_k, entropy_k, value_k, h = policy.evaluate(obs_buf[rows[k], cols[k]], act_buf[rows[k], cols[k]], h)
                    h = h * (1.0 - done_buf[rows[k], cols[k]]).unsqueeze(-1)
                    logps.append(logp_k)
                    entropies.append(entropy_k)
                    values.append(value_k)
                logp, entropy, value = torch.stack(logps), torch.stack(entropies), torch.stack(values)
                mask = alive_buf[rows, cols]
                count = mask.sum().clamp(min=1.0)
                mean = lambda x: (x * mask).sum() / count
                a = adv[rows, cols]
                a = (a - mean(a)) / ((mean((a - mean(a)) ** 2)).sqrt() + 1e-8)
                ratio = (logp - logp_buf[rows, cols]).exp()
                policy_loss = -mean(torch.min(ratio * a, ratio.clamp(1 - args.clip, 1 + args.clip) * a))
                value_loss = 0.5 * mean((value - returns[rows, cols]).pow(2))
                loss = policy_loss + args.value_coef * value_loss - entropy_coef * mean(entropy)
                optim.zero_grad(set_to_none=True)
                loss.backward()
                if update <= args.value_warmup:
                    # A policy taught elsewhere comes without a sense of what a position is worth.
                    # Until it has one, only the value head learns: the rest is left as it came.
                    for name, param in policy.named_parameters():
                        if not name.startswith("value."):
                            param.grad = None
                nn.utils.clip_grad_norm_(policy.parameters(), 0.5)
                optim.step()
        entropy = mean(entropy)

        samples += steps * n
        if update % args.log_every == 0 or update == args.updates:
            row = {
                "kind": "train",
                "update": update,
                "samples": samples,
                "samples_per_s": round(samples / (time.time() - started)),
                "reward_per_decision": float(rew_buf.mean()),
                "value_loss": value_loss.item(),
                "entropy": entropy.item(),
            }
            if stats:
                row.update(summarize(stats))
                stats = []
            record(row)
            torch.save({"model": policy.state_dict(), "hidden": args.hidden, "obs_len": OBS_LEN, "heads": HEADS, "memory": MEMORY}, out / "policy.pt")

    sampled = play(args, Pilot(policy, greedy=False), args.eval_envs, 1_000_000, device)
    greedy = play(args, Pilot(policy, greedy=True), args.eval_envs, 1_000_000, device)
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
    p.add_argument("--span", type=int, default=16, help="consecutive decisions in a stretch learned from: how far back the memory is taught to reach")
    p.add_argument("--hidden", type=int, default=256)
    p.add_argument("--lr", type=float, default=3e-4)
    p.add_argument("--gamma", type=float, default=0.999)
    p.add_argument("--lam", type=float, default=0.95)
    p.add_argument("--clip", type=float, default=0.2)
    p.add_argument("--value-coef", type=float, default=0.5)
    p.add_argument("--entropy-coef", type=float, default=0.01)
    p.add_argument("--entropy-end", type=float, default=None, help="entropy coefficient at the last update (default: no fading)")
    p.add_argument("--init", default=None, metavar="POLICY.PT", help="start from a saved policy")
    p.add_argument("--value-warmup", type=int, default=0, help="updates at the start in which only the value head learns")
    p.add_argument("--forget", default="none", choices=["none", "head", "shrink"],
                   help="with --init: reset the action and value heads, or shrink every weight and add noise")
    p.add_argument("--log-every", type=int, default=10)
    # The match: the full game, three teams of five, up to five minutes, a decision every 0.1 s.
    p.add_argument("--frame-skip", type=int, default=3)
    p.add_argument("--teams", type=int, default=3)
    p.add_argument("--ships-per-team", type=int, default=7)
    p.add_argument("--rocks", type=int, default=1760)
    p.add_argument("--radius", type=float, default=4800.0, help="radius of the planet the match is played around")
    p.add_argument("--max-ticks", type=int, default=36000)
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
