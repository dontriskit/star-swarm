use crate::geo::{self, Vec3};
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
    /// Tier 2: a heavy hull and a weak gun, built to fly into enemies: it deals a lot of
    /// ramming damage and takes little.
    Ram,
}

pub const CLASSES: [Class; 5] = [Class::Basic, Class::Lancer, Class::Twin, Class::Scatter, Class::Ram];
/// The classes a tier-1 ship can become.
pub const TIER2: [Class; 4] = [Class::Lancer, Class::Twin, Class::Scatter, Class::Ram];

/// A class's build, as multiples of the basic ship.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    pub hull: f32,
    /// Ramming damage it deals to an enemy it collides with.
    pub ram_deal: f32,
    /// Ramming damage it takes.
    pub ram_take: f32,
}

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
    /// Move up one tier: a bigger, stronger ship with a higher upgrade cap, starting again
    /// without upgrades. Leaving tier 1 means picking the class; later the class is kept.
    Tier(Class),
}

/// Ship tiers run from 1 to this.
pub const TIERS: u8 = 3;

/// What a team does together. Ships vote; the plan with the most votes becomes every ship's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Plan {
    /// Mine together, staying near each other.
    Farm,
    /// Gather at the team's own base and fight whoever comes.
    Defend,
    /// Attack the first enemy base still standing, counting teams on from one's own.
    AttackA,
    /// Attack the second one (the same base as AttackA when only one is left).
    AttackB,
}

pub const PLANS: [Plan; 4] = [Plan::Farm, Plan::Defend, Plan::AttackA, Plan::AttackB];

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
    /// A position on the planet; the zero vector when the order has no target.
    pub target: Vec3,
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
    /// Vote for what the team should do; None keeps the ship's last vote.
    pub vote: Option<Plan>,
}

#[derive(Clone, Debug)]
pub struct Config {
    /// Radius of the planet the match is played around. Distances are measured along its surface.
    pub radius: f32,
    pub dt: f32,
    pub teams: usize,
    pub ships_per_team: usize,
    pub rock_count: usize,
    pub max_ticks: u32,

    /// Radius of a tier-1 ship. See [`Config::ship_radii`] for the others.
    pub ship_radius: f32,
    /// Radius of a ship per tier: bigger ships take up more room and are easier to hit.
    pub ship_radii: [f32; TIERS as usize],
    /// Share of the closing speed two basic enemy ships each take as damage when they collide;
    /// a class's [`Body`] scales what it deals and what it takes.
    pub ram_damage: f32,
    /// Ticks between counts of the ships' votes on the team plan.
    pub plan_ticks: u32,
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
    /// Highest upgrade level per tier.
    pub tier_levels: [u8; TIERS as usize],
    /// Crystals to move up from tier 1, and from tier 2.
    pub tier_cost: [u32; TIERS as usize - 1],
    /// How much one level of each upgrade adds, indexed by [`Upgrade`].
    pub upgrade_steps: [f32; 8],
    /// A ship fresh into a tier is this fraction of a fully upgraded ship of the tier below,
    /// in every stat: a little worse at first, better once upgraded again.
    pub tier_discount: f32,

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
    pub weapons: [Weapon; 5],
    /// Build per class, indexed by [`Class`].
    pub bodies: [Body; 5],
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
    /// Once the shields are down, carried crystals take the damage before the hull does:
    /// each crystal soaks up this many points and is knocked out of the hold. 0 switches this off.
    pub crystal_armor: f32,

    pub rock_radius: f32,
    pub rock_hp: f32,
    pub rock_speed: f32,
    pub crash_damage: f32,

    pub crystal_ticks: u16,
    pub pickup_radius: f32,

    pub upgrade_cost: u32,

