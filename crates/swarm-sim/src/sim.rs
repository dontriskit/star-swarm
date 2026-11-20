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
    /// Damage per shot.
    Attack,
    /// Hull points.
    Health,
    /// Shots per second.
    FireRate,
    /// Shield points. Shields take damage before the hull and grow back on their own.
    Shield,
    /// Thrust and top speed.
    Speed,
    /// How fast shields grow back.
    Regen,
    /// How fast the ship turns.
    Agility,
    /// How many shots the ship carries and how fast they come back.
    Ammo,
}

pub const UPGRADES: [Upgrade; 8] = [
    Upgrade::Attack,
    Upgrade::Health,
    Upgrade::FireRate,
    Upgrade::Shield,
    Upgrade::Speed,
    Upgrade::Regen,
    Upgrade::Agility,
    Upgrade::Ammo,
];

/// What kind of ship it is. Every ship starts as a Basic; moving to tier 2 means picking one of the others.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Class {
    /// Tier 1: one laser, middling in everything.
    #[default]
    Basic,
    /// Tier 2: one fast, hard-hitting laser with a long reach and a slow rate of fire.
    Lancer,
    /// Tier 2: two lasers side by side.
    Twin,
    /// Tier 2: three lasers in a fan, short reach, fast fire.
    Scatter,
}

pub const CLASSES: [Class; 4] = [Class::Basic, Class::Lancer, Class::Twin, Class::Scatter];
/// The classes a tier-1 ship can become.
pub const TIER2: [Class; 3] = [Class::Lancer, Class::Twin, Class::Scatter];

/// A class's guns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Weapon {
    pub lasers: u8,
    /// Angle between neighbouring lasers, radians; 0 fires them side by side.
    pub fan: f32,
    /// How far a shot flies.
    pub range: f32,
    pub speed: f32,
    /// Ticks between volleys.
    pub cooldown: u16,
    /// Damage per laser at the muzzle.
    pub damage: f32,
}

/// What a ship can spend crystals on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Buy {
    Upgrade(Upgrade),
    /// Move up to tier 2 as the given class: stronger hull and shields, new guns, higher upgrade cap.
    Tier(Class),
}

