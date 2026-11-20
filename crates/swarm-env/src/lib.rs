//! Many matches stepped side by side, for training.
//!
//! Every ship of every match is one agent. Agents are numbered match by match,
//! ship by ship, and all buffers are flat arrays in that order.

use rayon::prelude::*;
use swarm_sim::replay::decode;
use swarm_sim::{Action, Config, Controller, Event, OBS_LEN, ScriptedBot, ScriptedCommander, World, observe, replay};

/// Scripted teams get fresh orders from their scripted commander this often.
const COMMAND_EVERY: u32 = 450;

/// What an agent is paid for. Training stages differ only in these weights.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rewards {
    /// Per crystal picked up.
    pub collect: f32,
    /// Per rock destroyed, times the rock's size (1 to 3).
    pub rock: f32,
    /// Per upgrade level bought.
    pub buy: f32,
    /// Per point of damage dealt to an enemy ship.
    pub hit: f32,
    /// Per enemy ship destroyed.
    pub kill: f32,
    /// Per point of damage taken by flying into a rock (subtracted).
    pub crash: f32,
    /// Per death (subtracted).
    pub death: f32,
    /// Per point of damage dealt to an enemy base.
    pub base_hit: f32,
    /// Per point of damage the ship's own base takes, charged to every ship of the team (subtracted).
    pub base_lost: f32,
    /// To every ship of the team whose base is the last one standing.
    pub win: f32,
    /// To every ship of a team when its base is destroyed (subtracted).
    pub lose: f32,
    /// Per point of damage the ship takes, from shots or rocks, shield or hull (subtracted).
    pub hurt: f32,
    /// Per tick the match goes on, from every ship (subtracted): time spent achieving nothing costs.
    pub time: f32,
    /// From every ship when a match runs out of time with no winner (subtracted).
    pub draw: f32,
    /// Per crystal a ship hands over at its base.
    pub deposit: f32,
    /// To every ship of a team when its base reaches a new level.
    pub base_level: f32,
    /// Per point of damage a ship's shots do to a rock of the smallest size.
    pub rock_hit: f32,
    /// Extra per point of damage dealt to an enemy ship while the team is attacking and the
    /// attacker has the upper hand: the enemy has less hull and shield left than the attacker,
    /// is outnumbered around itself, or has drones to be taken.
    pub edge: f32,
}

impl Default for Rewards {
    fn default() -> Self {
        Rewards { collect: 1.0, rock: 0.5, buy: 2.0, hit: 0.0, kill: 0.0, crash: 0.05, death: 5.0, base_hit: 0.0, base_lost: 0.0, win: 0.0, lose: 0.0, hurt: 0.0, time: 0.0, draw: 0.0, deposit: 0.0, base_level: 0.0, rock_hit: 0.0, edge: 0.0 }
    }
}

/// One finished match, seen from team 0: per-ship averages and how it ended.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EpisodeStats {
    /// Which slot the match ran in, and how many matches that slot had finished before it.
    pub env: u32,
    pub episode: u32,
    pub minutes: f32,
    pub collected: f32,
    pub levels: f32,
    pub kills: f32,
    pub deaths: f32,
    /// Team 0's base was the last one standing.
    pub won: bool,
    /// Team 0's base was destroyed.
    pub lost: bool,
    /// Share of team 0's ships in each class at the end, indexed by [`swarm_sim::Class`].
    pub classes: [f32; 5],
    /// Team 0's mean level of each upgrade at the end, indexed by [`swarm_sim::Upgrade`].
    pub upgrades: [f32; 8],
    /// Share of team 0's ships at tier 1, 2 and 3 at the end.
    pub tiers: [f32; 3],
    /// Share of ship-time team 0 spent in each mode, indexed by [`swarm_sim::Plan`]; the rest in none.
    pub plans: [f32; 4],
    /// The level team 0's base reached.
    pub base_level: f32,
    /// Crystals team 0's ships handed over at the base, per ship.
    pub deposited: f32,
    /// Drones team 0's ships had at the end, per ship.
    pub drones: f32,
}

