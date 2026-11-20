use crate::geo::{self, Vec3};
use crate::sim::{CLASSES, Class, Config, ORDER_KINDS, OrderKind, PLANS, Plan, TIERS, UPGRADES, Vec2, World};

/// How many of each kind of nearby object a ship sees.
pub const NEAR: usize = 6;
/// How many incoming enemy shots a ship sees.
pub const SHOTS: usize = 4;
/// How many enemy bases a ship knows about (the nearest ones still standing).
pub const BASES: usize = 2;
const SELF_LEN: usize = 11 + CLASSES.len() + UPGRADES.len() + 4 + ORDER_KINDS + 2 + 2 * PLANS.len() + 2;
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
    /// Remaining health as a fraction; for crystals 0, or 1 when it is a boost cell.
    pub hp: f32,
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
    pub enemy_bases: Vec<BaseObs>,
    /// Team radar: living teammates, nearest first, at most [`MATES`].
    pub mates: Vec<MateObs>,
    /// Team radar: enemies some teammate can see but this ship cannot, in its own frame,
    /// nearest first, at most [`CONTACTS`].
    pub contacts: Vec<Vec2>,
    /// Nearest first, at most [`NEAR`] each, within the view range.
    pub rocks: Vec<Near>,
    pub crystals: Vec<Near>,
    pub ships: Vec<Near>,
    /// Enemy shots, nearest first, at most [`SHOTS`].
    pub shots: Vec<Near>,
}

fn nearest(mut all: Vec<Near>, keep: usize) -> Vec<Near> {
    all.sort_by(|a, b| a.dist.total_cmp(&b.dist));
    all.truncate(keep);
    all
}

pub fn observe(world: &World, ship: usize) -> Observation {
    let cfg = &world.cfg;
    let me = &world.ships[ship];
    // Everything is expressed in this ship's own frame: x forward, y to the left.
    let local = |v: Vec3| geo::frame(me.pos, me.fwd, geo::tangent(me.pos, v));
    let see = |pos: Vec3, vel: Vec3, radius: f32, hp: f32, enemy: bool, fwd: Option<Vec3>| {
        // The straight-line distance is cheap and never longer than the way along the surface.
        if geo::chord(cfg.radius, me.pos, pos) > cfg.view_range {
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
        BaseObs { pos: at, dist: at.len(), hp: known[team] / cfg.base_hp }
    };
    let mut enemy_bases: Vec<BaseObs> = (0..world.bases.len()).filter(|t| *t != me.team as usize && known[*t] > 0.0).map(base).collect();
    enemy_bases.sort_by(|a, b| a.dist.total_cmp(&b.dist));
    enemy_bases.truncate(BASES);
    let mates = || world.ships.iter().enumerate().filter(|(i, s)| s.team == me.team && s.alive && *i != ship).map(|(_, s)| s);
    let voters = mates().count() + me.alive as usize;
    let middle = mates().fold(Vec3::default(), |sum, s| sum.add(s.pos));
    let mut radar: Vec<MateObs> = mates()
        .map(|s| {
            let at = world.to_frame(me.pos, me.fwd, s.pos);
            MateObs { pos: at, dist: at.len(), hull: s.hull.max(0.0) / cfg.hull_max(s), mode: s.vote }
        })
        .collect();
    radar.sort_by(|a, b| a.dist.total_cmp(&b.dist));
    radar.truncate(MATES);
    // Enemies in a teammate's sight but not in this ship's own.
    let mut contacts: Vec<Vec2> = world
        .ships
        .iter()
        .filter(|e| e.alive && e.team != me.team && geo::chord(cfg.radius, me.pos, e.pos) > cfg.view_range)
        .filter(|e| mates().any(|m| geo::chord(cfg.radius, m.pos, e.pos) < cfg.view_range))
        .map(|e| world.to_frame(me.pos, me.fwd, e.pos))
        .collect();
    contacts.sort_by(|a, b| a.len().total_cmp(&b.len()));
    contacts.truncate(CONTACTS);
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
        levels: me.levels,
        base: base(me.team as usize),
        order: me.order.kind,
        target: world.to_frame(me.pos, me.fwd, me.order.target),
        vote: me.vote,
        votes: PLANS.map(|p| {
            let n = mates().filter(|s| s.vote == Some(p)).count() + (me.alive && me.vote == Some(p)) as usize;
            if voters > 0 { n as f32 / voters as f32 } else { 0.0 }
        }),
        team: if middle.len() > 1e-3 { world.to_frame(me.pos, me.fwd, middle.unit()) } else { Vec2::default() },
        enemy_bases,
        mates: radar,
        contacts,
        rocks: nearest(
            world
                .rocks
                .iter()
                .filter_map(|r| see(r.pos, r.vel, cfg.rock_radius_of(r.size), r.hp / cfg.rock_hp_of(3), false, None))
                .collect(),
            NEAR,
        ),
        crystals: nearest(
            world.crystals.iter().filter_map(|c| see(c.pos, Vec3::default(), 0.0, c.boost_cell as u8 as f32, false, None)).collect(),
            NEAR,
        ),
        ships: nearest(
            world
                .ships
                .iter()
                .enumerate()
                .filter(|(i, s)| *i != ship && s.alive)
                .filter_map(|(_, s)| see(s.pos, s.vel, cfg.radius_of(s), s.hull / cfg.hull_max(s), s.team != me.team, Some(s.fwd)))
                .collect(),
            NEAR,
        ),
        shots: nearest(
            world
                .shots
                .iter()
                .filter(|s| s.team != me.team)
                .filter_map(|s| see(s.pos, s.vel, 0.0, s.damage_now(cfg.shot_falloff) / cfg.hull, true, None))
                .collect(),
            SHOTS,
        ),
    }
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
        for (group, slots) in [(&self.rocks, NEAR), (&self.crystals, NEAR), (&self.ships, NEAR), (&self.shots, SHOTS)] {
            for slot in 0..slots {
                match group.get(slot) {
                    Some(n) => put(&[
                        1.0,
                        n.pos.x / view,
                        n.pos.y / view,
                        n.vel.x / speed,
                        n.vel.y / speed,
                        n.radius / cfg.rock_radius_of(3),
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