    pub respawn_ticks: u16,
    pub kill_score: u32,
    pub view_range: f32,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            radius: 1500.0,
            dt: 1.0 / 30.0,
            teams: 3,
            ships_per_team: 5,
            rock_count: 420,
            max_ticks: 18000,
            ship_radius: 12.0,
            ship_radii: [12.0, 15.0, 19.0],
            ram_damage: 0.1,
            plan_ticks: 30,
            thrust_accel: 220.0,
            drag: 0.6,
            max_speed: 240.0,
            turn_rate: 3.2,
            hull: 100.0,
            shield: 40.0,
            shield_regen: 4.0,
            shield_delay: 60,
            cargo: [60, 120, 200],
            tier_levels: [3, 4, 5],
            tier_cost: [60, 120],
            // Attack, health, fire rate, shield, speed, regen, agility, ammo: fighting stats grow
            // fast, mobility slowly.
            upgrade_steps: [0.2, 0.2, 0.05, 0.2, 0.05, 0.2, 0.05, 0.2],
            tier_discount: 0.95,
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
                Weapon { lasers: 1, fan: 0.0, range: 260.0, speed: 480.0, cooldown: 10, damage: 8.0 },
            ],
            bodies: [
                Body { hull: 1.0, ram_deal: 1.0, ram_take: 1.0 },
                // The sniper pays for its reach with a thin hull.
                Body { hull: 0.7, ram_deal: 0.5, ram_take: 1.5 },
                Body { hull: 1.0, ram_deal: 1.0, ram_take: 1.0 },
                Body { hull: 1.0, ram_deal: 1.0, ram_take: 1.0 },
                Body { hull: 1.6, ram_deal: 5.0, ram_take: 0.25 },
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
            crystal_armor: 1.0,
            rock_radius: 18.0,
            rock_hp: 30.0,
            rock_speed: 12.0,
            crash_damage: 0.15,
            crystal_ticks: 600,
            pickup_radius: 26.0,
            upgrade_cost: 10,
            respawn_ticks: 90,
            kill_score: 20,
            view_range: 600.0,
        }
    }
}

