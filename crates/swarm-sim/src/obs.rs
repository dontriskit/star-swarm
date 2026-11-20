use crate::sim::{Config, ORDER_KINDS, OrderKind, TIERS, Vec2, World};

/// How many of each kind of nearby object a ship sees.
pub const NEAR: usize = 6;
/// How many incoming enemy shots a ship sees.
pub const SHOTS: usize = 4;
/// How many enemy bases a ship knows about (the nearest ones still standing).
pub const BASES: usize = 2;
const SELF_LEN: usize = 8 + 6 + 4 + ORDER_KINDS + 2;
const BASE_LEN: usize = 4;
const NEAR_LEN: usize = 10;
/// Length of [`Observation::to_vec`].
pub const OBS_LEN: usize = SELF_LEN + BASES * BASE_LEN + (3 * NEAR + SHOTS) * NEAR_LEN;

/// A nearby object in the observing ship's frame: x forward, y to the left,
/// world units.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Near {
    pub pos: Vec2,
    pub vel: Vec2,
    pub dist: f32,
    pub radius: f32,
    /// Remaining health as a fraction; 0 for crystals.
    pub hp: f32,
    pub enemy: bool,
    /// Where another ship is pointing, as a unit vector in this ship's frame;
    /// zero for everything that is not a ship.
    pub facing: Vec2,
}

/// A base as a ship knows it: bases are known map-wide, not only in view.
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
    /// The guns have cooled down.
    pub ready: bool,
    pub tier: u8,
    pub levels: [u8; 6],
    /// The ship's own base; `hp` is 0 once it has been destroyed.
    pub base: BaseObs,
    pub order: OrderKind,
    pub target: Vec2,
    /// Enemy bases still standing, nearest first, at most [`BASES`].
    pub enemy_bases: Vec<BaseObs>,
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
    let see = |pos: Vec2, vel: Vec2, radius: f32, hp: f32, enemy: bool, heading: Option<f32>| {
        let d = world.delta(me.pos, pos);
        let dist = d.len();
        (dist <= cfg.view_range).then(|| Near {
            pos: d.into_frame(me.heading),
            vel: vel.sub(me.vel).into_frame(me.heading),
            dist,
            radius,
            hp,
            enemy,
            facing: heading.map_or(Vec2::default(), |h| Vec2::from_angle(h - me.heading)),
        })
    };
    let base = |team: usize| {
        let b = &world.bases[team];
        let d = world.delta(me.pos, b.pos);
        BaseObs { pos: d.into_frame(me.heading), dist: d.len(), hp: b.hp.max(0.0) / cfg.base_hp }
    };
    let mut enemy_bases: Vec<BaseObs> =
        (0..world.bases.len()).filter(|t| *t != me.team as usize && world.bases[*t].alive()).map(base).collect();
    enemy_bases.sort_by(|a, b| a.dist.total_cmp(&b.dist));
    enemy_bases.truncate(BASES);
    Observation {
        alive: me.alive,
        hull: me.hull.max(0.0),
        hull_max: cfg.hull_max(me),
        shield: me.shield,
        shield_max: cfg.shield_max(me),
        cargo: me.cargo,
        cargo_max: cfg.cargo_max(me),
        vel: me.vel.into_frame(me.heading),
        ready: me.cooldown == 0,
        tier: me.tier,
        levels: me.levels,
        base: base(me.team as usize),
        order: me.order.kind,
        target: world.delta(me.pos, me.order.target).into_frame(me.heading),
        enemy_bases,
        rocks: nearest(
            world
                .rocks
                .iter()
                .filter_map(|r| see(r.pos, r.vel, cfg.rock_radius_of(r.size), r.hp / cfg.rock_hp_of(3), false, None))
                .collect(),
            NEAR,
        ),
        crystals: nearest(
            world.crystals.iter().filter_map(|c| see(c.pos, Vec2::default(), 0.0, 0.0, false, None)).collect(),
            NEAR,
        ),
        ships: nearest(
            world
                .ships
                .iter()
                .enumerate()
                .filter(|(i, s)| *i != ship && s.alive)
                .filter_map(|(_, s)| see(s.pos, s.vel, cfg.ship_radius, s.hull / cfg.hull_max(s), s.team != me.team, Some(s.heading)))
                .collect(),
            NEAR,
        ),
        shots: nearest(
            world
                .shots
                .iter()
                .filter(|s| s.team != me.team)
                .filter_map(|s| see(s.pos, s.vel, 0.0, s.damage / cfg.hull, true, None))
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
        let (view, half, speed) = (cfg.view_range, cfg.world_size / 2.0, cfg.max_speed);
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
        ]);
        put(&self.levels.map(|l| l as f32 / cfg.top_level() as f32));
        put(&[self.base.pos.x / half, self.base.pos.y / half, self.base.hp, (self.base.hp > 0.0) as u8 as f32]);
        let mut order = [0.0; ORDER_KINDS];
        order[self.order as usize] = 1.0;
        put(&order);
        put(&[self.target.x / half, self.target.y / half]);
        for slot in 0..BASES {
            match self.enemy_bases.get(slot) {
                Some(b) => put(&[1.0, b.pos.x / half, b.pos.y / half, b.hp]),
                None => put(&[0.0; BASE_LEN]),
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
