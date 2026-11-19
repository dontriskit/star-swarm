use crate::sim::{ORDER_KINDS, OrderKind, Vec2, World};

/// How many of each kind of nearby object a ship sees.
pub const NEAR: usize = 6;
const SELF_LEN: usize = 7 + 8 + 2 + ORDER_KINDS + 2;
const NEAR_LEN: usize = 8;
/// Length of [`Observation::to_vec`].
pub const OBS_LEN: usize = SELF_LEN + 3 * NEAR * NEAR_LEN;

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
}

/// Everything one ship knows. Every controller gets exactly this: a human's
/// GUI, a scripted bot, a trained policy (through [`Observation::to_vec`]).
#[derive(Clone, Debug, Default)]
pub struct Observation {
    pub alive: bool,
    pub hull: f32,
    pub hull_max: f32,
    pub energy: f32,
    pub energy_max: f32,
    pub cargo: u32,
    pub cargo_max: u32,
    pub vel: Vec2,
    pub ready: bool,
    pub levels: [u8; 8],
    pub base: Vec2,
    pub order: OrderKind,
    pub target: Vec2,
    /// Nearest first, at most [`NEAR`] each, within the view range.
    pub rocks: Vec<Near>,
    pub crystals: Vec<Near>,
    pub ships: Vec<Near>,
}

fn nearest(mut all: Vec<Near>) -> Vec<Near> {
    all.sort_by(|a, b| a.dist.total_cmp(&b.dist));
    all.truncate(NEAR);
    all
}

pub fn observe(world: &World, ship: usize) -> Observation {
    let cfg = &world.cfg;
    let me = &world.ships[ship];
    let see = |pos: Vec2, vel: Vec2, radius: f32, hp: f32, enemy: bool| {
        let d = world.delta(me.pos, pos);
        let dist = d.len();
        (dist <= cfg.view_range).then(|| Near {
            pos: d.into_frame(me.heading),
            vel: vel.sub(me.vel).into_frame(me.heading),
            dist,
            radius,
            hp,
            enemy,
        })
    };
    Observation {
        alive: me.alive,
        hull: me.hull.max(0.0),
        hull_max: cfg.hull_max(me),
        energy: me.energy,
        energy_max: cfg.energy_max(me),
        cargo: me.cargo,
        cargo_max: cfg.cargo_max(me),
        vel: me.vel.into_frame(me.heading),
        ready: me.cooldown == 0 && me.energy >= cfg.shot_cost,
        levels: me.levels,
        base: world.delta(me.pos, world.bases[me.team as usize]).into_frame(me.heading),
        order: me.order.kind,
        target: world.delta(me.pos, me.order.target).into_frame(me.heading),
        rocks: nearest(
            world
                .rocks
                .iter()
                .filter_map(|r| see(r.pos, r.vel, cfg.rock_radius_of(r.size), r.hp / cfg.rock_hp_of(3), false))
                .collect(),
        ),
        crystals: nearest(
            world.crystals.iter().filter_map(|c| see(c.pos, Vec2::default(), 0.0, 0.0, false)).collect(),
        ),
        ships: nearest(
            world
                .ships
                .iter()
                .enumerate()
                .filter(|(i, s)| *i != ship && s.alive)
                .filter_map(|(_, s)| see(s.pos, s.vel, cfg.ship_radius, s.hull / cfg.hull_max(s), s.team != me.team))
                .collect(),
        ),
    }
}

impl Observation {
    /// Fixed-length, roughly unit-scaled vector for a neural network.
    /// Missing neighbours are zero rows with the first element (present) at 0.
    pub fn to_vec(&self, world: &World) -> Vec<f32> {
        let cfg = &world.cfg;
        let (view, half, speed) = (cfg.view_range, cfg.world_size / 2.0, cfg.max_speed);
        let mut v = Vec::with_capacity(OBS_LEN);
        v.extend([
            self.alive as u8 as f32,
            self.hull / self.hull_max,
            self.energy / self.energy_max,
            self.cargo as f32 / self.cargo_max as f32,
            self.vel.x / speed,
            self.vel.y / speed,
            self.ready as u8 as f32,
        ]);
        v.extend(self.levels.iter().map(|l| *l as f32 / cfg.upgrade_max as f32));
        v.extend([self.base.x / half, self.base.y / half]);
        v.extend((0..ORDER_KINDS).map(|k| (k == self.order as usize) as u8 as f32));
        v.extend([self.target.x / half, self.target.y / half]);
        for group in [&self.rocks, &self.crystals, &self.ships] {
            for slot in 0..NEAR {
                match group.get(slot) {
                    Some(n) => v.extend([
                        1.0,
                        n.pos.x / view,
                        n.pos.y / view,
                        n.vel.x / speed,
                        n.vel.y / speed,
                        n.radius / cfg.rock_radius_of(3),
                        n.hp,
                        n.enemy as u8 as f32,
                    ]),
                    None => v.extend([0.0; NEAR_LEN]),
                }
            }
        }
        debug_assert_eq!(v.len(), OBS_LEN);
        v
    }
}