/// Ship tiers run from 1 to this.
pub const TIERS: u8 = 2;

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
    /// Burn the boost tank for extra thrust and top speed.
    pub boost: bool,
    pub buy: Option<Buy>,
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
    pub shield: f32,
    /// Shield points per second once regeneration has started.
    pub shield_regen: f32,
    /// Ticks without taking damage before shields start to grow back.
    pub shield_delay: u16,
    /// Cargo room per tier.
    pub cargo: [u32; TIERS as usize],
    /// Hull, shield and damage multiplier per tier.
    pub tier_power: [f32; TIERS as usize],
    /// Highest upgrade level per tier.
    pub tier_levels: [u8; TIERS as usize],
    /// Crystals to move from tier 1 to tier 2.
    pub tier_cost: u32,

    pub base_hp: f32,
    pub base_radius: f32,
    /// Ships this close to their own standing base are repaired.
    pub base_heal_radius: f32,
    /// Hull points per second repaired at the base.
    pub base_heal: f32,
    /// A standing base shoots the nearest enemy ship within this range.
    pub turret_range: f32,
    /// Damage per turret shot; 0 switches turrets off.
    pub turret_damage: f32,
    /// Ticks between turret shots.
    pub turret_cooldown: u16,

    /// Guns per class, indexed by [`Class`].
    pub weapons: [Weapon; 4],
    /// Share of its damage a shot still does at the end of its range; it fades linearly on the way.
    pub shot_falloff: f32,
    /// Shots carried and shots regained per second, before upgrades.
    pub ammo: f32,
    pub ammo_regen: f32,
    /// Thrust and top-speed multipliers while boosting.
    pub boost_thrust: f32,
    pub boost_speed: f32,
    /// Share of the boost tank burned and regained per second.
    pub boost_drain: f32,
    pub boost_regen: f32,
    /// Chance that a crystal dropped by a rock is a boost cell, which refills the tank.
    pub boost_cell_chance: f32,
    /// Deaths before a ship is demoted: tier 2 falls to tier 1, tier 1 loses its upgrades.
    pub lives: u8,

    pub rock_radius: f32,
    pub rock_hp: f32,
    pub rock_speed: f32,
    pub crash_damage: f32,

    pub crystal_ticks: u16,
    pub pickup_radius: f32,

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
            max_ticks: 18000,
            ship_radius: 12.0,
            thrust_accel: 220.0,
            drag: 0.6,
            max_speed: 240.0,
            turn_rate: 3.2,
            hull: 100.0,
            shield: 40.0,
            shield_regen: 4.0,
            shield_delay: 60,
            cargo: [60, 120],
            tier_power: [1.0, 1.5],
            tier_levels: [3, 5],
            tier_cost: 60,
            base_hp: 1000.0,
            base_radius: 60.0,
            base_heal_radius: 120.0,
            base_heal: 10.0,
            turret_range: 380.0,
            turret_damage: 15.0,
            turret_cooldown: 8,
            weapons: [
                Weapon { lasers: 1, fan: 0.0, range: 470.0, speed: 520.0, cooldown: 8, damage: 10.0 },
                Weapon { lasers: 1, fan: 0.0, range: 800.0, speed: 800.0, cooldown: 14, damage: 30.0 },
                Weapon { lasers: 2, fan: 0.0, range: 500.0, speed: 560.0, cooldown: 8, damage: 11.0 },
                Weapon { lasers: 3, fan: 0.14, range: 300.0, speed: 480.0, cooldown: 5, damage: 8.0 },
            ],
            shot_falloff: 0.4,
            ammo: 30.0,
            ammo_regen: 4.0,
            boost_thrust: 2.5,
            boost_speed: 1.6,
            boost_drain: 0.5,
            boost_regen: 0.1,
            boost_cell_chance: 0.15,
            lives: 3,
            rock_radius: 18.0,
            rock_hp: 30.0,
            rock_speed: 12.0,
            crash_damage: 0.15,
            crystal_ticks: 600,
            pickup_radius: 26.0,
            upgrade_cost: 10,
            upgrade_step: 0.2,
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
    fn power(&self, ship: &Ship) -> f32 {
        self.tier_power[ship.tier as usize - 1]
    }
    pub fn hull_max(&self, s: &Ship) -> f32 {
        self.hull * self.power(s) * self.boost(s, Upgrade::Health)
    }
    pub fn shield_max(&self, s: &Ship) -> f32 {
        self.shield * self.power(s) * self.boost(s, Upgrade::Shield)
    }
    pub fn weapon(&self, class: Class) -> &Weapon {
        &self.weapons[class as usize]
    }
    /// Damage per laser at the muzzle.
    pub fn damage(&self, s: &Ship) -> f32 {
        self.weapon(s.class).damage * self.boost(s, Upgrade::Attack)
    }
    /// Ticks between volleys.
    pub fn cooldown(&self, s: &Ship) -> u16 {
        ((self.weapon(s.class).cooldown as f32 / self.boost(s, Upgrade::FireRate)).round() as u16).max(2)
    }
    pub fn ammo_max(&self, s: &Ship) -> f32 {
        self.ammo * self.boost(s, Upgrade::Ammo)
    }
    pub fn turn_rate_of(&self, s: &Ship) -> f32 {
        self.turn_rate * self.boost(s, Upgrade::Agility)
    }
    /// How many ticks a shot of this class flies.
    pub fn shot_ticks(&self, class: Class) -> u16 {
        let w = self.weapon(class);
        ((w.range / (w.speed * self.dt)).round() as u16).max(1)
    }
    pub fn cargo_max(&self, s: &Ship) -> u32 {
        self.cargo[s.tier as usize - 1]
    }
    /// Highest upgrade level a ship of this tier can reach.
    pub fn max_level(&self, tier: u8) -> u8 {
        self.tier_levels[tier as usize - 1]
    }
    /// The highest upgrade level any ship can reach.
    pub fn top_level(&self) -> u8 {
        self.tier_levels[TIERS as usize - 1]
    }
    /// Price of the next level of an upgrade for a ship of this tier, or None at the cap.
    pub fn cost(&self, level: u8, tier: u8) -> Option<u32> {
        (level < self.max_level(tier)).then(|| self.upgrade_cost * (level as u32 + 1))
    }
    /// Price of what a ship wants to buy, or None when it cannot be bought at all.
    pub fn price(&self, s: &Ship, buy: Buy) -> Option<u32> {
        match buy {
            Buy::Upgrade(u) => self.cost(s.levels[u as usize], s.tier),
            Buy::Tier(class) => (s.tier < TIERS && class != Class::Basic).then_some(self.tier_cost),
        }
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
    pub shield: f32,
    /// Ticks left before shields start growing back.
    pub shield_wait: u16,
    /// 1 or 2.
    pub tier: u8,
    pub class: Class,
    /// Deaths left before the ship is demoted.
    pub lives: u8,
    /// Shots on board.
    pub ammo: f32,
    /// Boost tank, 0 to 1.
    pub boost: f32,
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
    /// A boost cell refills the boost tank instead of going into the cargo hold.
    pub boost_cell: bool,
}

