use crate::obs::{BaseObs, Near, Observation};
use crate::rng::Rng;
use crate::sim::{Action, Buy, Class, Config, DroneKind, Order, OrderKind, TIER2, TIERS, UPGRADES, Upgrade, Vec2, World};

/// Anything that flies a ship: a human's input, a scripted bot, a trained
/// policy, a remote agent.
pub trait Controller {
    fn act(&mut self, obs: &Observation, cfg: &Config) -> Action;
}

/// Hand-written baseline that follows commander orders. Training stages are
/// measured against it.
#[derive(Clone, Debug)]
pub struct ScriptedBot {
    /// The class this bot picks when it moves up to tier 2.
    pub prefers: Class,
}

impl Default for ScriptedBot {
    fn default() -> Self {
        ScriptedBot { prefers: Class::Twin }
    }
}

/// Upgrades in the order the bot prefers them when levels are tied.
const PRIORITY: [Upgrade; 8] = [
    Upgrade::Attack,
    Upgrade::FireRate,
    Upgrade::Speed,
    Upgrade::Shield,
    Upgrade::Ammo,
    Upgrade::Health,
    Upgrade::Agility,
    Upgrade::Regen,
];
/// Farther than this from where it is going, a bot burns its boost tank.
const BOOST_FROM: f32 = 450.0;

const AIM: f32 = 0.1;
const FIGHT_RANGE: f32 = 260.0;
/// Below this share of its hull a ship with a base to go to breaks off for repairs.
const RETREAT: f32 = 0.35;
/// With its hold this full a mining ship flies home to hand the crystals over.
const UNLOAD: f32 = 0.5;

impl ScriptedBot {
    /// Bots for a whole fleet: ship `i` gets its tier-2 class by turns, so every class shows up.
    pub fn for_ship(i: usize) -> Self {
        ScriptedBot { prefers: TIER2[i % TIER2.len()] }
    }

    fn turn_to(angle: f32) -> i8 {
        if angle > 0.05 { 1 } else if angle < -0.05 { -1 } else { 0 }
    }

    /// Heads for a point in the ship's frame and stops thrusting once drag
    /// alone will carry the ship to within `keep` of it.
    fn fly(to: Vec2, keep: f32, obs: &Observation, cfg: &Config) -> Action {
        let angle = to.y.atan2(to.x);
        // How far the ship still goes if it brakes now.
        let speed = obs.vel.len();
        let stop = speed * speed / (2.0 * cfg.brake_accel) + speed * 0.2;
        // Drifting the wrong way: stop first, a ship keeps going the way it was going.
        let astray = speed > 40.0 && obs.vel.dot(to) < 0.3 * speed * to.len();
        let arriving = keep > 0.0 && to.len() - keep <= stop;
        Action {
            turn: Self::turn_to(angle),
            // With nothing to keep clear of (a crystal, a waypoint) arrive at speed.
            thrust: angle.abs() < 0.6 && !arriving,
            brake: (arriving && speed > 15.0) || astray,
            // Boost only on a long, clear run: at boost speed a rock is fatal.
            boost: angle.abs() < 0.3 && to.len() - keep > BOOST_FROM && obs.rocks.first().is_none_or(|r| r.dist > 320.0),
            ..Action::default()
        }
    }

    /// If the ship is on course to hit a rock before drag stops it, thrusts
    /// sideways, away from the point of closest approach.
    fn dodge(obs: &Observation, cfg: &Config) -> Option<Action> {
        for rock in obs.rocks.iter() {
            let speed = rock.vel.len();
            if speed < 1.0 {
                continue;
            }
            let t = -rock.pos.dot(rock.vel) / (speed * speed);
            let miss = rock.pos.add(rock.vel.scale(t));
            let reach = rock.radius + cfg.ship_radius + 15.0;
            if t <= 0.0 || miss.len() > reach || rock.dist - reach > speed * 2.5 {
                continue;
            }
            let away = if miss.len() > 1.0 { miss.scale(-1.0) } else { Vec2::new(-rock.vel.y, rock.vel.x) };
            let angle = away.y.atan2(away.x);
            return Some(Action { turn: Self::turn_to(angle), thrust: angle.abs() < 0.8, boost: false, ..Action::default() });
        }
        None
    }

    /// Closes to `keep` from a target of the given radius and fires once lined up and in range.
    fn shoot_at(pos: Vec2, dist: f32, radius: f32, keep: f32, obs: &Observation, cfg: &Config) -> Action {
        let mut act = Self::fly(pos, radius + keep, obs, cfg);
        act.fire = pos.y.atan2(pos.x).abs() < AIM.max((radius / dist.max(1.0)).atan()) && dist < cfg.weapon(obs.class).range * 0.9;
        act
    }