impl EpisodeStats {
    fn of(world: &World, env: u64, episode: u64) -> Self {
        let team: Vec<&swarm_sim::Ship> = world.ships.iter().filter(|s| s.team == 0).collect();
        let mean = |f: fn(&swarm_sim::Ship) -> u32| team.iter().map(|s| f(s)).sum::<u32>() as f32 / team.len() as f32;
        EpisodeStats {
            env: env as u32,
            episode: episode as u32,
            minutes: world.tick as f32 * world.cfg.dt / 60.0,
            collected: mean(|s| s.collected),
            levels: mean(|s| s.levels.iter().map(|l| *l as u32).sum()),
            kills: mean(|s| s.kills),
            deaths: mean(|s| s.deaths),
            won: world.winner() == Some(0),
            lost: !world.bases[0].alive(),
            classes: std::array::from_fn(|c| team.iter().filter(|s| s.class as usize == c).count() as f32 / team.len() as f32),
            upgrades: std::array::from_fn(|u| team.iter().map(|s| s.levels[u] as f32).sum::<f32>() / team.len() as f32),
            tiers: std::array::from_fn(|t| team.iter().filter(|s| s.tier as usize == t + 1).count() as f32 / team.len() as f32),
            plans: [0.0; 4],
            base_level: world.bases[0].level as f32,
            deposited: mean(|s| s.deposited),
            drones: world.drones.iter().filter(|d| d.owner.is_some_and(|o| world.ships[o as usize].team == 0)).count() as f32 / team.len() as f32,
        }
    }
}

/// Magic at the start of a live stream file.
pub const LIVE_MAGIC: &[u8; 8] = b"SWLIVE01";

/// One thing that happened in the watched match, in the order it happened.
enum Tape {
    /// A new match began with this seed.
    Episode(u64),
    /// The action of every ship for one tick.
    Tick(Vec<u16>),
    /// The state hash after the tick just taken, to catch a viewer that has drifted.
    Hash(u64),
}

/// Ticks between state hashes in a live stream.
const LIVE_HASH_EVERY: u32 = 300;

struct Slot {
    /// The seed of the match in progress.
    seed: u64,
    /// Set for the one slot that is streamed to a viewer.
    tape: Option<Vec<Tape>>,
    /// Whether the match in progress has been on the tape from its first tick.
    on_tape: bool,
    /// Ship-ticks team 0 has spent in each mode in the match in progress.
    planned: [u32; 4],
    world: World,
    commander: ScriptedCommander,
    index: u64,
    episode: u64,
}

pub struct VecEnv {
    cfg: Config,
    rewards: Rewards,
    frame_skip: u32,
    /// Teams 0..learners take the actions passed to `step`; the rest are scripted.
    learners: usize,
    seed: u64,
    slots: Vec<Slot>,
    finished: Vec<EpisodeStats>,
    live: Option<std::io::BufWriter<std::fs::File>>,
}

impl VecEnv {
    /// `frame_skip` is how many ticks each chosen action is held for. Every team is a learner.
    pub fn new(cfg: Config, envs: usize, seed: u64, frame_skip: u32, rewards: Rewards) -> Self {
        let learners = cfg.teams;
        Self::with_learners(cfg, envs, seed, frame_skip, rewards, learners)
    }

    /// Teams `0..learners` are driven through `step`; the others fly on the scripted bot
    /// under a scripted commander. Scripted ships still have agent slots; see [`VecEnv::controlled`].
    pub fn with_learners(cfg: Config, envs: usize, seed: u64, frame_skip: u32, rewards: Rewards, learners: usize) -> Self {
        assert!(envs > 0 && frame_skip > 0 && cfg.teams * cfg.ships_per_team > 0 && learners <= cfg.teams);
        let slots = (0..envs as u64)
            .map(|index| {
                let seed = Self::seed_of(seed, index, 0, envs as u64);
                Slot { seed, tape: None, on_tape: false, planned: [0; 4], world: World::new(cfg.clone(), seed), commander: ScriptedCommander::new(seed), index, episode: 0 }
            })
            .collect();
        VecEnv { cfg, rewards, frame_skip, learners, seed, slots, finished: Vec::new(), live: None }
    }

