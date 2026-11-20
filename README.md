# Star Swarm

A team space game where a human commands a fleet of AI-flown ships. Ships break rocks, collect crystals, buy upgrades and fight other teams; the commander sets orders, the ships fly themselves.

## Rules

- The match is played around a planet: ships, rocks and bases sit on a sphere of radius 1200, and distances are measured along its surface. Three teams of nine ships by default. Bases are spaced evenly around the equator, a third of the way around from each other with three teams. The game window shows the planet as a globe; turn it with the arrow buttons or by dragging with the right mouse button.
- Every team has a base (1000 HP) with a turret that shoots the nearest enemy ship in range. A team wins when every other base is destroyed; a team without a base gets no new ships. A match lasts at most 30 minutes.
- Crystals buy nothing until a ship docks in its own base (comes to rest inside it) and hands them over, a tenth of a full hold a second for as long as it chooses to. A base has two places; a ship that arrives when both are taken waits, and every ship sees how full its base is. Each crystal handed over is a point for the ship to spend, and counts towards the base's next level: 1000 crystals in total for level 2, 2500 for level 3, up to level 7. A level adds 200 hit points and 15% turret damage; tier 3 needs a level-2 base.
- The shop is for docked ships. Points buy upgrades, the next tier (150 points, then 300, then 600), a base repair (10 points for 10 hit points) or a drone, one per tier of the ship; a drone can be sold back for half. An eco drone (40) breaks rocks near its owner and gathers crystals for it, and its shots do nothing to ships. A shield drone (40) stands between its owner and the nearest enemy and takes 30 points of shots meant for it. A combat drone (60) shoots at the nearest enemy. When the owner is destroyed its drones drift until any ship with room flies up and takes them.
- Flying into a rock damages the rock as well as the ship, a fifth of the impact speed: with 100 shield that grows back only slowly without upgrades, ramming small rocks is a fast way to mine early on.
- A ship sees the hull, shields and cargo of every ship in sight and of its teammates on the radar.
- A tier-1 ship does 2 damage a shot, has 100 hull and 100 shield, and carries 150 crystals. The smallest rock has 20 hit points, the biggest 500.
- A ship sees how hard each enemy in sight hits, as far as its team has felt it: the full-power damage of the last shot of that enemy to hit one of them. Nothing is known of an enemy that has not hit yet, and an upgrade it buys shows only once it has been felt.
- The planet has a radius of 3200: the three bases sit evenly around its equator, 6700 from each other, three quarters of a minute of flying. Rocks grow in farms, clusters of one kind with open space between them, all over the planet: about 130 farms a thousand apart, some 790 rocks in all. Where a farm lies decides what grows in it. Within 1500 of a base the rocks are small: safe, and poor. Farther out they are medium. Where the next base is about as far away as a team's own, the rocks are big, with a lode of big rocks exactly half-way between every two bases and one at each pole, as far from all three: the rich fields are the ones two teams come to from two sides. Single stragglers of any size lie anywhere. Each team's third of the planet has the same farms at the same distances from its base, the nearest one 600 out. A broken rock grows back in its farm ten minutes later. Every ship knows the eight nearest farms that still have rocks.
- A ship has inertia. The engine pushes along the nose and speed builds up over about two seconds; with the engine off the ship drifts the way it was going, hardly slowing, wherever its nose points, so it can fly one way and shoot another. Thrusting a new way brings the course round little by little. Four controls: left, right, forward (thrust) and back (brake, which stops a ship in about a second). Inside its own base a ship with the engine off is caught and brought to rest.
- Ammo decides how much a ship can shoot. A tier-1 ship without upgrades fires about twice a second and each shot costs a round; a magazine holds 30 and gets back 1.2 a second, so firing all the time empties it in half a minute and then only every other shot is there. Shooting at nothing is ammo not there for a target. Fire-rate upgrades add 12% a level.
- A match lasts 30 minutes at 20 simulation steps a second. Rocks leave packs of 12 to 22 crystals for the smallest and 167 to 225 for the biggest. A base needs 2000 crystals for level 2, then 5000, 10000, 16000 and 24000 for level 6.
- A destroyed ship comes back whole with an empty hold and no drones, and pays for it in points: 20, plus a fifth of what it had. A tier-1 ship loses nothing else. A ship above tier 1 has three lives, a fresh set with every tier; when the last is gone it is again the ship it was just before it moved up, with the upgrades it had then and three lives. Rushing up the tiers is the fast way to grow, and the risky one: a lone ship with a full hold on its way to the next tier is worth hunting.
- A ship on a run of kills without dying is paid 30 points for the third kill of the run and for each one after. Everyone in sight sees the run. Destroying such a ship pays every ship of the killer's team 10 points for each kill of the run it ended.
- A base under fire sheds crystals: a pack of 10 lands around it for every 10 damage it takes, for whoever gets there first.
- A pack of crystals fades from its full worth to nothing in 10 seconds, and ships see how much time each has left. Flying into a rock costs shields, then hull, but knocks no crystals out of the hold; enemy fire and enemy ramming do.
- The hull mends in two ways only: docked in the base, a tenth of it a second, or by picking up crystals, which repair a point each before any go into the hold. Rocks come in five sizes, S to XXL, with 10, 40, 90, 160 and 250 hit points. A broken rock leaves one pack of crystals: 8-15 for S, 16-40 for M, 41-70 for L, 71-110 for XL, 111-150 for XXL. A ship takes as much of a pack as it has room for. Small rocks pay best per shot, so they suit small ships.
- There are eight upgrades: attack, health, fire rate, shields, speed, shield regeneration, agility, ammo. Shields take damage first and grow back on their own; the hull is repaired only next to the team's own base.
- Ships have seven tiers with upgrade caps of 3 to 9 levels. Moving up costs 150 points, then 300, 600, 1000, 1500 and 2200. Any base will do up to tier 3; from tier 4 the base has to keep up (tier 4 needs a level-3 base, tier 7 a level-6 one, the highest a base goes). A full set of one upgrade adds the same share at every tier, 60% for attack, hull, shields, shield regeneration and ammo, so a ship is about one and a half times as strong with each tier and a top-tier ship more than twelve times a new one. An upgrade level costs 10 points times the level times the tier. Moving up, the ship starts again without upgrades: a fresh ship of the next tier is a little worse than a fully upgraded one of the tier below, and better once upgraded. A team has nine ships.
- Leaving tier 1 a ship picks what it becomes. The cannon fires one big, fast shot that reaches 1100, seldom, from a thin hull that turns slowly; its shots grow with its attack, and at the top tier one is a ball bigger than a small ship. The hunter is fast and nimble with a thin hull and fires bursts of three quick volleys, then waits. The tank is slow, has more than twice the hull and wins every collision. The cruiser has two lasers and a little more hull, and is good at everything. Every shot is as big as it hits hard.
- Things are big enough to hit: a tier-1 ship has a radius of 20 and a top-tier one 60, a base 180. A basic laser reaches 600 and its shot has a radius of 5; the cannon reaches 1100 with a shot of 11 that grows with its attack.
- Ships take up room: they bounce off each other, and enemies that collide take damage by how fast they met. A Ram deals five times the ramming damage and takes a quarter; a Lancer deals half and takes one and a half times.
- Every ship has a mode. Farming is the default and defending the base is a ship's own choice. Attacking is a team decision: when at least half of a team's living ships pick an attack, the whole team attacks the base most of them picked. A human commander's orders replace the modes of their team.
- Team radar: a ship knows where every teammate is, how damaged it is and what mode it is in, and it is told about enemies a teammate can see.
- A ship sees clearly out to 1040, with detail enough to aim: the twelve nearest rocks and the six nearest crystal packs, ships and enemy shots.
- Long-range radar: beyond its clear sight and up to 1500 away a ship still knows where enemy ships are, though not enough to aim, and how much rock lies in each of eight directions around it.
- Fog of war: beyond its own view range a ship sees nothing else, and a team knows an enemy base's health only from the last time one of its ships was near it. When you play, the window shows only what your team sees.
- A ship's hold is safe while its shields hold. The first enemy hit that gets past them bursts the hold open: everything it carries falls out at once, in packs around it, for anyone to pick up before they fade.
- Shots do full damage over the first 60% of their range and then fade evenly to nothing at the end of it. Every laser fired costs one round of ammo, which comes back over time.
- A boost tank gives extra thrust and top speed for two seconds and refills slowly; boost cells dropped by rocks refill it at once.