    fn shoot(at: &Near, keep: f32, obs: &Observation, cfg: &Config) -> Action {
        if obs.class == Class::Ram && at.enemy {
            // A Ram does its damage by flying into the enemy, as fast as it can.
            let mut act = Self::fly(at.pos, 0.0, obs, cfg);
            act.boost = act.thrust;
            act.fire = at.pos.y.atan2(at.pos.x).abs() < AIM && at.dist < cfg.weapon(obs.class).range * 0.9;
            return act;
        }
        Self::shoot_at(at.pos, at.dist, at.radius, keep, obs, cfg)
    }

    fn siege(base: &BaseObs, obs: &Observation, cfg: &Config) -> Action {
        Self::shoot_at(base.pos, base.dist, cfg.base_radius, 150.0, obs, cfg)
    }

    fn mine(obs: &Observation, cfg: &Config) -> Action {
        // A crystal pack needs cargo room; a boost cell is only worth it with a tank that is not full.
        let wanted = |c: &&Near| if c.enemy { obs.boost < 0.95 } else { obs.cargo < obs.cargo_max || obs.hull < obs.hull_max };
        if let Some(c) = obs.crystals.iter().find(wanted) {
            return Self::fly(c.pos, 0.0, obs, cfg);
        }
        // Prefer rocks this ship can break soon: a big rock nearby is worth less than a small one a little farther.
        let effort = |r: &&Near| r.dist + r.hp * cfg.rock_hp_of(crate::sim::ROCK_SIZES) * 12.0 / cfg.weapon(obs.class).damage.max(0.1);
        match obs.rocks.iter().min_by(|a, b| effort(a).total_cmp(&effort(b))) {
            // With shields to spare, a small rock is broken fastest by flying into it while shooting.
            Some(rock) if rock.radius <= cfg.rock_radius_of(2) && obs.shield > 0.5 * obs.shield_max => {
                let mut act = Self::shoot(rock, 140.0, obs, cfg);
                act.thrust = rock.pos.y.atan2(rock.pos.x).abs() < 0.6;
                act
            }
            Some(rock) => Self::shoot(rock, 140.0, obs, cfg),
            // Nothing in sight: fly to the nearest farm that still has rocks.
            None if !obs.fields.is_empty() => Self::fly(obs.fields[0].pos, 0.0, obs, cfg),
            // No farm known: head for the side the long-range radar shows the most rock on.
            None => {
                let best = (0..obs.rock_radar.len()).max_by(|a, b| obs.rock_radar[*a].total_cmp(&obs.rock_radar[*b])).unwrap_or(0);
                let angle = (best as f32 + 0.5) * std::f32::consts::TAU / obs.rock_radar.len() as f32;
                let angle = if angle > std::f32::consts::PI { angle - std::f32::consts::TAU } else { angle };
                Action { thrust: angle.abs() < 0.9, turn: Self::turn_to(angle), ..Action::default() }
            }
        }
    }

    /// Spends points: repairs a badly damaged base first, then fills its drone slots, then
    /// rushes for the next tier while the base allows one; at the highest tier the base allows
    /// it spreads points evenly over the upgrades.
    fn shop(&self, obs: &Observation, cfg: &Config) -> Option<Buy> {
        if obs.base.hp > 0.0 && obs.base.hp < 0.6 && obs.points >= cfg.repair_step {
            return Some(Buy::Repair);
        }
        if obs.drones < cfg.drone_slots[obs.tier as usize - 1] {
            // The first drone mines; any further one fights.
            let kind = if obs.drone_kinds[DroneKind::Eco as usize] == 0 { DroneKind::Eco } else { DroneKind::Combat };
            return (obs.points >= cfg.drone_cost[kind as usize]).then_some(Buy::Drone(kind));
        }
        // While the base allows a higher tier, rush for it: a tier is worth more than the
        // upgrades it wipes. Only a first level of attack and of ammo is bought on the way, to
        // mine faster; everything else is saved for the tier.
        if obs.tier < TIERS && cfg.base_allows(obs.tier, obs.base_level) {
            if obs.points >= cfg.tier_cost[obs.tier as usize - 1] {
                return Some(Buy::Tier(self.prefers));
            }
            return [Upgrade::Attack, Upgrade::Ammo]
                .into_iter()
                .find(|u| obs.levels[*u as usize] == 0)
                .filter(|_| cfg.cost(0, obs.tier).is_some_and(|cost| obs.points >= cost))
                .map(Buy::Upgrade);
        }
        // As high as the base lets it go: spread the points evenly over the upgrades.
        let next = PRIORITY.iter().copied().min_by_key(|u| obs.levels[*u as usize])?;
        cfg.cost(obs.levels[next as usize], obs.tier).filter(|cost| obs.points >= *cost).map(|_| Buy::Upgrade(next))
    }
}

