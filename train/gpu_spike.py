"""Upper bound for a GPU simulation: the costly core of one decision step (3 ticks of movement
and collision tests, then observations) as batched tensor ops. Not the game: no purchases,
drones, bases or respawns, and actions are random."""
import sys, time, math, torch
dev = torch.device("cuda")
S, R, B, C, RAD, DT = 21, 270, 128, 64, 1200.0, 1 / 30
OBS = 271

def unit(v): return v / v.norm(dim=-1, keepdim=True).clamp_min(1e-9)
def tangent(p, v): return v - p * (v * p).sum(-1, keepdim=True)

def make(n):
    g = torch.Generator(device=dev).manual_seed(1)
    rnd = lambda *s: torch.randn(*s, device=dev, generator=g)
    w = dict(pos=unit(rnd(n, S, 3)), rock=unit(rnd(n, R, 3)), shot=unit(rnd(n, B, 3)), cry=unit(rnd(n, C, 3)))
    w["fwd"] = unit(tangent(w["pos"], rnd(n, S, 3))); w["vel"] = w["fwd"] * 100
    w["rvel"] = unit(tangent(w["rock"], rnd(n, R, 3))) * 8; w["svel"] = unit(tangent(w["shot"], rnd(n, B, 3))) * 325
    w["rsize"] = torch.randint(1, 6, (n, R), device=dev, generator=g).float(); w["rhp"] = 10 * w["rsize"] ** 2
    w["salive"] = torch.rand(n, B, device=dev, generator=g) < 0.4
    w["hull"] = torch.full((n, S), 100.0, device=dev); w["cargo"] = torch.zeros(n, S, device=dev)
    w["cval"] = torch.randint(8, 150, (n, C), device=dev, generator=g).float()
    w["team"] = (torch.arange(S, device=dev) // 7).expand(n, S)
    return w

def advance(p, v):
    p2 = unit(p + v * (DT / RAD)); speed = v.norm(dim=-1, keepdim=True)
    return p2, unit(tangent(p2, v)) * speed

def chord(a, b):  # [n, A, 3] x [n, K, 3] -> [n, A, K] straight-line distance in world units
    return ((2 - 2 * a @ b.transpose(1, 2)).clamp_min(0)).sqrt() * RAD

def tick(w, turn, thrust):
    ang = turn * 3.2 * DT
    w["fwd"] = w["fwd"] * torch.cos(ang) + torch.cross(w["pos"], w["fwd"], dim=-1) * torch.sin(ang)
    w["vel"] = (w["vel"] + w["fwd"] * (130 * DT) * thrust) * (1 - 0.6 * DT)
    sp = w["vel"].norm(dim=-1, keepdim=True); w["vel"] = w["vel"] * (150 / sp.clamp_min(150))
    w["pos"], w["vel"] = advance(w["pos"], w["vel"]); w["fwd"] = unit(tangent(w["pos"], w["fwd"]))
    w["rock"], w["rvel"] = advance(w["rock"], w["rvel"]); w["shot"], w["svel"] = advance(w["shot"], w["svel"])
    # shots against rocks: the first rock each live shot touches takes damage
    d = chord(w["shot"], w["rock"]); hit = (d < 18 * w["rsize"][:, None, :]) & w["salive"][:, :, None]
    first = hit.float().argmax(-1); any_hit = hit.any(-1)
    w["rhp"] = w["rhp"].scatter_add(1, first, -any_hit.float()); w["salive"] = w["salive"] & ~any_hit
    # shots against enemy ships
    d = chord(w["shot"], w["pos"]); hit = (d < 12) & w["salive"][:, :, None]
    victim = hit.float().argmax(-1); any_hit = hit.any(-1)
    w["hull"] = w["hull"].scatter_add(1, victim, -any_hit.float()); w["salive"] = w["salive"] & ~any_hit
    # ships against rocks: push out and bounce off the nearest overlapping rock
    d = chord(w["pos"], w["rock"]); over = d < (18 * w["rsize"][:, None, :] + 12)
    near = torch.where(over, d, torch.full_like(d, 1e9)).argmin(-1); touching = over.any(-1, keepdim=True)
    rp = torch.gather(w["rock"], 1, near[..., None].expand(-1, -1, 3)); n_out = unit(tangent(w["pos"], w["pos"] - rp))
    closing = (w["vel"] * n_out).sum(-1, keepdim=True).clamp_max(0)
    w["vel"] = torch.where(touching, w["vel"] - n_out * 1.5 * closing, w["vel"]); w["hull"] = w["hull"] + (closing.squeeze(-1) * 0.15) * touching.squeeze(-1)
    # ships against ships
    d = chord(w["pos"], w["pos"]); over = (d < 24) & (d > 1e-3)
    w["hull"] = w["hull"] - over.float().sum(-1) * 0.01
    # crystal pick-up: the nearest ship in reach takes the pack
    d = chord(w["cry"], w["pos"]); reach = d < 34
    taker = torch.where(reach, d, torch.full_like(d, 1e9)).argmin(-1); got = reach.any(-1)
    w["cargo"] = w["cargo"].scatter_add(1, taker, w["cval"] * got); w["cval"] = w["cval"] * ~got

def frame(w, target):  # target [n, S, K, 3] -> position in each ship's own frame [n, S, K, 2]
    p, f = w["pos"][:, :, None, :], w["fwd"][:, :, None, :]
    cosang = (p * target).sum(-1, keepdim=True).clamp(-1, 1)
    way = unit(target - p * cosang) * (torch.acos(cosang) * RAD)
    left = torch.cross(p.expand_as(f), f, dim=-1)
    return torch.stack(((way * f).sum(-1), (way * left).sum(-1)), -1)

def nearest(w, others, k, extra):  # the k nearest of 'others' to each ship, within view, as feature rows
    d = chord(w["pos"], others); d = torch.where(d < 600, d, torch.full_like(d, 1e9))
    dist, idx = d.topk(k, dim=-1, largest=False)
    pos = torch.gather(others[:, None].expand(-1, S, -1, -1), 2, idx[..., None].expand(-1, -1, -1, 3))
    feats = [frame(w, pos) / 600, (dist < 1e8).float()[..., None]] + [torch.gather(e[:, None].expand(-1, S, -1), 2, idx)[..., None] for e in extra]
    return torch.cat(feats, -1).flatten(2)

def observe(w):
    parts = [nearest(w, w["rock"], 6, [w["rsize"], w["rhp"]]), nearest(w, w["cry"], 6, [w["cval"]]),
             nearest(w, w["pos"], 7, [w["hull"], w["team"].float()]), nearest(w, w["shot"], 4, [w["salive"].float()]),
             w["hull"][..., None], w["cargo"][..., None], frame(w, w["pos"].mean(1, keepdim=True)[:, None].expand(-1, S, 1, -1)).flatten(2)]
    o = torch.cat(parts, -1)
    return torch.nn.functional.pad(o, (0, OBS - o.shape[-1]))

@torch.no_grad()
def bench(n, steps=20):
    w = make(n)
    def decision():
        turn = torch.randint(-1, 2, (n, S, 1), device=dev).float(); thrust = torch.randint(0, 2, (n, S, 1), device=dev).float()
        for _ in range(3): tick(w, turn, thrust)
        return observe(w)
    for _ in range(3): decision()
    torch.cuda.synchronize(); t = time.perf_counter()
    for _ in range(steps): o = decision()
    torch.cuda.synchronize(); dt = (time.perf_counter() - t) / steps
    print(f"{n:6d} matches: {1000*dt:7.1f} ms per decision step, {n*3/dt:>12,.0f} match-ticks/s, {torch.cuda.max_memory_allocated()/2**30:.1f} GiB", flush=True)

for n in [int(a) for a in sys.argv[1:]] or [2048, 8192, 16384]:
    torch.cuda.reset_peak_memory_stats(); bench(n)
