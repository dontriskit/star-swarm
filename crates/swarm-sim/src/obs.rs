use crate::geo::{self, Vec3};
use crate::sim::{CLASSES, Class, Config, ORDER_KINDS, OrderKind, PLANS, Plan, ROCK_SIZES, TIERS, UPGRADES, Vec2, World};

/// How many of each kind of nearby object a ship sees.
pub const NEAR: usize = 6;
/// How many incoming enemy shots a ship sees.
pub const SHOTS: usize = 4;
/// How many enemy bases a ship knows about (the nearest ones still standing).
pub const BASES: usize = 2;
const SELF_LEN: usize = 11 + 4 + 3 + CLASSES.len() + UPGRADES.len() + 4 + ORDER_KINDS + 2 + 2 * PLANS.len() + 2;
const BASE_LEN: usize = 4;
/// How many teammates a ship's team radar shows (the nearest ones).
pub const MATES: usize = 6;
const MATE_LEN: usize = 4 + PLANS.len();
/// How many enemies spotted by teammates, out of the ship's own sight, its radar shows.
pub const CONTACTS: usize = 4;
const CONTACT_LEN: usize = 3;
const NEAR_LEN: usize = 10;
/// Length of [`Observation::to_vec`].
pub const OBS_LEN: usize = SELF_LEN + BASES * BASE_LEN + MATES * MATE_LEN + CONTACTS * CONTACT_LEN + (3 * NEAR + SHOTS) * NEAR_LEN;

/// A nearby object in the observing ship's frame: x forward, y to the left,
/// world units.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Near {
    pub pos: Vec2,
    pub vel: Vec2,
    pub dist: f32,
    pub radius: f32,
    /// Remaining health as a fraction of the biggest rock's or of the ship's own; for a crystal
    /// pack, its size as a fraction of the biggest pack.
    pub hp: f32,
    /// An enemy ship or shot. Among crystals it marks a boost cell.
    pub enemy: bool,
    /// Where another ship is pointing, as a unit vector in this ship's frame;
    /// zero for everything that is not a ship.
    pub facing: Vec2,
}

/// A teammate on the team radar: teammates are known wherever they are.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MateObs {
    /// In the ship's frame, world units.
    pub pos: Vec2,
    pub dist: f32,
    /// Remaining hull as a fraction.
    pub hull: f32,
    /// What the teammate has set out to do.
    pub mode: Option<Plan>,
}

/// A base as a ship knows it. Where bases are is known map-wide; an enemy base's health is
/// what the ship's team last saw (fog of war).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BaseObs {
    /// In the ship's frame, world units.
    pub pos: Vec2,
    pub dist: f32,
    /// Remaining health as a fraction of a new base.
    pub hp: f32,
}

/// Everything one ship knows. Every controller gets exactly this: a human's
/// GUI, a scripted bot, a trained policy (through [`Observation::to_vec`]).
#[derive(Clone, Debug, Default)]
pub struct Observation {
    pub alive: bool,
    pub hull: f32,
    pub hull_max: f32,
    pub shield: f32,
    pub shield_max: f32,
    pub cargo: u32,
    pub cargo_max: u32,
    pub vel: Vec2,
    /// The guns have cooled down and there is ammo for a volley.
    pub ready: bool,
    pub tier: u8,
    pub class: Class,
    /// Deaths left before the ship is demoted.
    pub lives: u8,
    pub ammo: f32,
    pub ammo_max: f32,
    /// Boost tank, 0 to 1.
    pub boost: f32,
    /// Points to spend, earned by handing crystals over at the base.
    pub points: u32,
    /// Drones the ship has.
    pub drones: u8,
    /// The level of the ship's own base.
    pub base_level: u8,
    /// How far the team is towards its base's next level, 0 to 1 (1 at the top level).
    pub base_progress: f32,
    /// The nearest ownerless drone in sight, in the ship's frame.
    pub free_drone: Option<Vec2>,
    pub levels: [u8; 8],
    /// The ship's own base; `hp` is 0 once it has been destroyed.
    pub base: BaseObs,
    pub order: OrderKind,
    pub target: Vec2,
    /// The ship's own mode.
    pub vote: Option<Plan>,
    /// Share of the team's living ships in each mode, indexed by [`Plan`].
    pub votes: [f32; 4],
    /// The middle of the ship's living teammates, in its own frame; zero when it has none.
    pub team: Vec2,
    /// Enemy bases the team believes are still standing, nearest first, at most [`BASES`].
    pub enemy_bases: List<BaseObs, BASES>,
    /// Team radar: living teammates, nearest first, at most [`MATES`].
    pub mates: List<MateObs, MATES>,
    /// Team radar: enemies some teammate can see but this ship cannot, in its own frame,
    /// nearest first, at most [`CONTACTS`].
    pub contacts: List<Vec2, CONTACTS>,
    /// Nearest first, at most [`NEAR`] each, within the view range.
    pub rocks: List<Near, NEAR>,
    pub crystals: List<Near, NEAR>,
    pub ships: List<Near, NEAR>,
    /// Enemy shots, nearest first, at most [`SHOTS`].
    pub shots: List<Near, SHOTS>,
}

