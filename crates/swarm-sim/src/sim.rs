use crate::rng::Rng;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const fn new(x: f32, y: f32) -> Self {
        Vec2 { x, y }
    }
    pub fn from_angle(a: f32) -> Self {
        Vec2::new(a.cos(), a.sin())
    }
    pub fn len(self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }
    pub fn add(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x + o.x, self.y + o.y)
    }
    pub fn sub(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x - o.x, self.y - o.y)
    }
    pub fn scale(self, k: f32) -> Vec2 {
        Vec2::new(self.x * k, self.y * k)
    }
    pub fn dot(self, o: Vec2) -> f32 {
        self.x * o.x + self.y * o.y
    }
    /// Rotates a world-space vector into the frame of a ship facing `heading`
    /// (x forward, y to the left).
    pub fn into_frame(self, heading: f32) -> Vec2 {
        let (s, c) = heading.sin_cos();
        Vec2::new(self.x * c + self.y * s, -self.x * s + self.y * c)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Upgrade {
    HullCap,
    HullRegen,
    EnergyCap,
    EnergyRegen,
    Damage,
    Speed,
    Agility,
    Cargo,
}

pub const UPGRADES: [Upgrade; 8] = [
    Upgrade::HullCap,
    Upgrade::HullRegen,
    Upgrade::EnergyCap,
    Upgrade::EnergyRegen,
    Upgrade::Damage,
    Upgrade::Speed,
    Upgrade::Agility,
    Upgrade::Cargo,
];

/// What a commander tells a ship to do. The ship still flies itself; the order
/// is part of its observation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum OrderKind {
    /// Break rocks and collect crystals wherever they are.
    #[default]
    Mine,
    /// Fly to the target and fight enemies there.
    Attack,
    /// Stay near the target, fight enemies that come close, mine otherwise.
    Defend,
    /// Fly to the target and hold.
    Regroup,
}

pub const ORDER_KINDS: usize = 4;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Order {
    pub kind: OrderKind,
    pub target: Vec2,
}

/// One ship's input for one tick. The same for humans, bots and models.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Action {
    pub thrust: bool,
    /// -1 turns right, 1 turns left, 0 holds.
    pub turn: i8,
    pub fire: bool,
    pub buy: Option<Upgrade>,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub world_size: f32,
    pub dt: f32,
    pub teams: usize,
    pub ships_per_team: usize,
    pub rock_count: usize,
    pub max_ticks: u32,

    pub ship_radius: f32,
    pub thrust_accel: f32,
    pub drag: f32,
    pub max_speed: f32,
    pub turn_rate: f32,
    pub hull: f32,
    pub hull_regen: f32,
    pub energy: f32,
    pub energy_regen: f32,
    pub cargo: u32,
    pub cargo_per_level: u32,

    pub shot_cost: f32,
    pub shot_damage: f32,
    pub shot_speed: f32,
    pub shot_ticks: u16,
    pub shot_cooldown: u16,

    pub rock_radius: f32,
    pub rock_hp: f32,
    pub rock_speed: f32,
    pub crash_damage: f32,

    pub crystal_ticks: u16,
    pub pickup_radius: f32,

    pub upgrade_max: u8,
    pub upgrade_cost: u32,
    pub upgrade_step: f32,

    pub respawn_ticks: u16,
    pub kill_score: u32,
    pub view_range: f32,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            world_size: 2400.0,
            dt: 1.0 / 30.0,
            teams: 3,
            ships_per_team: 5,
            rock_count: 90,
            max_ticks: 9000,
            ship_radius: 12.0,
            thrust_accel: 220.0,
            drag: 0.6,
            max_speed: 240.0,
            turn_rate: 3.2,
            hull: 100.0,
            hull_regen: 2.0,
            energy: 60.0,
            energy_regen: 12.0,
            cargo: 40,
            cargo_per_level: 20,
            shot_cost: 8.0,
            shot_damage: 10.0,
            shot_speed: 520.0,
            shot_ticks: 27,
            shot_cooldown: 5,
            rock_radius: 18.0,
            rock_hp: 30.0,
            rock_speed: 12.0,
            crash_damage: 0.15,
            crystal_ticks: 600,
            pickup_radius: 26.0,
            upgrade_max: 3,
            upgrade_cost: 10,
            upgrade_step: 0.25,
            respawn_ticks: 90,
            kill_score: 20,
            view_range: 600.0,
        }
    }
}

