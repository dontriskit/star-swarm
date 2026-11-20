"""Where a rollout step spends its time, at the training settings in use."""
import sys, time
sys.path.insert(0, "train")
import numpy as np, torch
from ppo import Policy, HEADS, pack, observe
from swarm_py import Env, OBS_LEN

dev = torch.device("cuda")
envs, hidden = 2048, 1024
env = Env(envs, seed=5, frame_skip=3, teams=3, ships_per_team=7, rock_count=1760, radius=4800.0, max_ticks=36000)
team = torch.arange(env.agents, device=dev) % env.ships // 9
ours, theirs = (team == 0).nonzero().squeeze(-1), (team != 0).nonzero().squeeze(-1)
policy, rival = Policy(hidden).to(dev), Policy(hidden).to(dev)
everyone = torch.zeros(env.agents, len(HEADS), dtype=torch.long, device=dev)
t = dict.fromkeys(["observe (Rust) + copy to GPU", "policy forward x2", "pack actions -> bytes", "step (Rust)", "rewards/dones -> GPU"], 0.0)
def tick(name, start):
    torch.cuda.synchronize(); t[name] += time.perf_counter() - start
steps = 30
with torch.no_grad():
    for i in range(steps + 3):
        if i == 3: t = dict.fromkeys(t, 0.0)   # warm-up
        s = time.perf_counter(); full = observe(env, dev); tick("observe (Rust) + copy to GPU", s)
        s = time.perf_counter(); everyone[ours] = policy.act(full[ours])[0]; everyone[theirs] = rival.act(full[theirs])[0]; tick("policy forward x2", s)
        s = time.perf_counter(); act = pack(everyone); tick("pack actions -> bytes", s)
        s = time.perf_counter(); rew, done = env.step(act); tick("step (Rust)", s)
        s = time.perf_counter(); r = torch.tensor(np.frombuffer(rew, dtype=np.float32), device=dev)[ours]; d = torch.tensor(np.frombuffer(done, dtype=np.uint8), device=dev)[ours]; tick("rewards/dones -> GPU", s)
total = sum(t.values())
for k, v in t.items():
    print(f"{k:28s} {1000*v/steps:7.1f} ms  {100*v/total:5.1f}%")
print(f"one decision step for {envs} matches: {1000*total/steps:.1f} ms -> {envs*3*steps/total:,.0f} match-ticks/s (rollout only, no PPO update)")