#[derive(Clone, Debug)]
pub struct Shot {
    pub pos: Vec2,
    pub vel: Vec2,
    /// Ticks left, and the ticks it started with.
    pub ticks: u16,
    pub life: u16,
    /// Damage at the muzzle; see [`Shot::damage_now`].
    pub damage: f32,
    /// The ship that fired it; None for a base turret.
    pub owner: Option<u16>,
    pub team: u8,
}

impl Shot {
    /// Damage fades linearly to `falloff` of the muzzle damage over the shot's range.
    pub fn damage_now(&self, falloff: f32) -> f32 {
        let flown = 1.0 - self.ticks as f32 / self.life.max(1) as f32;
        self.damage * (1.0 - (1.0 - falloff) * flown)
    }
}

/// A team's home: where its ships appear and get repaired. Losing it loses the match.
#[derive(Clone, Debug)]
pub struct Base {
    pub pos: Vec2,
    pub hp: f32,
    /// Ticks until the turret can fire again.
    pub cooldown: u16,
}

impl Base {
    pub fn alive(&self) -> bool {
        self.hp > 0.0
    }
}

/// What happened during one step. Rewards and the GUI's effects read these.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    RockDestroyed { by: Option<u16>, size: u8 },
    Collected { ship: u16 },
    Bought { ship: u16, what: Buy },
    BaseHit { by: u16, team: u8, damage: f32 },
    BaseDestroyed { by: u16, team: u8 },
    /// Out of lives: a tier-2 ship fell back to tier 1, or a tier-1 ship lost its upgrades.
    Demoted { ship: u16 },
    /// `by` is None when a base turret did it.
    Hit { by: Option<u16>, victim: u16, damage: f32 },
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
    /// One per team, indexed by team.
    pub bases: Vec<Base>,
    pub events: Vec<Event>,
    rng: Rng,
}

const CRYSTALS_PER_SIZE: [u32; 4] = [0, 2, 5, 10];
const MAX_DROPPED: u32 = 8;