/// Up to `N` items kept in order of a key, smallest first, without allocating. Reads like a slice.
#[derive(Clone, Copy, Debug)]
pub struct List<T: Copy + Default, const N: usize> {
    items: [T; N],
    keys: [f32; N],
    len: usize,
}

impl<T: Copy + Default, const N: usize> Default for List<T, N> {
    fn default() -> Self {
        List { items: [T::default(); N], keys: [0.0; N], len: 0 }
    }
}

impl<T: Copy + Default, const N: usize> std::ops::Deref for List<T, N> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        &self.items[..self.len]
    }
}

impl<T: Copy + Default, const N: usize> List<T, N> {
    /// Adds an item if it is among the `N` smallest keys seen so far. Items with equal keys
    /// stay in the order they were offered, as a stable sort would leave them.
    fn offer(&mut self, item: T, key: f32) {
        let at = self.keys[..self.len].iter().take_while(|k| k.total_cmp(&key).is_le()).count();
        if at >= N {
            return;
        }
        let last = self.len.min(N - 1);
        self.items.copy_within(at..last, at + 1);
        self.keys.copy_within(at..last, at + 1);
        self.items[at] = item;
        self.keys[at] = key;
        self.len = (self.len + 1).min(N);
    }
}

/// What can be worked out once for a whole world and shared by every ship's observation:
/// how far apart the ships are, who is alive on each team and what they voted, who has drones.
pub struct Scan<'a> {
    world: &'a World,
    /// Straight-line distance between every pair of ships, row by row.
    apart: Vec<f32>,
    /// Living ships of each team, in ship order.
    living: Vec<Vec<usize>>,
    /// How many living ships of each team are in each mode.
    tally: Vec<[u32; 4]>,
    /// Drones each ship owns.
    drones: Vec<u8>,
    /// Where the ownerless drones are.
    adrift: Vec<Vec3>,
    /// Squared straight-line view range on the unit sphere, with a little slack either way,
    /// for telling near from far without a square root in all but borderline cases.
    surely_far: f32,
    surely_near: f32,
}

impl<'a> Scan<'a> {
    pub fn new(world: &'a World) -> Self {
        let cfg = &world.cfg;
        let n = world.ships.len();
        let mut apart = vec![0.0; n * n];
        for i in 0..n {
            for j in i + 1..n {
                let d = geo::chord(cfg.radius, world.ships[i].pos, world.ships[j].pos);
                apart[i * n + j] = d;
                apart[j * n + i] = d;
            }
        }
        let teams = world.bases.len();
        let mut living = vec![Vec::new(); teams];
        let mut tally = vec![[0u32; 4]; teams];
        for (i, s) in world.ships.iter().enumerate().filter(|(_, s)| s.alive) {
            living[s.team as usize].push(i);
            if let Some(mode) = s.vote {
                tally[s.team as usize][mode as usize] += 1;
            }
        }
        let mut drones = vec![0u8; n];
        let mut adrift = Vec::new();
        for d in &world.drones {
            match d.owner {
                Some(owner) => drones[owner as usize] += 1,
                None => adrift.push(d.pos),
            }
        }
        let unit_view = cfg.view_range / cfg.radius;
        Scan { world, apart, living, tally, drones, adrift, surely_far: unit_view * unit_view * 1.001, surely_near: unit_view * unit_view * 0.999 }
    }

