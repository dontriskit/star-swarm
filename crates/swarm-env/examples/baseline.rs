//! The scripted bot on the evaluation matches of train/ppo.py (mining setup, seed 1,000,000),
//! holding each action for FRAME_SKIP ticks. Usage: baseline [FRAME_SKIP] [MATCHES]
use swarm_env::{Rewards, VecEnv};
use swarm_sim::Config;

fn main() {
    let mut args = std::env::args().skip(1);
    let frame_skip: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(3);
    let matches: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(256);
    let cfg = Config { teams: 1, ships_per_team: 4, rock_count: 40, world_size: 1400.0, max_ticks: 5400, ..Config::default() };
    let mut env = VecEnv::new(cfg, matches, 1_000_000, frame_skip, Rewards::default());
    let agents = env.agents();
    let (mut actions, mut rewards, mut dones) = (vec![0u16; agents], vec![0f32; agents], vec![0u8; agents]);
    let mut stats = Vec::new();
    while stats.len() < matches {
        env.scripted(&mut actions);
        env.step(&actions, &mut rewards, &mut dones);
        stats.extend(env.take_stats());
    }
    // The first match of every slot, so short matches are not counted more often than long ones.
    stats.retain(|s| s.episode == 0);
    let mean = |f: fn(&swarm_env::EpisodeStats) -> f32| stats.iter().map(f).sum::<f32>() / matches as f32;
    println!(
        "frame skip {frame_skip}, {matches} matches: {:.1} crystals per ship per minute, {:.2} upgrade levels, {:.3} deaths",
        mean(|s| s.collected) / mean(|s| s.minutes),
        mean(|s| s.levels),
        mean(|s| s.deaths)
    );
}