    /// Streams the first slot's matches to a file as they are played: each match's seed and
    /// setup, then every ship's action on every tick. A viewer that replays them sees exactly
    /// what happens in training, because the simulation is deterministic.
    pub fn stream_to(&mut self, path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
        use std::io::Write;
        let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
        file.write_all(LIVE_MAGIC)?;
        self.live = Some(file);
        // A match that has not started yet is streamed from its first tick. One already in
        // progress cannot be joined half-way; then the viewer starts with the next one.
        let slot = &mut self.slots[0];
        slot.on_tape = slot.world.tick == 0;
        slot.tape = Some(if slot.on_tape { vec![Tape::Episode(slot.seed)] } else { Vec::new() });
        self.flush_tape()
    }

    fn flush_tape(&mut self) -> std::io::Result<()> {
        use std::io::Write;
        let (Some(file), Some(tape)) = (&mut self.live, &mut self.slots[0].tape) else { return Ok(()) };
        for item in tape.drain(..) {
            match item {
                Tape::Episode(seed) => {
                    file.write_all(b"E")?;
                    file.write_all(&seed.to_le_bytes())?;
                    file.write_all(&self.cfg.radius.to_le_bytes())?;
                    file.write_all(&(self.cfg.teams as u16).to_le_bytes())?;
                    file.write_all(&(self.cfg.ships_per_team as u16).to_le_bytes())?;
                    file.write_all(&(self.cfg.rock_count as u32).to_le_bytes())?;
                    file.write_all(&self.cfg.max_ticks.to_le_bytes())?;
                    file.write_all(&self.cfg.base_hp.to_le_bytes())?;
                    file.write_all(&self.cfg.turret_damage.to_le_bytes())?;
                }
                Tape::Tick(actions) => {
                    file.write_all(b"T")?;
                    for a in actions {
                        file.write_all(&a.to_le_bytes())?;
                    }
                }
                Tape::Hash(hash) => {
                    file.write_all(b"H")?;
                    file.write_all(&hash.to_le_bytes())?;
                }
            }
        }
        file.flush()
    }

    /// One byte per agent: 1 where `step` uses the action passed in, 0 where the ship is scripted.
    pub fn controlled(&self, out: &mut [u8]) {
        assert_eq!(out.len(), self.agents());
        for (agent, flag) in out.iter_mut().enumerate() {
            *flag = ((agent % self.ships()) / self.cfg.ships_per_team < self.learners) as u8;
        }
    }

    /// Every match of every slot gets its own seed, so runs are reproducible and maps never repeat.
    fn seed_of(seed: u64, index: u64, episode: u64, envs: u64) -> u64 {
        seed.wrapping_add(index).wrapping_add(episode.wrapping_mul(envs))
    }

    pub fn envs(&self) -> usize {
        self.slots.len()
    }

    pub fn ships(&self) -> usize {
        self.cfg.teams * self.cfg.ships_per_team
    }

    pub fn agents(&self) -> usize {
        self.envs() * self.ships()
    }

    pub fn config(&self) -> &Config {
        &self.cfg
    }

    /// The match currently running in one slot.
    pub fn world(&self, env: usize) -> &World {
        &self.slots[env].world
    }

    /// Writes every agent's observation; `out` holds `agents() * OBS_LEN` values.
    pub fn observe(&self, out: &mut [f32]) {
        let ships = self.ships();
        assert_eq!(out.len(), self.agents() * OBS_LEN);
        self.slots.par_iter().zip(out.par_chunks_mut(ships * OBS_LEN)).for_each(|(slot, out)| {
            for (ship, row) in out.chunks_mut(OBS_LEN).enumerate() {
                observe(&slot.world, ship).fill(&slot.world.cfg, row);
            }
        });
    }

    /// What the scripted bot would do for every agent, in the action encoding of [`VecEnv::step`].
    pub fn scripted(&self, out: &mut [u16]) {
        let ships = self.ships();
        assert_eq!(out.len(), self.agents());
        self.slots.par_iter().zip(out.par_chunks_mut(ships)).for_each(|(slot, out)| {
            for (ship, action) in out.iter_mut().enumerate() {
                let mut bot = ScriptedBot::for_ship(ship);
                *action = replay::encode(&bot.act(&observe(&slot.world, ship), &slot.world.cfg));
            }
        });
    }