    /// Whether `pos` is farther from `from` than the view range, in a straight line. The same
    /// answer as comparing [`geo::chord`] with the view range, usually without the square root.
    fn out_of_sight(&self, from: Vec3, pos: Vec3) -> bool {
        let d = from.sub(pos);
        let squared = d.dot(d);
        if squared > self.surely_far {
            true
        } else if squared < self.surely_near {
            false
        } else {
            geo::chord(self.world.cfg.radius, from, pos) > self.world.cfg.view_range
        }
    }

    pub fn observe(&self, ship: usize) -> Observation {
        let world = self.world;
        let cfg = &world.cfg;
        let n = world.ships.len();
        let me = &world.ships[ship];
        let home = &world.bases[me.team as usize];
        // Everything is expressed in this ship's own frame: x forward, y to the left.
        let local = |v: Vec3| geo::frame(me.pos, me.fwd, geo::tangent(me.pos, v));
        // `far` says the object is beyond the view range in a straight line, which is cheap to
        // tell and never longer than the way along the surface.
        let see = |far: bool, pos: Vec3, vel: Vec3, radius: f32, hp: f32, enemy: bool, fwd: Option<Vec3>| {
            if far {
                return None;
            }
            let at = world.to_frame(me.pos, me.fwd, pos);
            let dist = at.len();
            (dist <= cfg.view_range).then(|| Near {
                pos: at,
                vel: local(vel.sub(me.vel)),
                dist,
                radius,
                hp,
                enemy,
                facing: fwd.map_or(Vec2::default(), local),
            })
        };
        // What the team last saw of each base, not necessarily how it is now.
        let known = &world.seen_base_hp[me.team as usize];
        let base = |team: usize| {
            let at = world.to_frame(me.pos, me.fwd, world.bases[team].pos);
            // Against the hit points of a new base, so a levelled-up base reads above 1.
            BaseObs { pos: at, dist: at.len(), hp: known[team] / cfg.base_hp }
        };
        let mut enemy_bases = List::default();
        for b in (0..world.bases.len()).filter(|t| *t != me.team as usize && known[*t] > 0.0).map(base) {
            enemy_bases.offer(b, b.dist);
        }
        let biggest_pack = cfg.rock_packs[ROCK_SIZES as usize - 1].1 as f32;

        // Teammates: the living ships of this team other than this one, in ship order.
        let team = &self.living[me.team as usize];
        let mates = || team.iter().copied().filter(|i| *i != ship);
        let voters = team.len();
        let tally = &self.tally[me.team as usize];
        let middle = mates().fold(Vec3::default(), |sum, i| sum.add(world.ships[i].pos));
        let mut radar = List::default();
        for i in mates() {
            let s = &world.ships[i];
            let at = world.to_frame(me.pos, me.fwd, s.pos);
            radar.offer(MateObs { pos: at, dist: at.len(), hull: s.hull.max(0.0) / cfg.hull_max(s), mode: s.vote }, at.len());
        }
        // Enemies in a teammate's sight but not in this ship's own.
        let mut contacts = List::default();
        for (e, enemy) in world.ships.iter().enumerate() {
            if !enemy.alive || enemy.team == me.team || !(self.apart[ship * n + e] > cfg.view_range) {
                continue;
            }
            if mates().any(|m| self.apart[m * n + e] < cfg.view_range) {
                let at = world.to_frame(me.pos, me.fwd, enemy.pos);
                contacts.offer(at, at.len());
            }
        }

        let mut rocks = List::default();
        for r in &world.rocks {
            if let Some(seen) = see(self.out_of_sight(me.pos, r.pos), r.pos, r.vel, cfg.rock_radius_of(r.size), r.hp / cfg.rock_hp_of(ROCK_SIZES), false, None) {
                rocks.offer(seen, seen.dist);
            }
        }
        let mut crystals = List::default();
        for c in &world.crystals {
            if let Some(seen) = see(self.out_of_sight(me.pos, c.pos), c.pos, Vec3::default(), 0.0, c.value as f32 / biggest_pack, c.boost_cell, None) {
                crystals.offer(seen, seen.dist);
            }
        }
        let mut ships = List::default();
        for (i, s) in world.ships.iter().enumerate() {
            if i == ship || !s.alive {
                continue;
            }
            let far = self.apart[ship * n + i] > cfg.view_range;
            if let Some(seen) = see(far, s.pos, s.vel, cfg.radius_of(s), s.hull / cfg.hull_max(s), s.team != me.team, Some(s.fwd)) {
                ships.offer(seen, seen.dist);
            }
        }
        let mut shots = List::default();
        for s in world.shots.iter().filter(|s| s.team != me.team) {
            if let Some(seen) = see(self.out_of_sight(me.pos, s.pos), s.pos, s.vel, 0.0, s.damage_now(cfg.shot_falloff) / cfg.hull, true, None) {
                shots.offer(seen, seen.dist);
            }
        }

        Observation {
            alive: me.alive,
            hull: me.hull.max(0.0),
            hull_max: cfg.hull_max(me),
            shield: me.shield,
            shield_max: cfg.shield_max(me),
            cargo: me.cargo,
            cargo_max: cfg.cargo_max(me),
            vel: local(me.vel),
            ready: me.cooldown == 0 && me.ammo >= cfg.volley_cost(me),
            tier: me.tier,
            class: me.class,
            lives: me.lives,
            ammo: me.ammo,
            ammo_max: cfg.ammo_max(me),
            boost: me.boost,
            points: me.points,
            drones: self.drones[ship],
            base_level: home.level,
            base_progress: match cfg.next_base_level_at(home.level) {
                Some(next) => {
                    let from = if home.level > 1 { cfg.base_level_at[home.level as usize - 2] } else { 0 };
                    (home.deposited.saturating_sub(from) as f32 / (next - from).max(1) as f32).min(1.0)
                }
                None => 1.0,
            },
            free_drone: self
                .adrift
                .iter()
                .filter(|d| geo::chord(cfg.radius, me.pos, **d) < cfg.view_range)
                .map(|d| world.to_frame(me.pos, me.fwd, *d))
                .min_by(|a, b| a.len().total_cmp(&b.len())),
            levels: me.levels,
            base: base(me.team as usize),
            order: me.order.kind,
            target: world.to_frame(me.pos, me.fwd, me.order.target),
            vote: me.vote,
            votes: PLANS.map(|p| if voters > 0 { tally[p as usize] as f32 / voters as f32 } else { 0.0 }),
            team: if middle.len() > 1e-3 { world.to_frame(me.pos, me.fwd, middle.unit()) } else { Vec2::default() },
            enemy_bases,
            mates: radar,
            contacts,
            rocks,
            crystals,
            ships,
            shots,
        }
    }
}