impl World {
    pub fn new(cfg: Config, seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        // Evenly spaced along the diagonal. The map wraps, so every team sees the
        // same layout from its own base and none starts closer to its enemies.
        let bases: Vec<Base> = (0..cfg.teams)
            .map(|t| {
                let along = (t as f32 + 0.5) / cfg.teams as f32 * cfg.world_size;
                Base { pos: Vec2::new(along, along), hp: cfg.base_hp, cooldown: 0 }
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
                    shield: 0.0,
                    shield_wait: 0,
                    tier: 1,
                    class: Class::Basic,
                    lives: world.cfg.lives,
                    ammo: 0.0,
                    boost: 1.0,
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

    /// The match ends when time runs out or at most one base is left standing.
    pub fn done(&self) -> bool {
        self.tick >= self.cfg.max_ticks || (self.bases.len() > 1 && self.standing() <= 1)
    }

    fn standing(&self) -> usize {
        self.bases.iter().filter(|b| b.alive()).count()
    }

    /// The team whose base is the only one left, if the match has been won.
    pub fn winner(&self) -> Option<u8> {
        (self.bases.len() > 1 && self.standing() == 1).then(|| self.bases.iter().position(Base::alive).unwrap() as u8)
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

    fn place(cfg: &Config, bases: &[Base], rng: &mut Rng, ship: &mut Ship) {
        let base = bases[ship.team as usize].pos;
        let offset = Vec2::from_angle(rng.range(0.0, std::f32::consts::TAU)).scale(rng.range(20.0, 90.0));
        ship.pos = wrap(cfg.world_size, base.add(offset));
        ship.vel = Vec2::default();
        ship.heading = rng.range(0.0, std::f32::consts::TAU);
        ship.hull = cfg.hull_max(ship);
        ship.shield = cfg.shield_max(ship);
        ship.shield_wait = 0;
        ship.ammo = cfg.ammo_max(ship);
        ship.boost = 1.0;
        ship.cooldown = 0;
        ship.alive = true;
        ship.last_hit_by = None;
    }

    /// Shields soak up damage first; what is left goes to the hull.
    fn hurt(cfg: &Config, ship: &mut Ship, damage: f32) {
        let absorbed = ship.shield.min(damage);
        ship.shield -= absorbed;
        ship.hull -= damage - absorbed;
        ship.shield_wait = cfg.shield_delay;
    }

    fn new_rock(cfg: &Config, bases: &[Base], rng: &mut Rng) -> Rock {
        let mut pos = Vec2::default();
        for _ in 0..10 {
            pos = Vec2::new(rng.range(0.0, cfg.world_size), rng.range(0.0, cfg.world_size));
            if bases.iter().all(|b| delta(cfg.world_size, b.pos, pos).len() > 200.0) {
                break;
            }
        }
        let size = 1 + rng.below(3) as u8;
        let vel = Vec2::from_angle(rng.range(0.0, std::f32::consts::TAU)).scale(rng.range(0.0, cfg.rock_speed));
        Rock { pos, vel, size, hp: cfg.rock_hp_of(size), last_hit_by: None }
    }

    /// Drops crystals around a point; `cells` lets some of them be boost cells.
    fn scatter(&mut self, at: Vec2, count: u32, spread: f32, cells: bool) {
        for _ in 0..count {
            let offset = Vec2::from_angle(self.rng.range(0.0, std::f32::consts::TAU)).scale(self.rng.range(0.0, spread));
            let boost_cell = cells && self.rng.unit() < self.cfg.boost_cell_chance;
            self.crystals.push(Crystal { pos: wrap(self.cfg.world_size, at.add(offset)), ticks: self.cfg.crystal_ticks, boost_cell });
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
            let home = &self.bases[ship.team as usize];
            if !ship.alive {
                // A team without a base gets no new ships.
                if home.alive() {
                    ship.respawn = ship.respawn.saturating_sub(1);
                    if ship.respawn == 0 {
                        Self::place(cfg, &self.bases, &mut self.rng, ship);
                    }
                }
                continue;
            }
            let act = actions[i];
            if let Some(what) = act.buy {
                if let Some(cost) = cfg.price(ship, what).filter(|cost| ship.cargo >= *cost) {
                    ship.cargo -= cost;
                    match what {
                        Buy::Upgrade(u) => ship.levels[u as usize] += 1,
                        Buy::Tier(class) => {
                            ship.tier += 1;
                            ship.class = class;
                            ship.hull = cfg.hull_max(ship);
                            ship.shield = cfg.shield_max(ship);
                        }
                    }
                    self.events.push(Event::Bought { ship: i as u16, what });
                }
            }
            ship.heading += act.turn.clamp(-1, 1) as f32 * cfg.turn_rate_of(ship) * dt;
            ship.heading = ship.heading.rem_euclid(std::f32::consts::TAU);
            let speed_boost = cfg.boost(ship, Upgrade::Speed);
            // Boosting burns the tank and needs the engine on; otherwise the tank slowly refills.
            let boosting = act.boost && act.thrust && ship.boost > 0.0;
            ship.boost = if boosting { (ship.boost - cfg.boost_drain * dt).max(0.0) } else { (ship.boost + cfg.boost_regen * dt).min(1.0) };
            let (push, top) = if boosting { (cfg.boost_thrust, cfg.boost_speed) } else { (1.0, 1.0) };
            if act.thrust {
                ship.vel = ship.vel.add(Vec2::from_angle(ship.heading).scale(cfg.thrust_accel * speed_boost * push * dt));
            }
            ship.vel = ship.vel.scale(1.0 - cfg.drag * dt);
            let (speed, cap) = (ship.vel.len(), cfg.max_speed * speed_boost * top);
            if speed > cap {
                ship.vel = ship.vel.scale(cap / speed);
            }
            ship.pos = wrap(size, ship.pos.add(ship.vel.scale(dt)));
            if ship.shield_wait > 0 {
                ship.shield_wait -= 1;
            } else {
                ship.shield = (ship.shield + cfg.shield_regen * cfg.boost(ship, Upgrade::Regen) * dt).min(cfg.shield_max(ship));
            }
            if home.alive() && delta(size, home.pos, ship.pos).len() < cfg.base_heal_radius {
                ship.hull = (ship.hull + cfg.base_heal * dt).min(cfg.hull_max(ship));
            }
            ship.ammo = (ship.ammo + cfg.ammo_regen * cfg.boost(ship, Upgrade::Ammo) * dt).min(cfg.ammo_max(ship));
            ship.cooldown = ship.cooldown.saturating_sub(1);
            let weapon = *cfg.weapon(ship.class);
            if act.fire && ship.cooldown == 0 && ship.ammo >= weapon.lasers as f32 {
                ship.cooldown = cfg.cooldown(ship);
                ship.ammo -= weapon.lasers as f32;
                let life = cfg.shot_ticks(ship.class);
                let side = Vec2::from_angle(ship.heading + std::f32::consts::FRAC_PI_2);
                for laser in 0..weapon.lasers {
                    // Lasers sit symmetrically around the nose: fanned out, or side by side.
                    let slot = laser as f32 - (weapon.lasers - 1) as f32 / 2.0;
                    let dir = Vec2::from_angle(ship.heading + slot * weapon.fan);
                    let offset = if weapon.fan == 0.0 { side.scale(slot * 14.0) } else { Vec2::default() };
                    self.shots.push(Shot {
                        pos: wrap(size, ship.pos.add(dir.scale(cfg.ship_radius)).add(offset)),
                        vel: ship.vel.add(dir.scale(weapon.speed)),
                        ticks: life,
                        life,
                        damage: cfg.damage(ship),
                        owner: Some(i as u16),
                        team: ship.team,
                    });
                }
            }
        }

        // Turrets: every standing base fires at the nearest enemy ship in range, leading its target.
        if self.cfg.turret_damage > 0.0 {
            for (team, base) in self.bases.iter_mut().enumerate() {
                base.cooldown = base.cooldown.saturating_sub(1);
                if !base.alive() || base.cooldown > 0 {
                    continue;
                }
                let target = self
                    .ships
                    .iter()
                    .filter(|s| s.alive && s.team as usize != team)
                    .map(|s| (delta(size, base.pos, s.pos), s.vel))
                    .filter(|(d, _)| d.len() < self.cfg.turret_range)
                    .min_by(|a, b| a.0.len().total_cmp(&b.0.len()));
                if let Some((to, vel)) = target {
                    let gun = self.cfg.weapon(Class::Basic);
                    let lead = to.add(vel.scale(to.len() / gun.speed));
                    let dir = lead.scale(1.0 / lead.len().max(1e-3));
                    let life = self.cfg.shot_ticks(Class::Basic);
                    base.cooldown = self.cfg.turret_cooldown;
                    self.shots.push(Shot {
                        pos: wrap(size, base.pos.add(dir.scale(self.cfg.base_radius))),
                        vel: dir.scale(gun.speed),
                        ticks: life,
                        life,
                        damage: self.cfg.turret_damage,
                        owner: None,
                        team: team as u8,
                    });
                }
            }
        }

        // Shots: move, then hit the first rock or enemy ship they touch.
        let falloff = self.cfg.shot_falloff;
        for shot in &mut self.shots {
            shot.pos = wrap(size, shot.pos.add(shot.vel.scale(dt)));
            shot.ticks -= 1;
            let damage = shot.damage_now(falloff);
            if let Some(rock) = self
                .rocks
                .iter_mut()
                .find(|r| r.hp > 0.0 && delta(size, r.pos, shot.pos).len() < self.cfg.rock_radius * r.size as f32)
            {
                rock.hp -= damage;
                rock.last_hit_by = shot.owner;
                shot.ticks = 0;
                continue;
            }
            let hit = self.ships.iter().position(|s| {
                s.alive && s.team != shot.team && delta(size, s.pos, shot.pos).len() < self.cfg.ship_radius
            });
            if let Some(v) = hit {
                Self::hurt(&self.cfg, &mut self.ships[v], damage);
                self.ships[v].last_hit_by = shot.owner;
                self.events.push(Event::Hit { by: shot.owner, victim: v as u16, damage });
                shot.ticks = 0;
                continue;
            }
            // Only ships' shots damage bases; turret shots pass over them.
            let Some(owner) = shot.owner else { continue };
            let base = self.bases.iter().position(|b| b.alive() && delta(size, b.pos, shot.pos).len() < self.cfg.base_radius);
            if let Some(team) = base.filter(|team| *team as u8 != shot.team) {
                let dealt = damage.min(self.bases[team].hp);
                self.bases[team].hp -= damage;
                let damage = dealt;
                self.events.push(Event::BaseHit { by: owner, team: team as u8, damage });
                if !self.bases[team].alive() {
                    self.events.push(Event::BaseDestroyed { by: owner, team: team as u8 });
                }
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
                    Self::hurt(&self.cfg, ship, damage);
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
            self.scatter(pos, CRYSTALS_PER_SIZE[rock_size as usize], self.cfg.rock_radius * rock_size as f32, true);
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
                // A boost cell is worth picking up until the tank is full; a crystal needs cargo room.
                let wanted = if c.boost_cell { s.boost < 1.0 } else { s.cargo < cfg.cargo_max(s) };
                if !s.alive || !wanted {
                    continue;
                }
                let dist = delta(size, s.pos, c.pos).len();
                if dist < cfg.pickup_radius && best.is_none_or(|(_, b)| dist < b) {
                    best = Some((i, dist));
                }
            }
            if let Some((i, _)) = best {
                if c.boost_cell {
                    ships[i].boost = 1.0;
                    return false;
                }
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
            ship.lives = ship.lives.saturating_sub(1);
            if ship.lives == 0 {
                ship.lives = self.cfg.lives;
                if ship.tier > 1 {
                    ship.tier -= 1;
                    ship.class = Class::Basic;
                    let cap = self.cfg.max_level(ship.tier);
                    ship.levels.iter_mut().for_each(|l| *l = (*l).min(cap));
                } else {
                    ship.levels = [0; 8];
                }
                self.events.push(Event::Demoted { ship: i as u16 });
            }
            if let Some(k) = by {
                self.ships[k as usize].kills += 1;
                self.ships[k as usize].score += self.cfg.kill_score;
            }
            self.events.push(Event::Killed { by, victim: i as u16 });
            self.scatter(pos, dropped, 30.0, false);
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
            for f in [s.pos.x, s.pos.y, s.vel.x, s.vel.y, s.heading, s.hull, s.shield] {
                put(f.to_bits());
            }
            put(s.tier as u32);
            put(s.class as u32);
            put(s.lives as u32);
            put(s.ammo.to_bits());
            put(s.boost.to_bits());
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
        for b in &self.bases {
            put(b.hp.to_bits());
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