impl Config {
    fn boost(&self, ship: &Ship, u: Upgrade) -> f32 {
        1.0 + self.upgrade_step * ship.levels[u as usize] as f32
    }
    pub fn hull_max(&self, s: &Ship) -> f32 {
        self.hull * self.boost(s, Upgrade::HullCap)
    }
    pub fn energy_max(&self, s: &Ship) -> f32 {
        self.energy * self.boost(s, Upgrade::EnergyCap)
    }
    pub fn cargo_max(&self, s: &Ship) -> u32 {
        self.cargo + self.cargo_per_level * s.levels[Upgrade::Cargo as usize] as u32
    }
    /// Price of the next level of an upgrade, or None at the cap.
    pub fn cost(&self, level: u8) -> Option<u32> {
        (level < self.upgrade_max).then(|| self.upgrade_cost * (level as u32 + 1))
    }
    pub fn shot_range(&self) -> f32 {
        self.shot_speed * self.shot_ticks as f32 * self.dt
    }
    pub fn rock_radius_of(&self, size: u8) -> f32 {
        self.rock_radius * size as f32
    }
    pub fn rock_hp_of(&self, size: u8) -> f32 {
        self.rock_hp * (size as f32) * (size as f32)
    }
}

#[derive(Clone, Debug)]
pub struct Ship {
    pub team: u8,
    pub pos: Vec2,
    pub vel: Vec2,
    pub heading: f32,
    pub hull: f32,
    pub energy: f32,
    pub cargo: u32,
    pub levels: [u8; 8],
    pub cooldown: u16,
    pub alive: bool,
    pub respawn: u16,
    pub order: Order,
    pub last_hit_by: Option<u16>,
    pub score: u32,
    pub collected: u32,
    pub kills: u32,
    pub deaths: u32,
}

#[derive(Clone, Debug)]
pub struct Rock {
    pub pos: Vec2,
    pub vel: Vec2,
    pub size: u8,
    pub hp: f32,
    pub last_hit_by: Option<u16>,
}

#[derive(Clone, Debug)]
pub struct Crystal {
    pub pos: Vec2,
    pub ticks: u16,
}

#[derive(Clone, Debug)]
pub struct Shot {
    pub pos: Vec2,
    pub vel: Vec2,
    pub ticks: u16,
    pub damage: f32,
    pub owner: u16,
    pub team: u8,
}

/// What happened during one step. Rewards and the GUI's effects read these.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    RockDestroyed { by: Option<u16>, size: u8 },
    Collected { ship: u16 },
    Bought { ship: u16, upgrade: Upgrade },
    Hit { by: u16, victim: u16, damage: f32 },
    Crashed { ship: u16, damage: f32 },
    Killed { by: Option<u16>, victim: u16 },
}

#[derive(Clone, Debug)]
pub struct World {
    pub cfg: Config,
    pub tick: u32,
    pub ships: Vec<Ship>,
    pub rocks: Vec<Rock>,
    pub crystals: Vec<Crystal>,
    pub shots: Vec<Shot>,
    /// One spawn point per team.
    pub bases: Vec<Vec2>,
    pub events: Vec<Event>,
    rng: Rng,
}

const CRYSTALS_PER_SIZE: [u32; 4] = [0, 2, 5, 10];
const MAX_DROPPED: u32 = 8;

