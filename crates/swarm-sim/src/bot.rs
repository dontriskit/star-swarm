use crate::obs::{Near, Observation};
use crate::rng::Rng;
use crate::sim::{Action, Config, Order, OrderKind, UPGRADES, Upgrade, Vec2, World};

/// Anything that flies a ship: a human's input, a scripted bot, a trained
/// policy, a remote agent.
pub trait Controller {
    fn act(&mut self, obs: &Observation, cfg: &Config) -> Action;
}

/// Hand-written baseline that follows commander orders. Training stages are
/// measured against it.
#[derive(Clone, Debug, Default)]
pub struct ScriptedBot;

/// Upgrades in the order the bot prefers them when levels are tied.
const PRIORITY: [Upgrade; 8] = [
    Upgrade::Damage,
    Upgrade::EnergyRegen,
    Upgrade::Speed,
    Upgrade::HullCap,
    Upgrade::Agility,
    Upgrade::EnergyCap,
    Upgrade::HullRegen,
    Upgrade::Cargo,
];

const AIM: f32 = 0.1;
const FIGHT_RANGE: f32 = 260.0;

impl ScriptedBot {
    fn turn_to(angle: f32) -> i8 {
        if angle > 0.05 { 1 } else if angle < -0.05 { -1 } else { 0 }
    }

    /// Heads for a point in the ship's frame and stops thrusting once drag
    /// alone will carry the ship to within `keep` of it.
    fn fly(to: Vec2, keep: f32, obs: &Observation, cfg: &Config) -> Action {
        let angle = to.y.atan2(to.x);
        let coast = obs.vel.len() / cfg.drag;
        Action {
            turn: Self::turn_to(angle),
            // With nothing to keep clear of (a crystal, a waypoint) arrive at speed.
            thrust: angle.abs() < 0.6 && (keep == 0.0 || to.len() - keep > coast),
            ..Action::default()
        }
    }

    /// If the ship is on course to hit a rock before drag stops it, thrusts
    /// sideways, away from the point of closest approach.
    fn dodge(obs: &Observation, cfg: &Config) -> Option<Action> {
        for rock in &obs.rocks {
            let speed = rock.vel.len();
            if speed < 1.0 {
                continue;
            }
            let t = -rock.pos.dot(rock.vel) / (speed * speed);
            let miss = rock.pos.add(rock.vel.scale(t));
            let reach = rock.radius + cfg.ship_radius + 15.0;
            if t <= 0.0 || miss.len() > reach || rock.dist - reach > speed / cfg.drag {
                continue;
            }
            let away = if miss.len() > 1.0 { miss.scale(-1.0) } else { Vec2::new(-rock.vel.y, rock.vel.x) };
            let angle = away.y.atan2(away.x);
            return Some(Action { turn: Self::turn_to(angle), thrust: angle.abs() < 0.8, ..Action::default() });
        }
        None
    }

    fn shoot(at: &Near, keep: f32, obs: &Observation, cfg: &Config) -> Action {
        let mut act = Self::fly(at.pos, at.radius + keep, obs, cfg);
        act.fire = at.pos.y.atan2(at.pos.x).abs() < AIM.max((at.radius / at.dist.max(1.0)).atan())
            && at.dist < cfg.shot_range() * 0.9;
        act
    }

    fn mine(obs: &Observation, cfg: &Config) -> Action {
        if obs.cargo < obs.cargo_max {
            if let Some(c) = obs.crystals.first() {
                return Self::fly(c.pos, 0.0, obs, cfg);
            }
        }
        match obs.rocks.first() {
            Some(rock) => Self::shoot(rock, 140.0, obs, cfg),
            // Nothing in sight: cruise until something shows up.
            None => Action { thrust: true, ..Action::default() },
        }
    }
}

impl Controller for ScriptedBot {
    fn act(&mut self, obs: &Observation, cfg: &Config) -> Action {
        if !obs.alive {
            return Action::default();
        }
        let enemy = obs.ships.iter().find(|s| s.enemy);
        let healthy = obs.hull > 0.4 * obs.hull_max;
        let mut act = match obs.order {
            OrderKind::Mine => match enemy {
                Some(e) if e.dist < FIGHT_RANGE && healthy => Self::shoot(e, 160.0, obs, cfg),
                _ => Self::mine(obs, cfg),
            },
            OrderKind::Attack => match enemy {
                Some(e) => Self::shoot(e, 160.0, obs, cfg),
                None if obs.target.len() > 150.0 => Self::fly(obs.target, 0.0, obs, cfg),
                None => Self::mine(obs, cfg),
            },
            OrderKind::Defend => match enemy {
                _ if obs.target.len() > 350.0 => Self::fly(obs.target, 0.0, obs, cfg),
                Some(e) => Self::shoot(e, 160.0, obs, cfg),
                None => Self::mine(obs, cfg),
            },
            OrderKind::Regroup => match enemy {
                _ if obs.target.len() > 120.0 => Self::fly(obs.target, 0.0, obs, cfg),
                Some(e) if e.dist < FIGHT_RANGE => Self::shoot(e, 160.0, obs, cfg),
                _ => Action::default(),
            },
        };
        if let Some(dodge) = Self::dodge(obs, cfg) {
            act = Action { fire: act.fire, ..dodge };
        }
        // Spread crystals evenly: buy the lowest-level upgrade as soon as it is affordable.
        let next = PRIORITY.iter().copied().min_by_key(|u| obs.levels[*u as usize]);
        if let Some(u) = next {
            if cfg.cost(obs.levels[u as usize]).is_some_and(|cost| obs.cargo >= cost) {
                act.buy = Some(u);
            }
        }
        debug_assert_eq!(UPGRADES.len(), PRIORITY.len());
        act
    }
}

/// Hand-written commander for teams no human leads: ships that have upgraded
/// enough are sent to raid another team's base, the rest keep mining.
#[derive(Clone, Debug)]
pub struct ScriptedCommander {
    rng: Rng,
}

impl ScriptedCommander {
    /// Upgrade levels a ship needs before it is sent to fight.
    const RAID_LEVELS: u32 = 6;

    pub fn new(seed: u64) -> Self {
        ScriptedCommander { rng: Rng::new(seed) }
    }

    pub fn command(&mut self, world: &mut World, team: u8) {
        let teams = world.bases.len() as u32;
        if teams < 2 {
            return;
        }
        let other = (team as u32 + 1 + self.rng.below(teams - 1)) % teams;
        let target = world.bases[other as usize];
        for ship in world.ships.iter_mut().filter(|s| s.team == team) {
            let levels: u32 = ship.levels.iter().map(|l| *l as u32).sum();
            ship.order = if levels >= Self::RAID_LEVELS {
                Order { kind: OrderKind::Attack, target }
            } else {
                Order::default()
            };
        }
    }
}