/// One ship's observation. To observe many ships of the same world, build one [`Scan`] and
/// ask it for each: the work they share is then done once.
pub fn observe(world: &World, ship: usize) -> Observation {
    Scan::new(world).observe(ship)
}

impl Observation {
    /// Fixed-length, roughly unit-scaled vector for a neural network.
    /// Missing neighbours are zero rows with the first element (present) at 0.
    pub fn to_vec(&self, cfg: &Config) -> Vec<f32> {
        let mut v = vec![0.0; OBS_LEN];
        self.fill(cfg, &mut v);
        v
    }

    /// Writes the same vector as [`Observation::to_vec`] into `out`, which must be [`OBS_LEN`] long.
    pub fn fill(&self, cfg: &Config, out: &mut [f32]) {
        assert_eq!(out.len(), OBS_LEN);
        // Nothing is farther away than half-way around the planet.
        let (view, half, speed) = (cfg.view_range, cfg.radius * std::f32::consts::PI, cfg.max_speed);
        let mut at = 0;
        let mut put = |values: &[f32]| {
            out[at..at + values.len()].copy_from_slice(values);
            at += values.len();
        };
        put(&[
            self.alive as u8 as f32,
            self.hull / self.hull_max,
            self.shield / self.shield_max,
            self.cargo as f32 / self.cargo_max as f32,
            self.vel.x / speed,
            self.vel.y / speed,
            self.ready as u8 as f32,
            (self.tier - 1) as f32 / (TIERS - 1) as f32,
            self.ammo / self.ammo_max,
            self.boost,
            self.lives as f32 / cfg.lives.max(1) as f32,
        ]);
        // A ship that could afford anything it wants has "plenty"; beyond that the number says little.
        put(&[(self.points as f32 / 200.0).min(2.0), self.drones as f32 / 3.0, self.base_level as f32 / cfg.base_levels as f32, self.base_progress]);
        match self.free_drone {
            Some(d) => put(&[1.0, d.x / view, d.y / view]),
            None => put(&[0.0; 3]),
        }
        let mut class = [0.0; CLASSES.len()];
        class[self.class as usize] = 1.0;
        put(&class);
        put(&self.levels.map(|l| l as f32 / cfg.top_level() as f32));
        put(&[self.base.pos.x / half, self.base.pos.y / half, self.base.hp, (self.base.hp > 0.0) as u8 as f32]);
        let mut order = [0.0; ORDER_KINDS];
        order[self.order as usize] = 1.0;
        put(&order);
        put(&[self.target.x / half, self.target.y / half]);
        let mut vote = [0.0; PLANS.len()];
        if let Some(p) = self.vote {
            vote[p as usize] = 1.0;
        }
        put(&vote);
        put(&self.votes);
        put(&[self.team.x / half, self.team.y / half]);
        for slot in 0..BASES {
            match self.enemy_bases.get(slot) {
                Some(b) => put(&[1.0, b.pos.x / half, b.pos.y / half, b.hp]),
                None => put(&[0.0; BASE_LEN]),
            }
        }
        for slot in 0..MATES {
            match self.mates.get(slot) {
                Some(m) => {
                    let mut row = [0.0; MATE_LEN];
                    row[..4].copy_from_slice(&[1.0, m.pos.x / half, m.pos.y / half, m.hull]);
                    if let Some(mode) = m.mode {
                        row[4 + mode as usize] = 1.0;
                    }
                    put(&row);
                }
                None => put(&[0.0; MATE_LEN]),
            }
        }
        for slot in 0..CONTACTS {
            match self.contacts.get(slot) {
                Some(c) => put(&[1.0, c.x / half, c.y / half]),
                None => put(&[0.0; CONTACT_LEN]),
            }
        }
        for (group, slots) in [(&self.rocks[..], NEAR), (&self.crystals[..], NEAR), (&self.ships[..], NEAR), (&self.shots[..], SHOTS)] {
            for slot in 0..slots {
                match group.get(slot) {
                    Some(n) => put(&[
                        1.0,
                        n.pos.x / view,
                        n.pos.y / view,
                        n.vel.x / speed,
                        n.vel.y / speed,
                        n.radius / cfg.rock_radius_of(ROCK_SIZES),
                        n.hp,
                        n.enemy as u8 as f32,
                        n.facing.x,
                        n.facing.y,
                    ]),
                    None => put(&[0.0; NEAR_LEN]),
                }
            }
        }
        debug_assert_eq!(at, OBS_LEN);
    }
}