    /// Holds each agent's action (16 bits, see [`swarm_sim::replay::encode`]) for
    /// `frame_skip` ticks. `rewards` and `dones` get one entry per agent. A match
    /// that ends is replaced by a fresh one at once; its agents get `dones` = 1
    /// and the next observation comes from the new match.
    pub fn step(&mut self, actions: &[u16], rewards: &mut [f32], dones: &mut [u8]) {
        let ships = self.ships();
        let agents = self.agents();
        assert!(actions.len() == agents && rewards.len() == agents && dones.len() == agents);
        let (pay, frame_skip, seed, envs, cfg) = (self.rewards, self.frame_skip, self.seed, self.envs() as u64, &self.cfg);
        let (learners, per_team) = (self.learners, self.cfg.ships_per_team);
        let finished: Vec<EpisodeStats> = self
            .slots
            .par_iter_mut()
            .zip(actions.par_chunks(ships))
            .zip(rewards.par_chunks_mut(ships))
            .zip(dones.par_chunks_mut(ships))
            .filter_map(|(((slot, actions), rewards), dones)| {
                rewards.fill(0.0);
                dones.fill(0);
                let mut held: Vec<Action> = actions.iter().map(|b| decode(*b)).collect();
                let team_of = |ship: usize| ship / per_team;
                for _ in 0..frame_skip {
                    if learners < cfg.teams {
                        // Scripted teams decide every tick, as they do in the game.
                        if slot.world.tick % COMMAND_EVERY == 0 {
                            for team in learners..cfg.teams {
                                slot.commander.command(&mut slot.world, team as u8);
                            }
                        }
                        for ship in learners * per_team..held.len() {
                            held[ship] = ScriptedBot::for_ship(ship).act(&observe(&slot.world, ship), &slot.world.cfg);
                        }
                    }
                    // Only a match seen from its first tick is on the tape.
                    let taped = slot.on_tape;
                    if taped {
                        slot.tape.as_mut().unwrap().push(Tape::Tick(held.iter().map(replay::encode).collect()));
                    }
                    slot.world.step(&held);
                    // The events are judged against the world as the tick left it.
                    let world = &slot.world;
                    let upper_hand = |by: usize, victim: usize, damage: f32| {
                        let (a, v) = (&world.ships[by], &world.ships[victim]);
                        if a.order.kind != swarm_sim::OrderKind::Attack {
                            return false;
                        }
                        let near = |team: u8| world.ships.iter().filter(|s| s.alive && s.team == team && world.dist(s.pos, v.pos) < 400.0).count();
                        // As the enemy stood before this hit.
                        v.hull + v.shield + damage < a.hull + a.shield
                            || near(a.team) > near(v.team)
                            || world.drones.iter().any(|d| d.owner == Some(victim as u16))
                    };
                    for event in &world.events {
                        match *event {
                            Event::BaseHit { by, team, damage } => {
                                rewards[by as usize] += pay.base_hit * damage;
                                (0..rewards.len()).filter(|s| team_of(*s) == team as usize).for_each(|s| rewards[s] -= pay.base_lost * damage);
                            }
                            Event::BaseDestroyed { team, .. } => {
                                (0..rewards.len()).filter(|s| team_of(*s) == team as usize).for_each(|s| rewards[s] -= pay.lose);
                            }
                            Event::Collected { ship, amount } => rewards[ship as usize] += pay.collect * amount as f32,
                            Event::RockDestroyed { by: Some(ship), size } => rewards[ship as usize] += pay.rock * size as f32,
                            Event::RockDestroyed { by: None, .. } => {}
                            Event::Bought { ship, .. } => rewards[ship as usize] += pay.buy,
                            Event::Hit { by, victim, damage } => {
                                rewards[victim as usize] -= pay.hurt * damage;
                                if let Some(by) = by {
                                    let bonus = if upper_hand(by as usize, victim as usize, damage) { pay.edge } else { 0.0 };
                                    rewards[by as usize] += (pay.hit + bonus) * damage;
                                }
                            }
                            Event::RockHit { by, size, damage } => {
                                if size == 1 {
                                    rewards[by as usize] += pay.rock_hit * damage;
                                }
                            }
                            Event::Demoted { .. } => {}
                            Event::Deposited { ship, amount } => rewards[ship as usize] += pay.deposit * amount as f32,
                            Event::BaseLevel { team, .. } => {
                                (0..rewards.len()).filter(|s| team_of(*s) == team as usize).for_each(|s| rewards[s] += pay.base_level);
                            }
                            Event::Crashed { ship, damage } => rewards[ship as usize] -= (pay.crash + pay.hurt) * damage,
                            Event::Killed { by, victim } => {
                                rewards[victim as usize] -= pay.death;
                                if let Some(by) = by {
                                    rewards[by as usize] += pay.kill;
                                }
                            }
                        }
                    }
                    rewards.iter_mut().for_each(|r| *r -= pay.time);
                    // An upgrade is bought once, not once per held tick.
                    held.iter_mut().for_each(|a| a.buy = None);
                    for mode in slot.world.ships.iter().filter(|s| s.team == 0).filter_map(|s| s.vote) {
                        slot.planned[mode as usize] += 1;
                    }
                    if taped && slot.world.tick % LIVE_HASH_EVERY == 0 {
                        slot.tape.as_mut().unwrap().push(Tape::Hash(slot.world.state_hash()));
                    }
                    if slot.world.done() {
                        match slot.world.winner() {
                            Some(winner) => (0..rewards.len()).filter(|s| team_of(*s) == winner as usize).for_each(|s| rewards[s] += pay.win),
                            // Time ran out with more than one base standing: nobody won, and everybody pays.
                            None => rewards.iter_mut().for_each(|r| *r -= pay.draw),
                        }
                        let mut stats = EpisodeStats::of(&slot.world, slot.index, slot.episode);
                        stats.plans = slot.planned.map(|ticks| ticks as f32 / (slot.world.tick.max(1) as usize * per_team) as f32);
                        slot.planned = [0; 4];
                        slot.episode += 1;
                        let next = Self::seed_of(seed, slot.index, slot.episode, envs);
                        slot.world = World::new(cfg.clone(), next);
                        slot.commander = ScriptedCommander::new(next);
                        slot.seed = next;
                        if let Some(tape) = &mut slot.tape {
                            tape.push(Tape::Episode(next));
                            slot.on_tape = true;
                        }
                        dones.fill(1);
                        return Some(stats);
                    }
                }
                None
            })
            .collect();
        self.finished.extend(finished);
        // A viewer missing a few ticks is no reason to stop training.
        let _ = self.flush_tape();
    }