impl Controller for ScriptedBot {
    fn act(&mut self, obs: &Observation, cfg: &Config) -> Action {
        if !obs.alive {
            return Action::default();
        }
        let enemy = obs.ships.iter().find(|s| s.enemy);
        let healthy = obs.hull > 0.4 * obs.hull_max;
        let home = obs.base.hp > 0.0;
        // Crystals are worth nothing until handed over: with a well-filled hold, and no orders
        // to be somewhere else, head home.
        // With a well-filled hold and no orders to be somewhere else, a ship flies home; once it is
        // unloading it stays until the hold is empty, and while it can still afford something.
        let free = matches!(obs.order, OrderKind::Mine | OrderKind::Defend);
        let loaded = obs.cargo as f32 >= UNLOAD * obs.cargo_max as f32 || (obs.in_base && obs.cargo > 0);
        let shopping = obs.in_base && self.shop(obs, cfg).is_some();
        // Badly hurt, it goes home whatever its orders, and once docked stays until mended.
        let hurt = obs.hull < RETREAT * obs.hull_max || (obs.in_base && obs.hull < 0.9 * obs.hull_max);
        let unload = home && ((free && (loaded || shopping)) || hurt);
        let full = obs.base_docked >= obs.base_slots && !obs.in_base;
        let mut act = if unload && (obs.in_base || obs.base.dist < cfg.base_radius * 0.6) {
            // Docked, or inside and coming to rest: sit still, hand the crystals over, buy.
            Action { brake: true, ..Action::default() }
        } else if unload && full && obs.base.dist < cfg.base_heal_radius {
            // Every place in the base is taken: wait just outside for one to come free.
            Action { brake: true, ..Action::default() }
        } else if unload && obs.base.dist > cfg.base_radius * 0.5 {
            // Badly hurt or loaded up: back to the base, right into it.
            Self::fly(obs.base.pos, if unload { cfg.base_radius * 0.3 } else { 0.0 }, obs, cfg)
        } else {
            match obs.order {
                OrderKind::Mine => match enemy {
                    Some(e) if e.dist < FIGHT_RANGE && healthy => Self::shoot(e, 160.0, obs, cfg),
                    _ => Self::mine(obs, cfg),
                },
                OrderKind::Attack => match (enemy, obs.enemy_bases.first()) {
                    (Some(e), _) if e.dist < FIGHT_RANGE => Self::shoot(e, 160.0, obs, cfg),
                    // An enemy base in reach matters more than anything farther away.
                    (_, Some(base)) if base.dist < cfg.view_range => Self::siege(base, obs, cfg),
                    (Some(e), _) => Self::shoot(e, 160.0, obs, cfg),
                    _ if obs.target.len() > 150.0 => Self::fly(obs.target, 0.0, obs, cfg),
                    _ => Self::mine(obs, cfg),
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
            }
        };
        // Dodging is for rocks too big to break by flying into them.
        let ramming = obs.shield > 0.5 * obs.shield_max && obs.rocks.first().is_some_and(|r| r.radius <= cfg.rock_radius_of(2));
        if let Some(dodge) = Self::dodge(obs, cfg).filter(|_| !ramming) {
            act = Action { fire: act.fire, ..dodge };
        }
        act.deposit = obs.in_base;
        act.buy = self.shop(obs, cfg).filter(|_| obs.in_base);
        debug_assert_eq!(UPGRADES.len(), PRIORITY.len());
        act
    }
}

/// Hand-written commander for teams no human leads: one ship guards the base,
/// ships that have upgraded enough raid the nearest enemy base still standing,
/// the rest keep mining.
#[derive(Clone, Debug)]
pub struct ScriptedCommander {
    rng: Rng,
    /// The enemy team each team is currently raiding.
    raiding: Vec<Option<u8>>,
}

impl ScriptedCommander {
    /// Upgrade levels a ship needs before it is sent to fight.
    const RAID_LEVELS: u32 = 5;

    pub fn new(seed: u64) -> Self {
        ScriptedCommander { rng: Rng::new(seed), raiding: Vec::new() }
    }

    /// Re-issues orders to one team and returns how many ships were sent to raid.
    pub fn command(&mut self, world: &mut World, team: u8) -> usize {
        let home = world.bases[team as usize].pos;
        let standing: Vec<u8> = (0..world.bases.len() as u8).filter(|t| *t != team && world.bases[*t as usize].alive()).collect();
        if standing.is_empty() {
            return 0;
        }
        // Pick an enemy at random and stay on it until its base is gone, so no team is everyone's first target.
        self.raiding.resize(world.bases.len(), None);
        let current = self.raiding[team as usize].filter(|t| standing.contains(t));
        let enemy = current.unwrap_or_else(|| standing[self.rng.below(standing.len() as u32) as usize]);
        self.raiding[team as usize] = Some(enemy);
        let target = world.bases[enemy as usize].pos;
        let guards_home = world.bases[team as usize].alive();
        let (mut raiders, mut guard_set) = (0, false);
        for ship in world.ships.iter_mut().filter(|s| s.team == team) {
            let levels: u32 = ship.levels.iter().map(|l| *l as u32).sum();
            ship.order = if guards_home && !guard_set {
                guard_set = true;
                Order { kind: OrderKind::Defend, target: home }
            } else if levels >= Self::RAID_LEVELS || !guards_home {
                raiders += 1;
                Order { kind: OrderKind::Attack, target }
            } else {
                Order::default()
            };
        }
        raiders
    }
}