impl World {
    pub fn new(cfg: Config, seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let center = cfg.world_size / 2.0;
        let bases: Vec<Vec2> = (0..cfg.teams)
            .map(|t| {
                let a = std::f32::consts::TAU * t as f32 / cfg.teams as f32;
                Vec2::new(center, center).add(Vec2::from_angle(a).scale(cfg.world_size * 0.35))
            })
            .collect();
        let mut world = World {
            tick: 0,
            ships: Vec::new(),
            rocks: Vec::new(),
            crystals: Vec::new(),
            shots: Vec::new(),
            bases,
            events: Vec::new(),
            rng: Rng::new(0),
            cfg,
        };
        for team in 0..world.cfg.teams {
            for _ in 0..world.cfg.ships_per_team {
                let mut ship = Ship {
                    team: team as u8,
                    pos: Vec2::default(),
                    vel: Vec2::default(),
                    heading: 0.0,
                    hull: 0.0,
                    energy: 0.0,
                    cargo: 0,
                    levels: [0; 8],
                    cooldown: 0,
                    alive: true,
                    respawn: 0,
                    order: Order::default(),
                    last_hit_by: None,
                    score: 0,
                    collected: 0,
                    kills: 0,
                    deaths: 0,
                };
                Self::place(&world.cfg, &world.bases, &mut rng, &mut ship);
                world.ships.push(ship);
            }
        }
        for _ in 0..world.cfg.rock_count {
            let rock = Self::new_rock(&world.cfg, &world.bases, &mut rng);
            world.rocks.push(rock);
        }
        world.rng = rng;
        world
    }

    pub fn done(&self) -> bool {
        self.tick >= self.cfg.max_ticks
    }

    pub fn set_order(&mut self, ship: usize, order: Order) {
        self.ships[ship].order = order;
    }

    pub fn wrap(&self, p: Vec2) -> Vec2 {
        wrap(self.cfg.world_size, p)
    }

    /// Shortest vector from `a` to `b` on the wrapping map.
    pub fn delta(&self, a: Vec2, b: Vec2) -> Vec2 {
        delta(self.cfg.world_size, a, b)
    }

    fn place(cfg: &Config, bases: &[Vec2], rng: &mut Rng, ship: &mut Ship) {
        let base = bases[ship.team as usize];
        let offset = Vec2::from_angle(rng.range(0.0, std::f32::consts::TAU)).scale(rng.range(20.0, 90.0));
        ship.pos = wrap(cfg.world_size, base.add(offset));
        ship.vel = Vec2::default();
        ship.heading = rng.range(0.0, std::f32::consts::TAU);
        ship.hull = cfg.hull_max(ship);
        ship.energy = cfg.energy_max(ship);
        ship.cooldown = 0;
        ship.alive = true;
        ship.last_hit_by = None;
    }

    fn new_rock(cfg: &Config, bases: &[Vec2], rng: &mut Rng) -> Rock {
        let mut pos = Vec2::default();
        for _ in 0..10 {
            pos = Vec2::new(rng.range(0.0, cfg.world_size), rng.range(0.0, cfg.world_size));
            if bases.iter().all(|b| delta(cfg.world_size, *b, pos).len() > 200.0) {
                break;
            }
        }
        let size = 1 + rng.below(3) as u8;
        let vel = Vec2::from_angle(rng.range(0.0, std::f32::consts::TAU)).scale(rng.range(0.0, cfg.rock_speed));
        Rock { pos, vel, size, hp: cfg.rock_hp_of(size), last_hit_by: None }
    }

    fn scatter(&mut self, at: Vec2, count: u32, spread: f32) {
        for _ in 0..count {
            let offset = Vec2::from_angle(self.rng.range(0.0, std::f32::consts::TAU)).scale(self.rng.range(0.0, spread));
            self.crystals.push(Crystal { pos: wrap(self.cfg.world_size, at.add(offset)), ticks: self.cfg.crystal_ticks });
        }
    }