impl Config {
    /// What a ship of a tier starts from in one stat, as a multiple of a new tier-1 ship:
    /// a fully upgraded ship of the tier below, less the discount.
    pub fn tier_base(&self, u: Upgrade, tier: u8) -> f32 {
        (1..tier).fold(1.0, |base, t| base * (1.0 + self.upgrade_steps[u as usize] * self.max_level(t) as f32) * self.tier_discount)
    }
    /// A ship's multiplier in one stat: its tier's starting point, raised by its upgrade level.
    pub fn boost(&self, ship: &Ship, u: Upgrade) -> f32 {
        self.tier_base(u, ship.tier) * (1.0 + self.upgrade_steps[u as usize] * ship.levels[u as usize] as f32)
    }
    pub fn radius_of(&self, s: &Ship) -> f32 {
        self.ship_radii[s.tier as usize - 1]
    }
    pub fn body(&self, class: Class) -> &Body {
        &self.bodies[class as usize]
    }
    pub fn hull_max(&self, s: &Ship) -> f32 {
        self.hull * self.body(s.class).hull * self.boost(s, Upgrade::Health)
    }
    pub fn shield_max(&self, s: &Ship) -> f32 {
        self.shield * self.boost(s, Upgrade::Shield)
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
            // Leaving tier 1 needs a real class; after that the class named is ignored.
            Buy::Tier(class) => (s.tier < TIERS && (s.tier > 1 || class != Class::Basic)).then(|| self.tier_cost[s.tier as usize - 1]),
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
    /// Where the ship is: a unit vector from the planet's centre.
    pub pos: Vec3,
    /// Tangent to the surface at `pos`, world units per second.
    pub vel: Vec3,
    /// The way the nose points: a unit vector tangent to the surface at `pos`.
    pub fwd: Vec3,
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
    /// Damage taken on the cargo that has not yet knocked a whole crystal loose.
    pub spill: f32,
    pub cargo: u32,
    pub levels: [u8; 8],
    /// The ship's standing vote on the team plan.
    pub vote: Option<Plan>,
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
    pub pos: Vec3,
    pub vel: Vec3,
    pub size: u8,
    pub hp: f32,
    pub last_hit_by: Option<u16>,
}

#[derive(Clone, Debug)]
pub struct Crystal {
    pub pos: Vec3,
    pub ticks: u16,
    /// A boost cell refills the boost tank instead of going into the cargo hold.
    pub boost_cell: bool,
}

#[derive(Clone, Debug)]
pub struct Shot {
    pub pos: Vec3,
    pub vel: Vec3,
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
    pub pos: Vec3,
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
    /// The plan each team's ships last voted in, indexed by team; None until someone votes.
    pub plans: Vec<Option<Plan>>,
    /// Teams whose orders come from outside (a human commander): their votes are not counted.
    pub commanded: Vec<bool>,
    pub events: Vec<Event>,
    rng: Rng,
}

const CRYSTALS_PER_SIZE: [u32; 4] = [0, 2, 5, 10];
const MAX_DROPPED: u32 = 8;

impl World {
    pub fn new(cfg: Config, seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        // Evenly spaced around the equator, so every team sees the same layout
        // from its own base and none starts closer to its enemies.
        let bases: Vec<Base> = (0..cfg.teams)
            .map(|t| Base { pos: Vec3::from_lon_lat(std::f32::consts::TAU * t as f32 / cfg.teams as f32, 0.0), hp: cfg.base_hp, cooldown: 0 })
            .collect();
        let mut world = World {
            tick: 0,
            ships: Vec::new(),
            rocks: Vec::new(),
            crystals: Vec::new(),
            shots: Vec::new(),
            plans: vec![None; bases.len()],
            commanded: vec![false; bases.len()],
            bases,
            events: Vec::new(),
            rng: Rng::new(0),
            cfg,
        };
        for team in 0..world.cfg.teams {
            for _ in 0..world.cfg.ships_per_team {
                let mut ship = Ship {
                    team: team as u8,
                    pos: Vec3::new(1.0, 0.0, 0.0),
                    vel: Vec3::default(),
                    fwd: Vec3::new(0.0, 1.0, 0.0),
                    hull: 0.0,
                    shield: 0.0,
                    shield_wait: 0,
                    tier: 1,
                    class: Class::Basic,
                    lives: world.cfg.lives,
                    ammo: 0.0,
                    boost: 1.0,
                    spill: 0.0,
                    cargo: 0,
                    levels: [0; 8],
                    vote: None,
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

    /// Counts every team's votes and turns the winning plan into the order of all its ships.
    /// Teams commanded from outside, and teams where nobody has voted, keep their orders.
    fn count_votes(&mut self) {
        for team in 0..self.bases.len() {
            if self.commanded[team] {
                continue;
            }
            let mut tally = [0u32; PLANS.len()];
            let mut middle = Vec3::default();
            for s in self.ships.iter().filter(|s| s.team as usize == team && s.alive) {
                middle = middle.add(s.pos);
                if let Some(vote) = s.vote {
                    tally[vote as usize] += 1;
                }
            }
            let most = *tally.iter().max().unwrap_or(&0);
            if most == 0 {
                continue;
            }
            // On a tie the standing plan stays if it is among the leaders.
            let plan = self.plans[team].filter(|p| tally[*p as usize] == most).unwrap_or_else(|| PLANS[tally.iter().position(|n| *n == most).unwrap()]);
            self.plans[team] = Some(plan);
            let home = self.bases[team].pos;
            // Enemy bases still standing, counting teams on from this one.
            let enemies: Vec<Vec3> = (1..self.bases.len()).map(|k| &self.bases[(team + k) % self.bases.len()]).filter(|b| b.alive()).map(|b| b.pos).collect();
            let order = match plan {
                // The team's own middle is the place to stay near.
                Plan::Farm => Order { kind: OrderKind::Mine, target: if middle.len() > 1e-3 { middle.unit() } else { home } },
                Plan::Defend => Order { kind: OrderKind::Defend, target: home },
                Plan::AttackA => Order { kind: OrderKind::Attack, target: enemies.first().copied().unwrap_or(home) },
                Plan::AttackB => Order { kind: OrderKind::Attack, target: enemies.get(1).or(enemies.first()).copied().unwrap_or(home) },
            };
            self.ships.iter_mut().filter(|s| s.team as usize == team).for_each(|s| s.order = order);
        }
    }

    /// Distance between two positions along the surface.
    pub fn dist(&self, a: Vec3, b: Vec3) -> f32 {
        geo::dist(self.cfg.radius, a, b)
    }

    /// The position `dist` away from `from` in the direction `angle` (radians, in the
    /// arbitrary but fixed local compass of [`geo::basis`]).
    pub fn offset(&self, from: Vec3, angle: f32, dist: f32) -> Vec3 {
        geo::travel(self.cfg.radius, from, geo::bearing(from, angle), dist)
    }

    /// Points a ship's nose at a position.
    pub fn face(&mut self, ship: usize, at: Vec3) {
        let s = &mut self.ships[ship];
        s.fwd = geo::tangent(s.pos, at).unit();
    }

    /// Where `to` lies as seen from a ship at `from` facing `fwd`: x forward, y to the left,
    /// as far away as the way there along the surface.
    pub fn to_frame(&self, from: Vec3, fwd: Vec3, to: Vec3) -> Vec2 {
        geo::frame(from, fwd, geo::toward(self.cfg.radius, from, to))
    }

    fn place(cfg: &Config, bases: &[Base], rng: &mut Rng, ship: &mut Ship) {
        let base = bases[ship.team as usize].pos;
        let away = geo::bearing(base, rng.range(0.0, std::f32::consts::TAU));
        ship.pos = geo::travel(cfg.radius, base, away, rng.range(20.0, 90.0));
        ship.vel = Vec3::default();
        ship.fwd = geo::bearing(ship.pos, rng.range(0.0, std::f32::consts::TAU));
        ship.hull = cfg.hull_max(ship);
        ship.shield = cfg.shield_max(ship);
        ship.shield_wait = 0;
        ship.ammo = cfg.ammo_max(ship);
        ship.boost = 1.0;
        ship.cooldown = 0;
        ship.alive = true;
        ship.last_hit_by = None;
    }

    /// Shields soak up damage first, then the crystals in the hold, and only then the hull.
    /// Returns how many crystals were knocked out of the hold.
    fn hurt(cfg: &Config, ship: &mut Ship, damage: f32) -> u32 {
        let absorbed = ship.shield.min(damage);
        ship.shield -= absorbed;
        ship.shield_wait = cfg.shield_delay;
        let mut left = damage - absorbed;
        let mut dropped = 0;
        if cfg.crystal_armor > 0.0 && ship.cargo > 0 && left > 0.0 {
            ship.spill += left / cfg.crystal_armor;
            let loose = ship.spill.floor();
            if loose as u32 > ship.cargo {
                // More than the hold could take: the rest reaches the hull.
                left = (ship.spill - ship.cargo as f32) * cfg.crystal_armor;
                dropped = ship.cargo;
                ship.spill = 0.0;
            } else {
                left = 0.0;
                dropped = loose as u32;
                ship.spill -= loose;
            }
            ship.cargo -= dropped;
        }
        ship.hull -= left;
        dropped
    }

    fn new_rock(cfg: &Config, bases: &[Base], rng: &mut Rng) -> Rock {
        let mut pos = Vec3::new(1.0, 0.0, 0.0);
        for _ in 0..10 {
            // Uniform over the sphere: uniform height, uniform angle around the axis.
            let z = rng.range(-1.0, 1.0);
            let around = rng.range(0.0, std::f32::consts::TAU);
            let r = (1.0 - z * z).max(0.0).sqrt();
            pos = Vec3::new(r * around.cos(), r * around.sin(), z);
            if bases.iter().all(|b| geo::chord(cfg.radius, b.pos, pos) > 200.0) {
                break;
            }
        }
        let size = 1 + rng.below(3) as u8;
        let vel = geo::bearing(pos, rng.range(0.0, std::f32::consts::TAU)).scale(rng.range(0.0, cfg.rock_speed));
        Rock { pos, vel, size, hp: cfg.rock_hp_of(size), last_hit_by: None }
    }

    /// Drops crystals between `near` and `far` from a point; `cells` lets some of them be boost cells.
    fn scatter(&mut self, at: Vec3, count: u32, near: f32, far: f32, cells: bool) {
        for _ in 0..count {
            let away = geo::bearing(at, self.rng.range(0.0, std::f32::consts::TAU));
            let pos = geo::travel(self.cfg.radius, at, away, self.rng.range(near, far));
            let boost_cell = cells && self.rng.unit() < self.cfg.boost_cell_chance;
            self.crystals.push(Crystal { pos, ticks: self.cfg.crystal_ticks, boost_cell });
        }
    }

    /// Advances the match by one tick. `actions[i]` drives `ships[i]`.
    pub fn step(&mut self, actions: &[Action]) -> &[Event] {
        assert_eq!(actions.len(), self.ships.len(), "one action per ship");
        self.events.clear();
        self.tick += 1;
        let dt = self.cfg.dt;
        let radius = self.cfg.radius;

        // Votes: a ship keeps its vote until it casts another; every so often each team's
        // votes are counted and the winning plan becomes the order of all its ships.
        for (ship, act) in self.ships.iter_mut().zip(actions) {
            if act.vote.is_some() {
                ship.vote = act.vote;
            }
        }
        if self.tick % self.cfg.plan_ticks == 0 {
            self.count_votes();
        }

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
                            if ship.tier == 1 {
                                ship.class = class;
                            }
                            ship.tier += 1;
                            // A new tier starts again without upgrades.
                            ship.levels = [0; 8];
                            ship.hull = cfg.hull_max(ship);
                            ship.shield = cfg.shield_max(ship);
                        }
                    }
                    self.events.push(Event::Bought { ship: i as u16, what });
                }
            }
            ship.fwd = geo::turn(ship.pos, ship.fwd, act.turn.clamp(-1, 1) as f32 * cfg.turn_rate_of(ship) * dt);
            let speed_boost = cfg.boost(ship, Upgrade::Speed);
            // Boosting burns the tank and needs the engine on; otherwise the tank slowly refills.
            let boosting = act.boost && act.thrust && ship.boost > 0.0;
            ship.boost = if boosting { (ship.boost - cfg.boost_drain * dt).max(0.0) } else { (ship.boost + cfg.boost_regen * dt).min(1.0) };
            let (push, top) = if boosting { (cfg.boost_thrust, cfg.boost_speed) } else { (1.0, 1.0) };
            if act.thrust {
                ship.vel = ship.vel.add(ship.fwd.scale(cfg.thrust_accel * speed_boost * push * dt));
            }
            ship.vel = ship.vel.scale(1.0 - cfg.drag * dt);
            let (speed, cap) = (ship.vel.len(), cfg.max_speed * speed_boost * top);
            if speed > cap {
                ship.vel = ship.vel.scale(cap / speed);
            }
            (ship.pos, ship.vel) = geo::advance(radius, ship.pos, ship.vel, dt);
            // Carry the nose along: it has to stay tangent to the surface where the ship now is.
            ship.fwd = geo::tangent(ship.pos, ship.fwd).unit();
            if ship.shield_wait > 0 {
                ship.shield_wait -= 1;
            } else {
                ship.shield = (ship.shield + cfg.shield_regen * cfg.boost(ship, Upgrade::Regen) * dt).min(cfg.shield_max(ship));
            }
            if home.alive() && geo::chord(radius, home.pos, ship.pos) < cfg.base_heal_radius {
                ship.hull = (ship.hull + cfg.base_heal * dt).min(cfg.hull_max(ship));
            }
            ship.ammo = (ship.ammo + cfg.ammo_regen * cfg.boost(ship, Upgrade::Ammo) * dt).min(cfg.ammo_max(ship));
            ship.cooldown = ship.cooldown.saturating_sub(1);
            let weapon = *cfg.weapon(ship.class);
            if act.fire && ship.cooldown == 0 && ship.ammo >= weapon.lasers as f32 {
                ship.cooldown = cfg.cooldown(ship);
                ship.ammo -= weapon.lasers as f32;
                let life = cfg.shot_ticks(ship.class);
                let left = ship.pos.cross(ship.fwd);
                for laser in 0..weapon.lasers {
                    // Lasers sit symmetrically around the nose: fanned out, or side by side.
                    let slot = laser as f32 - (weapon.lasers - 1) as f32 / 2.0;
                    let dir = geo::turn(ship.pos, ship.fwd, slot * weapon.fan);
                    let aside = if weapon.fan == 0.0 { left.scale(slot * 14.0) } else { Vec3::default() };
                    self.shots.push(Shot {
                        pos: geo::nudge(radius, ship.pos, dir.scale(cfg.radius_of(ship)).add(aside)),
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
                    .filter(|s| s.alive && s.team as usize != team && geo::chord(radius, base.pos, s.pos) < self.cfg.turret_range)
                    .map(|s| (geo::toward(radius, base.pos, s.pos), geo::tangent(base.pos, s.vel)))
                    .min_by(|a, b| a.0.len().total_cmp(&b.0.len()));
                if let Some((to, vel)) = target {
                    let gun = self.cfg.weapon(Class::Basic);
                    let dir = to.add(vel.scale(to.len() / gun.speed)).unit();
                    let life = self.cfg.shot_ticks(Class::Basic);
                    base.cooldown = self.cfg.turret_cooldown;
                    self.shots.push(Shot {
                        pos: geo::nudge(radius, base.pos, dir.scale(self.cfg.base_radius)),
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

        // Crystals knocked out of holds this tick: where, and how many.
        let mut spilled: Vec<(Vec3, u32)> = Vec::new();

        // Shots: move, then hit the first rock or enemy ship they touch.
        let falloff = self.cfg.shot_falloff;
        for shot in &mut self.shots {
            (shot.pos, shot.vel) = geo::advance(radius, shot.pos, shot.vel, dt);
            shot.ticks -= 1;
            let damage = shot.damage_now(falloff);
            if let Some(rock) = self
                .rocks
                .iter_mut()
                .find(|r| r.hp > 0.0 && geo::chord(radius, r.pos, shot.pos) < self.cfg.rock_radius * r.size as f32)
            {
                rock.hp -= damage;
                rock.last_hit_by = shot.owner;
                shot.ticks = 0;
                continue;
            }
            let hit = self.ships.iter().position(|s| {
                s.alive && s.team != shot.team && geo::chord(radius, s.pos, shot.pos) < self.cfg.radius_of(s)
            });
            if let Some(v) = hit {
                let dropped = Self::hurt(&self.cfg, &mut self.ships[v], damage);
                spilled.push((self.ships[v].pos, dropped));
                self.ships[v].last_hit_by = shot.owner;
                self.events.push(Event::Hit { by: shot.owner, victim: v as u16, damage });
                shot.ticks = 0;
                continue;
            }
            // Only ships' shots damage bases; turret shots pass over them.
            let Some(owner) = shot.owner else { continue };
            let base = self.bases.iter().position(|b| b.alive() && geo::chord(radius, b.pos, shot.pos) < self.cfg.base_radius);
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
            (rock.pos, rock.vel) = geo::advance(radius, rock.pos, rock.vel, dt);
            for (i, ship) in self.ships.iter_mut().enumerate() {
                if !ship.alive {
                    continue;
                }
                let reach = self.cfg.rock_radius * rock.size as f32 + self.cfg.radius_of(ship);
                let dist = geo::chord(radius, rock.pos, ship.pos);
                if dist >= reach || dist < 1e-3 {
                    continue;
                }
                // Push the ship out along the way from the rock to it, and bounce it off.
                let out = geo::tangent(rock.pos, ship.pos).unit();
                ship.pos = geo::travel(radius, rock.pos, out, reach);
                ship.vel = geo::tangent(ship.pos, ship.vel);
                ship.fwd = geo::tangent(ship.pos, ship.fwd).unit();
                let n = geo::tangent(ship.pos, ship.pos.sub(rock.pos)).unit();
                let closing = ship.vel.sub(rock.vel).dot(n);
                if closing < 0.0 {
                    ship.vel = ship.vel.sub(n.scale(1.5 * closing));
                    let damage = -closing * self.cfg.crash_damage;
                    let dropped = Self::hurt(&self.cfg, ship, damage);
                    spilled.push((ship.pos, dropped));
                    ship.last_hit_by = None;
                    self.events.push(Event::Crashed { ship: i as u16, damage });
                }
            }
        }

        // Ships take up room: two that overlap are pushed apart and bounce; enemies also take
        // damage by how hard they met.
        for i in 0..self.ships.len() {
            for j in i + 1..self.ships.len() {
                let (a, b) = (&self.ships[i], &self.ships[j]);
                if !a.alive || !b.alive {
                    continue;
                }
                let reach = self.cfg.radius_of(a) + self.cfg.radius_of(b);
                let dist = geo::chord(radius, a.pos, b.pos);
                if dist >= reach || dist < 1e-3 {
                    continue;
                }
                // Each ship's way straight away from the other, along the surface where it is.
                let (na, nb) = (geo::tangent(a.pos, a.pos.sub(b.pos)).unit(), geo::tangent(b.pos, b.pos.sub(a.pos)).unit());
                let closing = a.vel.dot(na) + b.vel.dot(nb);
                let (enemies, apart) = (a.team != b.team, (reach - dist) / 2.0);
                // What each takes depends on how the other is built for ramming and how it is built to take it.
                let (body_a, body_b) = (*self.cfg.body(a.class), *self.cfg.body(b.class));
                let blows = [body_b.ram_deal * body_a.ram_take, body_a.ram_deal * body_b.ram_take];
                for ((k, n, other), blow) in [(i, na, j), (j, nb, i)].into_iter().zip(blows) {
                    let ship = &mut self.ships[k];
                    ship.pos = geo::travel(radius, ship.pos, n, apart);
                    ship.vel = geo::tangent(ship.pos, ship.vel);
                    ship.fwd = geo::tangent(ship.pos, ship.fwd).unit();
                    if closing < 0.0 {
                        ship.vel = ship.vel.sub(n.scale(0.75 * closing));
                        if enemies {
                            let damage = -closing * self.cfg.ram_damage * blow;
                            let dropped = Self::hurt(&self.cfg, ship, damage);
                            spilled.push((ship.pos, dropped));
                            ship.last_hit_by = Some(other as u16);
                            self.events.push(Event::Crashed { ship: k as u16, damage });
                        }
                    }
                }
            }
        }

        // Spilled crystals land a little way off, out of the ship's own reach, for anyone to pick up.
        for (at, count) in spilled {
            self.scatter(at, count, self.cfg.pickup_radius * 2.0, self.cfg.pickup_radius * 4.5, false);
        }

        // Broken rocks leave crystals and are replaced elsewhere.
        for r in 0..self.rocks.len() {
            if self.rocks[r].hp > 0.0 {
                continue;
            }
            let (pos, rock_size, by) = (self.rocks[r].pos, self.rocks[r].size, self.rocks[r].last_hit_by);
            self.events.push(Event::RockDestroyed { by, size: rock_size });
            self.scatter(pos, CRYSTALS_PER_SIZE[rock_size as usize], 0.0, self.cfg.rock_radius * rock_size as f32, true);
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
                let dist = geo::chord(radius, s.pos, c.pos);
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
                    if ship.tier == 1 {
                        ship.class = Class::Basic;
                    }
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
            self.scatter(pos, dropped, 0.0, 30.0, false);
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
        let spot = |p: Vec3| [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()];
        put(self.tick);
        for s in &self.ships {
            for bits in [spot(s.pos), spot(s.vel), spot(s.fwd)].concat() {
                put(bits);
            }
            put(s.hull.to_bits());
            put(s.shield.to_bits());
            put(s.tier as u32);
            put(s.class as u32);
            put(s.lives as u32);
            put(s.ammo.to_bits());
            put(s.boost.to_bits());
            put(s.spill.to_bits());
            put(s.cargo);
            put(s.score);
            put(s.alive as u32);
            put(s.respawn as u32);
            for l in s.levels {
                put(l as u32);
            }
        }
        for r in &self.rocks {
            spot(r.pos).into_iter().for_each(&mut put);
            put(r.hp.to_bits());
        }
        for b in &self.bases {
            put(b.hp.to_bits());
        }
        put(self.crystals.len() as u32);
        for c in &self.crystals {
            spot(c.pos).into_iter().for_each(&mut put);
        }
        put(self.shots.len() as u32);
        for s in &self.shots {
            spot(s.pos).into_iter().for_each(&mut put);
        }
        h
    }
}