## Layout

- `crates/swarm-sim`: the deterministic simulation core. No GUI, no I/O. Same seed and same actions give the same match on the same binary and machine. Also holds the scripted bot, the scripted commander and the replay format.
- `crates/swarm-gui`: the commander GUI on [gpui-kit](https://gpui-kit.com/docs). You lead the blue team against two scripted commanders, or watch a saved replay.

- `crates/swarm-env`: the batched headless environment. Many matches stepped side by side on all cores; every ship is one agent; reward weights are data.
- `crates/swarm-py`: Python bindings for `swarm-env`, built with maturin.
- `train/ppo.py`: PPO in PyTorch. One shared policy flies every ship.

Planned (see the issues): combat training with self-play, ONNX inference in Rust so the fleet flies on a trained model, ships that follow orders, and a socket for external bots.

## Run

```sh
cargo run --release -p swarm-gui                      # play as commander
cargo run --release -p swarm-gui -- --seed 7          # another map
cargo run --release -p swarm-gui -- --replay FILE     # watch a saved match

cargo test                                                        # all tests
cargo run --release -p swarm-sim --example bench -- 1             # one bot match: throughput and stats
cargo run --release -p swarm-sim --example record -- 1 out.replay # record a bot match
```

In the GUI: pick an order (Kop, Atakuj, Broń, Zbiórka), then click a point on the map. With no ships selected the order goes to the whole fleet; click ships on the map or in the list to command only those. "Zapisz powtórkę" writes `star-swarm-<seed>.replay` to the current directory.

On Linux the GUI needs a Wayland or X11 session and a working Vulkan driver.

## Train

```sh
uv venv --python 3.12 .venv
uv pip install --python .venv/bin/python -r train/requirements.txt
(cd crates/swarm-py && VIRTUAL_ENV=$PWD/../../.venv ../../.venv/bin/maturin develop --release)

train/curriculum.sh win      # self-play on bases that harden stage by stage; writes runs/win-s1 ... runs/win-s4
cargo run --release -p swarm-env --example throughput     # environment speed alone
```

To see a trained policy fly your fleet against scripted teams, or to measure it over full matches:

```sh
.venv/bin/python train/export.py runs/win-s4/policy.pt runs/win-s4/policy.swp       # weights file for the game
cargo run --release -p swarm-gui -- --model runs/win-s4/policy.swp                 # watch it play
cargo run --release -p swarm-sim --example versus -- runs/win-s4/policy.swp 20     # 20 matches, numbers per team
.venv/bin/python train/watch.py runs/win-s4/policy.pt model.replay                 # one training-style match as a replay
```

To watch training as it happens, run this next to it. The trainer streams one of its matches to `runs/<name>/live.swl` and the window replays it tick for tick, with a table of the run's latest measurements. (Without a stream, `train/live.sh runs &` keeps a game-readable copy of the newest policy and the window plays matches of its own on it.)

```sh
cargo run --release -p swarm-gui -- --watch runs      # shows one of the trainer's own matches as it is played
cargo run --release -p swarm-gui -- --wins runs       # replays the matches the team being trained won, razed a base in or came first in
```

The run first measures the scripted bot on a fixed set of matches, then trains, then plays the trained policy on the same matches. Each line of `runs/<name>/metrics.jsonl` is one measurement for team 0: how often it won, how often it lost its base, crystals per ship per minute, upgrade levels, kills and deaths. It also records what the ships turned into: the share of each class and the mean level of each upgrade. In training the policy plays earlier copies of itself and is paid only for winning, losing, damage to bases and damage taken; nothing is paid for mining or upgrading.