    /// Advances the match by one tick. `actions[i]` drives `ships[i]`.
    pub fn step(&mut self, actions: &[Action]) -> &[Event] {
        assert_eq!(actions.len(), self.ships.len(), "one action per ship");
        self.events.clear();
        self.tick += 1;
        let dt = self.cfg.dt;
        let size = self.cfg.world_size;

        // Ships: buy, steer, move, shoot, respawn.
        for i in 0..self.ships.len() {
            let cfg = &self.cfg;
            let ship = &mut self.ships[i];
            if !ship.alive {
                ship.respawn = ship.respawn.saturating_sub(1);
                if ship.respawn == 0 {
                    Self::place(cfg, &self.bases, &mut self.rng, ship);
                }
                continue;
            }
            let act = actions[i];
            if let Some(u) = act.buy {
                if let Some(cost) = cfg.cost(ship.levels[u as usize]) {
                    if ship.cargo >= cost {
                        ship.cargo -= cost;
                        ship.levels[u as usize] += 1;
                        self.events.push(Event::Bought { ship: i as u16, upgrade: u });
                    }
                }
            }
            ship.heading += act.turn.clamp(-1, 1) as f32 * cfg.turn_rate * cfg.boost(ship, Upgrade::Agility) * dt;
            ship.heading = ship.heading.rem_euclid(std::f32::consts::TAU);
            let speed_boost = cfg.boost(ship, Upgrade::Speed);
            if act.thrust {
                ship.vel = ship.vel.add(Vec2::from_angle(ship.heading).scale(cfg.thrust_accel * speed_boost * dt));
            }
            ship.vel = ship.vel.scale(1.0 - cfg.drag * dt);
            let (speed, cap) = (ship.vel.len(), cfg.max_speed * speed_boost);
            if speed > cap {
                ship.vel = ship.vel.scale(cap / speed);
            }
            ship.pos = wrap(size, ship.pos.add(ship.vel.scale(dt)));
            ship.hull = (ship.hull + cfg.hull_regen * cfg.boost(ship, Upgrade::HullRegen) * dt).min(cfg.hull_max(ship));
            ship.energy = (ship.energy + cfg.energy_regen * cfg.boost(ship, Upgrade::EnergyRegen) * dt).min(cfg.energy_max(ship));
            ship.cooldown = ship.cooldown.saturating_sub(1);
            if act.fire && ship.cooldown == 0 && ship.energy >= cfg.shot_cost {
                ship.energy -= cfg.shot_cost;
                ship.cooldown = cfg.shot_cooldown;
                let dir = Vec2::from_angle(ship.heading);
                self.shots.push(Shot {
                    pos: wrap(size, ship.pos.add(dir.scale(cfg.ship_radius))),
                    vel: ship.vel.add(dir.scale(cfg.shot_speed)),
                    ticks: cfg.shot_ticks,
                    damage: cfg.shot_damage * cfg.boost(ship, Upgrade::Damage),
                    owner: i as u16,
                    team: ship.team,
                });
            }
        }

        // Shots: move, then hit the first rock or enemy ship they touch.
        for shot in &mut self.shots {
            shot.pos = wrap(size, shot.pos.add(shot.vel.scale(dt)));
            shot.ticks -= 1;
            if let Some(rock) = self
                .rocks
                .iter_mut()
                .find(|r| r.hp > 0.0 && delta(size, r.pos, shot.pos).len() < self.cfg.rock_radius * r.size as f32)
            {
                rock.hp -= shot.damage;
                rock.last_hit_by = Some(shot.owner);
                shot.ticks = 0;
                continue;
            }
            let hit = self.ships.iter().position(|s| {
                s.alive && s.team != shot.team && delta(size, s.pos, shot.pos).len() < self.cfg.ship_radius
            });
            if let Some(v) = hit {
                self.ships[v].hull -= shot.damage;
                self.ships[v].last_hit_by = Some(shot.owner);
                self.events.push(Event::Hit { by: shot.owner, victim: v as u16, damage: shot.damage });
                shot.ticks = 0;
            }
        }
        self.shots.retain(|s| s.ticks > 0);

        // Rocks drift; ships bounce off them and take damage by impact speed.
        for r in 0..self.rocks.len() {
            let rock = &mut self.rocks[r];
            rock.pos = wrap(size, rock.pos.add(rock.vel.scale(dt)));
            let reach = self.cfg.rock_radius * rock.size as f32 + self.cfg.ship_radius;
            for (i, ship) in self.ships.iter_mut().enumerate() {
                if !ship.alive {
                    continue;
                }
                let d = delta(size, rock.pos, ship.pos);
                let dist = d.len();
                if dist >= reach || dist < 1e-3 {
                    continue;
                }
                let n = d.scale(1.0 / dist);
                ship.pos = wrap(size, rock.pos.add(n.scale(reach)));
                let closing = ship.vel.sub(rock.vel).dot(n);
                if closing < 0.0 {
                    ship.vel = ship.vel.sub(n.scale(1.5 * closing));
                    let damage = -closing * self.cfg.crash_damage;
                    ship.hull -= damage;
                    ship.last_hit_by = None;
                    self.events.push(Event::Crashed { ship: i as u16, damage });
                }
            }
        }

        // Broken rocks leave crystals and are replaced elsewhere.
        for r in 0..self.rocks.len() {
            if self.rocks[r].hp > 0.0 {
                continue;
            }
            let (pos, rock_size, by) = (self.rocks[r].pos, self.rocks[r].size, self.rocks[r].last_hit_by);
            self.events.push(Event::RockDestroyed { by, size: rock_size });
            self.scatter(pos, CRYSTALS_PER_SIZE[rock_size as usize], self.cfg.rock_radius * rock_size as f32);
            self.rocks[r] = Self::new_rock(&self.cfg, &self.bases, &mut self.rng);
        }

        // Crystals go to the nearest ship in reach that still has room.
        let cfg = &self.cfg;
        let ships = &mut self.ships;
        let events = &mut self.events;
        self.crystals.retain_mut(|c| {
            c.ticks -= 1;
            let mut best: Option<(usize, f32)> = None;
            for (i, s) in ships.iter().enumerate() {
                if !s.alive || s.cargo >= cfg.cargo_max(s) {
                    continue;
                }
                let dist = delta(size, s.pos, c.pos).len();
                if dist < cfg.pickup_radius && best.is_none_or(|(_, b)| dist < b) {
                    best = Some((i, dist));
                }
            }
            if let Some((i, _)) = best {
                ships[i].cargo += 1;
                ships[i].collected += 1;
                ships[i].score += 1;
                events.push(Event::Collected { ship: i as u16 });
                return false;
            }
            c.ticks > 0
        });

        // Deaths: drop part of the cargo, credit the killer, start the respawn timer.
        for i in 0..self.ships.len() {
            if !self.ships[i].alive || self.ships[i].hull > 0.0 {
                continue;
            }
            let (pos, dropped, by) = (self.ships[i].pos, self.ships[i].cargo.min(MAX_DROPPED), self.ships[i].last_hit_by);
            let ship = &mut self.ships[i];
            ship.alive = false;
            ship.respawn = self.cfg.respawn_ticks;
            ship.cargo = 0;
            ship.deaths += 1;
            if let Some(k) = by {
                self.ships[k as usize].kills += 1;
                self.ships[k as usize].score += self.cfg.kill_score;
            }
            self.events.push(Event::Killed { by, victim: i as u16 });
            self.scatter(pos, dropped, 30.0);
        }

        &self.events
    }

