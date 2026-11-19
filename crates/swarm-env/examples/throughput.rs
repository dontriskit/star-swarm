//! Steps a batch of mining matches with scripted actions and prints agent-steps per second.
use std::time::Instant;
use swarm_env::{Rewards, VecEnv};
use swarm_sim::{Config, OBS_LEN};

fn main() {
    let envs: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(256);
    let cfg = Config { teams: 1, ships_per_team: 4, rock_count: 40, world_size: 1400.0, max_ticks: 1800, ..Config::default() };
    let frame_skip = 3;
    let mut env = VecEnv::new(cfg, envs, 1, frame_skip, Rewards::default());
    let agents = env.agents();
    let (mut actions, mut rewards, mut dones) = (vec![0u8; agents], vec![0f32; agents], vec![0u8; agents]);
    let mut obs = vec![0f32; agents * OBS_LEN];
    let steps = 600;
    let start = Instant::now();
    for _ in 0..steps {
        env.observe(&mut obs);
        env.scripted(&mut actions);
        env.step(&actions, &mut rewards, &mut dones);
    }
    let secs = start.elapsed().as_secs_f64();
    println!("{envs} matches x {} ships, {} threads", env.ships(), rayon::current_num_threads());
    println!("  {:.0} decisions/s per agent batch, {:.0} agent-decisions/s, {:.0} agent-ticks/s", steps as f64 / secs, (steps * agents) as f64 / secs, (steps * agents) as f64 * frame_skip as f64 / secs);
}