    /// Stats of the matches that ended since the last call.
    pub fn take_stats(&mut self) -> Vec<EpisodeStats> {
        std::mem::take(&mut self.finished)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mining() -> Config {
        Config { teams: 1, ships_per_team: 4, rock_count: 40, radius: 400.0, max_ticks: 900, ..Config::default() }
    }

    fn run(env: &mut VecEnv, steps: usize) -> (f32, Vec<EpisodeStats>, Vec<f32>) {
        let agents = env.agents();
        let (mut actions, mut rewards, mut dones) = (vec![0u16; agents], vec![0f32; agents], vec![0u8; agents]);
        let mut obs = vec![0f32; agents * OBS_LEN];
        let (mut total, mut stats) = (0.0, Vec::new());
        for _ in 0..steps {
            env.scripted(&mut actions);
            env.step(&actions, &mut rewards, &mut dones);
            total += rewards.iter().sum::<f32>();
            stats.extend(env.take_stats());
        }
        env.observe(&mut obs);
        (total, stats, obs)
    }

    #[test]
    fn scripted_agents_earn_reward_and_matches_restart() {
        let mut env = VecEnv::new(mining(), 8, 1, 3, Rewards::default());
        let (total, stats, obs) = run(&mut env, 700);
        assert!(total > 0.0, "scripted bots earned {total}");
        // 900 ticks at 3 per step is 300 steps a match: two rounds of 8 matches have ended.
        assert_eq!(stats.len(), 16);
        assert!(stats.iter().all(|s| (s.minutes - 0.5).abs() < 1e-3));
        assert!(stats.iter().map(|s| s.collected).sum::<f32>() > 0.0);
        assert!(obs.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn same_seed_same_rollout_whatever_the_thread_schedule() {
        let roll = || {
            let mut env = VecEnv::new(mining(), 16, 9, 3, Rewards::default());
            let (total, _, obs) = run(&mut env, 400);
            (total.to_bits(), obs.iter().map(|v| v.to_bits() as u64).sum::<u64>())
        };
        assert_eq!(roll(), roll());
    }

    #[test]
    fn a_bought_upgrade_is_charged_once_per_decision() {
        let cfg = Config { rock_count: 0, ..mining() };
        let mut env = VecEnv::new(cfg, 1, 1, 3, Rewards::default());
        env.slots[0].world.ships[0].points = 100;
        let buy = replay::encode(&Action { buy: Some(swarm_sim::Buy::Upgrade(swarm_sim::Upgrade::Attack)), ..Action::default() });
        let (mut rewards, mut dones) = (vec![0f32; 4], vec![0u8; 4]);
        env.step(&[buy, 0, 0, 0], &mut rewards, &mut dones);
        assert_eq!(env.world(0).ships[0].levels[swarm_sim::Upgrade::Attack as usize], 1);
        assert_eq!(rewards[0], Rewards::default().buy);
    }

    #[test]
    fn scripted_teams_ignore_passed_actions_and_matches_end_with_a_winner() {
        let pay = Rewards { win: 10.0, lose: 10.0, base_hit: 0.1, ..Rewards::default() };
        // Weak, undefended bases and short matches, so scripted raids settle matches quickly.
        let cfg = Config { base_hp: 100.0, turret_damage: 0.0, max_ticks: 3000, ..Config::default() };
        let mut env = VecEnv::with_learners(cfg, 8, 3, 3, pay, 1);
        let agents = env.agents();
        let mut mask = vec![0u8; agents];
        env.controlled(&mut mask);
        // 3 teams of 7: the first 7 ships of each match are the learner's.
        assert_eq!(mask[..21].iter().map(|m| *m as usize).sum::<usize>(), 7);
        assert!(mask[..7].iter().all(|m| *m == 1));

        // The learner team does nothing at all; the scripted teams must still play and finish matches.
        let (actions, mut rewards, mut dones) = (vec![0u16; agents], vec![0f32; agents], vec![0u8; agents]);
        let mut stats = Vec::new();
        for _ in 0..1200 {
            env.step(&actions, &mut rewards, &mut dones);
            stats.extend(env.take_stats());
            if stats.len() >= 8 {
                break;
            }
        }
        assert!(stats.len() >= 8, "only {} matches ended", stats.len());
        assert!(stats.iter().all(|s| !(s.won && s.lost)));
        // The idle team bought nothing and stayed tier 1.
        assert!(stats.iter().all(|s| s.classes[0] == 1.0 && s.upgrades.iter().all(|l| *l == 0.0)));
    }

    #[test]
    fn time_costs_every_tick_and_a_match_without_a_winner_costs_more() {
        // Turrets off and nobody acting: no base falls, the match runs out of time.
        let cfg = Config { rock_count: 0, max_ticks: 90, turret_damage: 0.0, ..Config::default() };
        let pay = Rewards { time: 0.01, draw: 5.0, crash: 0.0, death: 0.0, ..Rewards::default() };
        let mut env = VecEnv::new(cfg, 1, 3, 3, pay);
        let agents = env.agents();
        let (actions, mut rewards, mut dones) = (vec![0u16; agents], vec![0f32; agents], vec![0u8; agents]);
        env.step(&actions, &mut rewards, &mut dones);
        assert!(rewards.iter().all(|r| (*r + 0.03).abs() < 1e-6), "three ticks at 0.01 each: {:?}", &rewards[..3]);
        for _ in 0..28 {
            env.step(&actions, &mut rewards, &mut dones);
        }
        assert!(dones.iter().all(|d| *d == 0));
        env.step(&actions, &mut rewards, &mut dones);
        assert!(dones.iter().all(|d| *d == 1));
        assert!(rewards.iter().all(|r| (*r + 5.03).abs() < 1e-5), "the draw is charged to everyone: {:?}", &rewards[..3]);
    }

    /// One ship a side, ship 0 pointing at ship 1 from 150 away, far from the bases; returns
    /// what ship 0 was paid by the time its first shot landed.
    fn first_hit_pay(pay: Rewards, attacking: bool, victim_hull: f32) -> f32 {
        let cfg = Config { teams: 2, ships_per_team: 1, rock_count: 0, turret_damage: 0.0, ..Config::default() };
        let mut env = VecEnv::new(cfg, 1, 3, 1, pay);
        let world = &mut env.slots[0].world;
        let pole = swarm_sim::Vec3::new(0.0, 0.0, 1.0);
        world.ships[1].pos = pole;
        world.ships[1].hull = victim_hull;
        world.ships[0].pos = world.offset(pole, 0.3, 150.0);
        world.face(0, pole);
        // Orders are given from outside in this test, so the ships' modes do not overwrite them.
        world.commanded = vec![true, true];
        if attacking {
            world.ships[0].order = swarm_sim::Order { kind: swarm_sim::OrderKind::Attack, target: pole };
        }
        let fire = replay::encode(&Action { fire: true, ..Action::default() });
        let (mut rewards, mut dones) = (vec![0f32; 2], vec![0u8; 2]);
        let mut paid = 0.0;
        for _ in 0..60 {
            env.step(&[fire, 0], &mut rewards, &mut dones);
            paid += rewards[0];
            if env.world(0).ships[1].shield < 40.0 {
                break;
            }
        }
        paid
    }

    #[test]
    fn attacking_with_the_upper_hand_pays_extra() {
        let pay = Rewards { hit: 1.0, edge: 3.0, crash: 0.0, death: 0.0, ..Rewards::default() };
        // The same single hit of a little under 1 damage in each case.
        let plain = first_hit_pay(pay, false, 100.0);
        let even = first_hit_pay(pay, true, 100.0);
        let weaker = first_hit_pay(pay, true, 50.0);
        assert!(plain > 0.5 && plain < 1.0, "a plain hit pays its damage: {plain}");
        assert!((even - plain).abs() < 1e-4, "attacking an equal pays no more: {even}");
        assert!((weaker - 4.0 * plain).abs() < 1e-3, "attacking a weaker enemy pays hit plus edge: {weaker}");
        // Farming ships get no bonus for shooting a weaker enemy either.
        assert!((first_hit_pay(pay, false, 50.0) - plain).abs() < 1e-4);
    }

    #[test]
    fn shooting_a_small_rock_pays_and_a_big_one_does_not() {
        let pay = Rewards { rock_hit: 2.0, rock: 0.0, crash: 0.0, death: 0.0, collect: 0.0, ..Rewards::default() };
        let paid_for = |size: u8| {
            let cfg = Config { teams: 1, ships_per_team: 1, rock_count: 0, ..Config::default() };
            let mut env = VecEnv::new(cfg, 1, 3, 1, pay);
            let world = &mut env.slots[0].world;
            let pole = swarm_sim::Vec3::new(0.0, 0.0, 1.0);
            world.ships[0].pos = pole;
            let spot = world.offset(pole, 0.3, 200.0);
            world.face(0, spot);
            world.rocks.push(swarm_sim::Rock { pos: spot, vel: swarm_sim::Vec3::default(), size, hp: 500.0, last_hit_by: None });
            let fire = replay::encode(&Action { fire: true, ..Action::default() });
            let (mut rewards, mut dones) = (vec![0f32; 1], vec![0u8; 1]);
            (0..60).map(|_| { env.step(&[fire], &mut rewards, &mut dones); rewards[0] }).sum::<f32>()
        };
        assert!(paid_for(1) > 1.0, "hits on a small rock pay: {}", paid_for(1));
        assert_eq!(paid_for(3), 0.0);
    }
}