    /// FNV-1a over the full state. Two worlds with the same hash went through
    /// the same match.
    pub fn state_hash(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut put = |v: u32| {
            for b in v.to_le_bytes() {
                h = (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3);
            }
        };
        put(self.tick);
        for s in &self.ships {
            for f in [s.pos.x, s.pos.y, s.vel.x, s.vel.y, s.heading, s.hull, s.energy] {
                put(f.to_bits());
            }
            put(s.cargo);
            put(s.score);
            put(s.alive as u32);
            put(s.respawn as u32);
            for l in s.levels {
                put(l as u32);
            }
        }
        for r in &self.rocks {
            for f in [r.pos.x, r.pos.y, r.hp] {
                put(f.to_bits());
            }
        }
        put(self.crystals.len() as u32);
        for c in &self.crystals {
            put(c.pos.x.to_bits());
            put(c.pos.y.to_bits());
        }
        put(self.shots.len() as u32);
        for s in &self.shots {
            put(s.pos.x.to_bits());
            put(s.pos.y.to_bits());
        }
        h
    }
}

fn wrap(size: f32, p: Vec2) -> Vec2 {
    Vec2::new(p.x.rem_euclid(size), p.y.rem_euclid(size))
}

fn delta(size: f32, a: Vec2, b: Vec2) -> Vec2 {
    let fold = |d: f32| {
        if d > size / 2.0 {
            d - size
        } else if d < -size / 2.0 {
            d + size
        } else {
            d
        }
    };
    Vec2::new(fold(b.x - a.x), fold(b.y - a.y))
}
