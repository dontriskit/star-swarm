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
    /// The team's base has to be at least the level of the tier moved up to.
    Tier(Class),
    /// A drone that follows the ship, breaks nearby rocks and gathers crystals for it.
    Drone,
    /// Repair the team's base.
    Repair,
}

/// Rocks come in sizes 1 to this: S, M, L, XL, XXL.
pub const ROCK_SIZES: u8 = 5;

/// Ship tiers run from 1 to this.
pub const TIERS: u8 = 3;

/// What a ship has set out to do, which its teammates can see. Farming is the default, and
/// defending is each ship's own choice. Attacking is a team decision: when at least half of a
/// team's living ships pick an attack, the whole team attacks.
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
    /// Switch to a mode; None keeps the ship's current one.
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
    /// Ticks between turning the ships' modes into fresh orders (targets move).
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
    /// Ammo carried and ammo regained per second, before upgrades.
    pub ammo: f32,
    pub ammo_regen: f32,
    /// How much damage one unit of ammo buys: a shot costs its muzzle damage divided by this,
    /// so harder-hitting shots, from a bigger gun or from attack upgrades, eat more ammo.
    pub ammo_damage: f32,
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
    /// Hit points of the smallest rock; a rock of size s has s squared times as many.
    pub rock_hp: f32,
    /// The pack of crystals a rock of each size leaves behind: fewest and most, by size.
    pub rock_packs: [(u32, u32); ROCK_SIZES as usize],
    /// How common each rock size is, relative to the others.
    pub rock_weights: [u32; ROCK_SIZES as usize],
    /// Highest level a base can reach.
    pub base_levels: u8,
    /// Crystals a team has to bring to its base, in total, for the base to reach level 2, 3, and so on.
    pub base_level_at: [u32; 6],
    /// Hit points a base gains with every level.
    pub base_hp_per_level: f32,
    /// Share of its damage a base's turret gains with every level.
    pub turret_per_level: f32,
    /// Crystals a ship next to its base hands over per tick.
    pub deposit_rate: u32,
    /// Points one repair costs, and base hit points it restores.
    pub repair_step: u32,
    /// Points a drone costs.
    pub drone_cost: u32,
    /// Drones a ship of tier 1, 2 and 3 can have.
    pub drone_slots: [u8; TIERS as usize],
    /// How far from its owner a drone looks for rocks.
    pub drone_reach: f32,
    pub drone_speed: f32,
    /// Ticks between a drone's shots.
    pub drone_cooldown: u16,
    pub drone_damage: f32,
    pub rock_speed: f32,
    pub crash_damage: f32,

    pub crystal_ticks: u16,
    pub pickup_radius: f32,

    pub upgrade_cost: u32,

    pub respawn_ticks: u16,
    pub kill_score: u32,
    /// How far a ship sees clearly: everything in this range is observed in detail, enough to aim.
    pub view_range: f32,
    /// How far a ship's long-range radar reaches: it shows where enemy ships are and in which
    /// directions the rocks lie, without any detail.
    pub radar_range: f32,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            radius: 1200.0,
            dt: 1.0 / 30.0,
            teams: 3,
            ships_per_team: 7,
            rock_count: 270,
            max_ticks: 36000,
            ship_radius: 12.0,
            ship_radii: [12.0, 15.0, 19.0],
            ram_damage: 0.1,
            plan_ticks: 30,
            thrust_accel: 130.0,
            drag: 0.6,
            max_speed: 150.0,
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
                Weapon { lasers: 1, fan: 0.0, range: 470.0, speed: 325.0, cooldown: 8, damage: 1.0 },
                Weapon { lasers: 1, fan: 0.0, range: 800.0, speed: 500.0, cooldown: 14, damage: 3.0 },
                Weapon { lasers: 2, fan: 0.0, range: 500.0, speed: 350.0, cooldown: 8, damage: 1.1 },
                Weapon { lasers: 3, fan: 0.14, range: 300.0, speed: 300.0, cooldown: 5, damage: 0.8 },
                Weapon { lasers: 1, fan: 0.0, range: 260.0, speed: 300.0, cooldown: 10, damage: 0.8 },
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
            ammo_damage: 1.0,
            boost_thrust: 2.5,
            boost_speed: 1.6,
            boost_drain: 0.5,
            boost_regen: 0.1,
            boost_cell_chance: 0.15,
            lives: 3,
            crystal_armor: 1.0,
            rock_radius: 18.0,
            rock_hp: 10.0,
            rock_packs: [(8, 15), (16, 40), (41, 70), (71, 110), (111, 150)],
            rock_weights: [35, 28, 20, 11, 6],
            base_levels: 7,
            base_level_at: [1000, 2500, 4500, 7000, 10000, 14000],
            base_hp_per_level: 200.0,
            turret_per_level: 0.15,
            deposit_rate: 5,
            repair_step: 10,
            drone_cost: 40,
            drone_slots: [1, 2, 3],
            drone_reach: 240.0,
            drone_speed: 170.0,
            drone_cooldown: 10,
            drone_damage: 1.0,
            rock_speed: 12.0,
            crash_damage: 0.15,
            crystal_ticks: 600,
            pickup_radius: 26.0,
            upgrade_cost: 10,
            respawn_ticks: 90,
            kill_score: 20,
            view_range: 600.0,
            radar_range: 1500.0,
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
    /// Ammo one volley costs: every laser's damage, at [`Config::ammo_damage`] damage per unit.
    pub fn volley_cost(&self, s: &Ship) -> f32 {
        self.weapon(s.class).lasers as f32 * self.damage(s) / self.ammo_damage
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
    /// Price in points of what a ship wants to buy, or None when it cannot be bought at all.
    /// `base` is the ship's own base and `drones` how many drones it already has.
    pub fn price(&self, s: &Ship, buy: Buy, base: &Base, drones: usize) -> Option<u32> {
        match buy {
            Buy::Upgrade(u) => self.cost(s.levels[u as usize], s.tier),
            // Leaving tier 1 needs a real class; after that the class named is ignored. A ship
            // cannot outgrow its base: tier 2 needs a level-2 base, tier 3 a level-3 one.
            Buy::Tier(class) => (s.tier < TIERS && base.level > s.tier && (s.tier > 1 || class != Class::Basic)).then(|| self.tier_cost[s.tier as usize - 1]),
            Buy::Drone => (drones < self.drone_slots[s.tier as usize - 1] as usize).then_some(self.drone_cost),
            Buy::Repair => (base.alive() && base.hp < self.base_hp_max(base.level)).then_some(self.repair_step),
        }
    }
    /// Hit points of a base of a given level when whole.
    pub fn base_hp_max(&self, level: u8) -> f32 {
        self.base_hp + self.base_hp_per_level * (level.max(1) - 1) as f32
    }
    /// Crystals a base of this level needs in total to reach the next one, or None at the top.
    pub fn next_base_level_at(&self, level: u8) -> Option<u32> {
        (level < self.base_levels).then(|| self.base_level_at[level as usize - 1])
    }
    /// How big a pack of crystals is: bigger packs are bigger targets. A boost cell (value 0) is the smallest.
    pub fn crystal_radius(&self, value: u32) -> f32 {
        8.0 + (value as f32).sqrt()
    }
    /// How close a ship of the given radius (0 for a drone) has to come to a pack to take it:
    /// its own radius plus twice the pack's.
    pub fn pickup_reach(&self, ship_radius: f32, value: u32) -> f32 {
        ship_radius + 2.0 * self.crystal_radius(value)
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
    /// Crystals on board. They buy nothing until handed over at the team's base.
    pub cargo: u32,
    /// Points to spend: one for every crystal the ship has handed over at its base.
    pub points: u32,
    /// Crystals the ship has handed over in all.
    pub deposited: u32,
    pub levels: [u8; 8],
    /// The ship's mode; None until it picks one.
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
    /// How many crystals are in this pack; a ship without room for all of it takes what fits.
    pub value: u32,
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
    /// 1 to [`Config::base_levels`]. A higher level means more hit points, a stronger turret,
    /// and higher ship tiers for the team.
    pub level: u8,
    /// Crystals the team has handed over in all; enough of them raise the level.
    pub deposited: u32,
}

/// A mining helper. It follows its owner, breaks rocks near it and gathers crystals for it.
/// When its owner is destroyed it drifts until any ship with room for it flies up and takes it.
#[derive(Clone, Debug)]
pub struct Drone {
    pub pos: Vec3,
    pub owner: Option<u16>,
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
    /// A ship's shot (or its drone's) hit a rock.
    RockHit { by: u16, size: u8, damage: f32 },
    /// A ship (or its drone) picked up this many crystals.
    Collected { ship: u16, amount: u32 },
    Bought { ship: u16, what: Buy },
    BaseHit { by: u16, team: u8, damage: f32 },
    BaseDestroyed { by: u16, team: u8 },
    /// A ship handed crystals over at its base.
    Deposited { ship: u16, amount: u32 },
    /// A team's base reached a new level.
    BaseLevel { team: u8, level: u8 },
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
    pub drones: Vec<Drone>,
    /// One per team, indexed by team.
    pub bases: Vec<Base>,
    /// The mode most of each team's ships are in, indexed by team; None until one picks a mode.
    pub plans: Vec<Option<Plan>>,
    /// Fog of war: the health each team last saw each base at, indexed by team, then by base.
    /// A base is seen while a living ship of the team is within view range of it.
    pub seen_base_hp: Vec<Vec<f32>>,
    /// Teams whose orders come from outside (a human commander): their votes are not counted.
    pub commanded: Vec<bool>,
    pub events: Vec<Event>,
    rng: Rng,
    /// Scratch space reused every tick: where the rocks and the ships are, laid out for fast first-pass tests.
    rock_field: geo::Field,
    ship_field: geo::Field,
}


impl World {
    pub fn new(cfg: Config, seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        // Evenly spaced around the equator, so every team sees the same layout
        // from its own base and none starts closer to its enemies.
        let bases: Vec<Base> = (0..cfg.teams)
            .map(|t| Base { pos: Vec3::from_lon_lat(std::f32::consts::TAU * t as f32 / cfg.teams as f32, 0.0), hp: cfg.base_hp, cooldown: 0, level: 1, deposited: 0 })
            .collect();
        let mut world = World {
            tick: 0,
            ships: Vec::new(),
            rocks: Vec::new(),
            crystals: Vec::new(),
            shots: Vec::new(),
            drones: Vec::new(),
            plans: vec![None; bases.len()],
            seen_base_hp: vec![vec![cfg.base_hp; bases.len()]; bases.len()],
            commanded: vec![false; bases.len()],
            bases,
            events: Vec::new(),
            rock_field: geo::Field::default(),
            ship_field: geo::Field::default(),
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
                    points: 0,
                    deposited: 0,
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

    /// Turns the ships' modes into orders. Farming is the default and defending is a ship's own
    /// choice; when at least half of a team's living ships pick an attack, the whole team
    /// attacks the base most of them picked. Teams commanded from outside, and teams where no
    /// ship has ever picked a mode (scripted ones), keep the orders they have.
    fn count_votes(&mut self) {
        for team in 0..self.bases.len() {
            if self.commanded[team] || !self.ships.iter().any(|s| s.team as usize == team && s.vote.is_some()) {
                continue;
            }
            let mut tally = [0u32; PLANS.len()];
            let mut middle = Vec3::default();
            let mut living = 0;
            for s in self.ships.iter().filter(|s| s.team as usize == team && s.alive) {
                middle = middle.add(s.pos);
                living += 1;
                tally[s.vote.unwrap_or(Plan::Farm) as usize] += 1;
            }
            let (a, b) = (tally[Plan::AttackA as usize], tally[Plan::AttackB as usize]);
            let attack = (living > 0 && (a + b) * 2 >= living).then(|| if b > a { Plan::AttackB } else { Plan::AttackA });
            let most = *tally.iter().max().unwrap_or(&0);
            self.plans[team] = attack.or_else(|| (most > 0).then(|| PLANS[tally.iter().position(|n| *n == most).unwrap()]));
            let home = self.bases[team].pos;
            // Enemy bases this team believes are still standing, counting teams on from this one.
            let enemies: Vec<Vec3> = (1..self.bases.len())
                .map(|k| (team + k) % self.bases.len())
                .filter(|b| self.seen_base_hp[team][*b] > 0.0)
                .map(|b| self.bases[b].pos)
                .collect();
            let farm = Order { kind: OrderKind::Mine, target: if middle.len() > 1e-3 { middle.unit() } else { home } };
            let order_for = |mode: Plan| match (attack, mode) {
                (Some(Plan::AttackB), _) => Order { kind: OrderKind::Attack, target: enemies.get(1).or(enemies.first()).copied().unwrap_or(home) },
                (Some(_), _) => Order { kind: OrderKind::Attack, target: enemies.first().copied().unwrap_or(home) },
                (None, Plan::Defend) => Order { kind: OrderKind::Defend, target: home },
                // Without the team behind it, a ship that wants to attack keeps farming.
                (None, _) => farm,
            };
            for s in self.ships.iter_mut().filter(|s| s.team as usize == team) {
                s.order = order_for(s.vote.unwrap_or(Plan::Farm));
            }
        }
    }

    /// Fog of war: each team's knowledge of a base's health is refreshed while one of its
    /// living ships is within view range of the base. A team always knows its own base.
    fn look_at_bases(&mut self) {
        for team in 0..self.bases.len() {
            for (b, base) in self.bases.iter().enumerate() {
                let in_sight = b == team
                    || self.ships.iter().any(|s| s.team as usize == team && s.alive && geo::chord(self.cfg.radius, s.pos, base.pos) < self.cfg.view_range);
                if in_sight {
                    self.seen_base_hp[team][b] = base.hp.max(0.0);
                }
            }
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
        // Small rocks are common, the biggest rare.
        let mut pick = rng.below(cfg.rock_weights.iter().sum());
        let size = 1 + cfg.rock_weights.iter().position(|w| if pick < *w { true } else { pick -= *w; false }).unwrap_or(0) as u8;
        let vel = geo::bearing(pos, rng.range(0.0, std::f32::consts::TAU)).scale(rng.range(0.0, cfg.rock_speed));
        Rock { pos, vel, size, hp: cfg.rock_hp_of(size), last_hit_by: None }
    }

    /// Drops one pack of crystals between `near` and `far` from a point. Nothing for an empty pack.
    fn drop_pack(&mut self, at: Vec3, value: u32, near: f32, far: f32) {
        if value == 0 {
            return;
        }
        let away = geo::bearing(at, self.rng.range(0.0, std::f32::consts::TAU));
        let pos = geo::travel(self.cfg.radius, at, away, self.rng.range(near, far));
        self.crystals.push(Crystal { pos, ticks: self.cfg.crystal_ticks, value, boost_cell: false });
    }

    /// Advances the match by one tick. `actions[i]` drives `ships[i]`.
    pub fn step(&mut self, actions: &[Action]) -> &[Event] {
        assert_eq!(actions.len(), self.ships.len(), "one action per ship");
        self.events.clear();
        self.tick += 1;
        let dt = self.cfg.dt;
        let radius = self.cfg.radius;

        // Modes: a ship keeps its mode until it picks another; every so often every ship's
        // mode is turned into a fresh order, since the places they point at move.
        for (ship, act) in self.ships.iter_mut().zip(actions) {
            if act.vote.is_some() {
                ship.vote = act.vote;
            }
        }
        if self.tick % self.cfg.plan_ticks == 0 {
            self.count_votes();
        }

        // Teams whose ships paid for a repair this tick.
        let mut repairs: Vec<u8> = Vec::new();

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
                let drones = self.drones.iter().filter(|d| d.owner == Some(i as u16)).count();
                if let Some(cost) = cfg.price(ship, what, home, drones).filter(|cost| ship.points >= *cost) {
                    ship.points -= cost;
                    match what {
                        Buy::Drone => self.drones.push(Drone { pos: ship.pos, owner: Some(i as u16), cooldown: 0 }),
                        Buy::Repair => repairs.push(ship.team),
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
            let cost = cfg.volley_cost(ship);
            if act.fire && ship.cooldown == 0 && ship.ammo >= cost {
                ship.cooldown = cfg.cooldown(ship);
                ship.ammo -= cost;
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

        for team in repairs {
            let base = &mut self.bases[team as usize];
            base.hp = (base.hp + self.cfg.repair_step as f32).min(self.cfg.base_hp_max(base.level));
        }

        // Ships next to their own base hand over the crystals they carry: every crystal is a
        // point for the ship and counts towards the base's next level.
        for i in 0..self.ships.len() {
            let ship = &mut self.ships[i];
            let base = &mut self.bases[ship.team as usize];
            if !ship.alive || ship.cargo == 0 || !base.alive() || geo::chord(radius, base.pos, ship.pos) > self.cfg.base_heal_radius {
                continue;
            }
            let amount = ship.cargo.min(self.cfg.deposit_rate);
            ship.cargo -= amount;
            ship.points += amount;
            ship.deposited += amount;
            base.deposited += amount;
            self.events.push(Event::Deposited { ship: i as u16, amount });
            while self.cfg.next_base_level_at(base.level).is_some_and(|at| base.deposited >= at) {
                base.level += 1;
                // The new hit points come with the level.
                base.hp += self.cfg.base_hp_per_level;
                self.events.push(Event::BaseLevel { team: ship.team, level: base.level });
            }
        }

        // Drones: an orphan is taken by the first ship with room that flies up to it; an owned one
        // goes for the nearest rock near its owner, or trails its owner, and shoots rocks in range.
        for d in 0..self.drones.len() {
            let drone = &self.drones[d];
            let Some(owner) = drone.owner.filter(|o| self.ships[*o as usize].alive) else {
                let taker = self.ships.iter().enumerate().position(|(i, s)| {
                    s.alive
                        && geo::chord(radius, s.pos, drone.pos) < self.cfg.pickup_radius
                        && self.drones.iter().filter(|x| x.owner == Some(i as u16)).count() < self.cfg.drone_slots[s.tier as usize - 1] as usize
                });
                self.drones[d].owner = taker.map(|i| i as u16);
                continue;
            };
            let ship = &self.ships[owner as usize];
            let rock = self
                .rocks
                .iter()
                .filter(|r| r.hp > 0.0 && geo::closer(radius, ship.pos, r.pos, self.cfg.drone_reach))
                .min_by(|a, b| geo::chord(radius, drone.pos, a.pos).total_cmp(&geo::chord(radius, drone.pos, b.pos)))
                .map(|r| (r.pos, self.cfg.rock_radius * r.size as f32));
            // Crystals lying about come first, while the owner has room for them; then rocks; else trail the owner.
            let crystal = (ship.cargo < self.cfg.cargo_max(ship))
                .then(|| {
                    self.crystals
                        .iter()
                        .filter(|c| !c.boost_cell && geo::closer(radius, ship.pos, c.pos, self.cfg.drone_reach))
                        .min_by(|a, b| geo::chord(radius, drone.pos, a.pos).total_cmp(&geo::chord(radius, drone.pos, b.pos)))
                        .map(|c| c.pos)
                })
                .flatten();
            let (goal, keep) = match (crystal, rock) {
                (Some(pos), _) => (pos, 0.0),
                (None, Some((pos, size))) => (pos, size + 80.0),
                (None, None) => (ship.pos, 45.0),
            };
            let (team, pos) = (ship.team, drone.pos);
            let drone = &mut self.drones[d];
            drone.cooldown = drone.cooldown.saturating_sub(1);
            let way = geo::toward(radius, pos, goal);
            if way.len() > keep {
                drone.pos = geo::travel(radius, pos, way.unit(), (self.cfg.drone_speed * dt).min(way.len() - keep));
            }
            if let Some((at, _)) = rock {
                let gun = self.cfg.weapon(Class::Basic);
                let aim = geo::toward(radius, drone.pos, at);
                if drone.cooldown == 0 && aim.len() < gun.range * 0.6 {
                    drone.cooldown = self.cfg.drone_cooldown;
                    let life = self.cfg.shot_ticks(Class::Basic);
                    self.shots.push(Shot { pos: drone.pos, vel: aim.unit().scale(gun.speed), ticks: life, life, damage: self.cfg.drone_damage, owner: Some(owner), team });
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
                    let stronger = 1.0 + self.cfg.turret_per_level * (base.level - 1) as f32;
                    base.cooldown = self.cfg.turret_cooldown;
                    self.shots.push(Shot {
                        pos: geo::nudge(radius, base.pos, dir.scale(self.cfg.base_radius)),
                        vel: dir.scale(gun.speed),
                        ticks: life,
                        life,
                        damage: self.cfg.turret_damage * stronger,
                        owner: None,
                        team: team as u8,
                    });
                }
            }
        }

        // Crystals knocked out of holds this tick: where, and how many.
        let mut spilled: Vec<(Vec3, u32)> = Vec::new();

        // Nearly every shot touches no rock; a flat first pass says so without the exact search.
        self.rock_field.fill(radius, self.rocks.iter().map(|r| (r.pos, self.cfg.rock_radius * r.size as f32)));
        let rock_field = &self.rock_field;

        // Shots: move, then hit the first rock or enemy ship they touch.
        let falloff = self.cfg.shot_falloff;
        for shot in &mut self.shots {
            (shot.pos, shot.vel) = geo::advance(radius, shot.pos, shot.vel, dt);
            shot.ticks -= 1;
            let damage = shot.damage_now(falloff);
            let touched = if rock_field.any_near(radius, shot.pos, 0.0) {
                self.rocks.iter_mut().find(|r| r.hp > 0.0 && geo::chord(radius, r.pos, shot.pos) < self.cfg.rock_radius * r.size as f32)
            } else {
                None
            };
            if let Some(rock) = touched {
                rock.hp -= damage;
                rock.last_hit_by = shot.owner;
                if let Some(by) = shot.owner {
                    self.events.push(Event::RockHit { by, size: rock.size, damage });
                }
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

        // Rocks drift; ships bounce off them and take damage by impact speed. Most rocks have no
        // ship anywhere near: a flat first pass over the ships says so. Once a ship has been
        // pushed this tick the flat positions are stale, and every rock is checked properly.
        let biggest_ship = self.cfg.ship_radii.iter().copied().fold(0.0, f32::max);
        self.ship_field.fill(radius, self.ships.iter().filter(|s| s.alive).map(|s| (s.pos, biggest_ship)));
        let mut pushed = false;
        for r in 0..self.rocks.len() {
            let rock = &mut self.rocks[r];
            (rock.pos, rock.vel) = geo::advance(radius, rock.pos, rock.vel, dt);
            if !pushed && !self.ship_field.any_near(radius, rock.pos, self.cfg.rock_radius * rock.size as f32) {
                continue;
            }
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
                pushed = true;
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
            self.drop_pack(at, count, 90.0, 140.0);
        }

        // Broken rocks leave crystals and are replaced elsewhere.
        for r in 0..self.rocks.len() {
            if self.rocks[r].hp > 0.0 {
                continue;
            }
            let (pos, rock_size, by) = (self.rocks[r].pos, self.rocks[r].size, self.rocks[r].last_hit_by);
            self.events.push(Event::RockDestroyed { by, size: rock_size });
            let (least, most) = self.cfg.rock_packs[rock_size as usize - 1];
            let value = least + self.rng.below(most - least + 1);
            self.drop_pack(pos, value, 0.0, self.cfg.rock_radius);
            if self.rng.unit() < self.cfg.boost_cell_chance {
                let away = geo::bearing(pos, self.rng.range(0.0, std::f32::consts::TAU));
                let at = geo::travel(radius, pos, away, self.cfg.rock_radius * 1.5);
                self.crystals.push(Crystal { pos: at, ticks: self.cfg.crystal_ticks, value: 0, boost_cell: true });
            }
            self.rocks[r] = Self::new_rock(&self.cfg, &self.bases, &mut self.rng);
        }

        // Crystals go to the nearest ship in reach that still has room.
        let cfg = &self.cfg;
        let ships = &mut self.ships;
        let events = &mut self.events;
        let drones = &self.drones;
        self.crystals.retain_mut(|c| {
            c.ticks -= 1;
            let mut best: Option<(usize, f32)> = None;
            // A drone gathers crystals (not boost cells) for its owner, as long as the owner has room.
            if !c.boost_cell {
                for d in drones {
                    let Some(i) = d.owner.map(|o| o as usize) else { continue };
                    let dist = geo::chord(radius, d.pos, c.pos);
                    if ships[i].alive && ships[i].cargo < cfg.cargo_max(&ships[i]) && dist < cfg.pickup_reach(0.0, c.value) && best.is_none_or(|(_, b)| dist < b) {
                        best = Some((i, dist));
                    }
                }
            }
            for (i, s) in ships.iter().enumerate() {
                // A boost cell is worth picking up until the tank is full; a crystal needs cargo room.
                let wanted = if c.boost_cell { s.boost < 1.0 } else { s.cargo < cfg.cargo_max(s) };
                if !s.alive || !wanted {
                    continue;
                }
                let dist = geo::chord(radius, s.pos, c.pos);
                if dist < cfg.pickup_reach(cfg.radius_of(s), c.value) && best.is_none_or(|(_, b)| dist < b) {
                    best = Some((i, dist));
                }
            }
            if let Some((i, _)) = best {
                if c.boost_cell {
                    ships[i].boost = 1.0;
                    return false;
                }
                // Take what fits; the rest of the pack stays where it is.
                let amount = c.value.min(cfg.cargo_max(&ships[i]) - ships[i].cargo);
                ships[i].cargo += amount;
                ships[i].collected += amount;
                ships[i].score += amount;
                c.value -= amount;
                events.push(Event::Collected { ship: i as u16, amount });
                return c.value > 0 && c.ticks > 0;
            }
            c.ticks > 0
        });

        // Deaths: drop part of the cargo, credit the killer, start the respawn timer.
        for i in 0..self.ships.len() {
            if !self.ships[i].alive || self.ships[i].hull > 0.0 {
                continue;
            }
            // Half of what was on board is left behind as one pack; the rest is lost.
            let (pos, dropped, by) = (self.ships[i].pos, self.ships[i].cargo.div_ceil(2), self.ships[i].last_hit_by);
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
            self.drop_pack(pos, dropped, 0.0, 30.0);
        }

        self.look_at_bases();
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
            put(s.points);
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
            put(b.level as u32);
            put(b.deposited);
        }
        put(self.drones.len() as u32);
        for d in &self.drones {
            spot(d.pos).into_iter().for_each(&mut put);
            put(d.owner.map_or(u32::MAX, u32::from));
        }
        put(self.crystals.len() as u32);
        for c in &self.crystals {
            spot(c.pos).into_iter().for_each(&mut put);
            put(c.value);
        }
        put(self.shots.len() as u32);
        for s in &self.shots {
            spot(s.pos).into_iter().for_each(&mut put);
        }
        h
    }
}
