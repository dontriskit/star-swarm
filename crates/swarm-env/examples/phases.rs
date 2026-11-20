//! Times the two things a rollout step asks of the environment, on their own: building the
//! observations and stepping the matches. Usage: phases [MATCHES]
use std::time::Instant;
use swarm_env::{Rewards, VecEnv};
use swarm_sim::{Config, OBS_LEN};

fn main() {
    let envs: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(2048);
    let mut env = VecEnv::new(Config::default(), envs, 5, 3, Rewards::default());
    let agents = env.agents();
    let (mut actions, mut rewards, mut dones) = (vec![0u16; agents], vec![0f32; agents], vec![0u8; agents]);
    let mut obs = vec![0f32; agents * OBS_LEN];
    // Get the matches going, so there are shots, crystals and drones about.
    for _ in 0..200 {
        env.scripted(&mut actions);
        env.step(&actions, &mut rewards, &mut dones);
    }
    let rounds = 20;
    let (mut observe, mut step, mut script) = (0.0, 0.0, 0.0);
    for _ in 0..rounds {
        let t = Instant::now();
        env.observe(&mut obs);
        observe += t.elapsed().as_secs_f64();
        let t = Instant::now();
        env.scripted(&mut actions);
        script += t.elapsed().as_secs_f64();
        let t = Instant::now();
        env.step(&actions, &mut rewards, &mut dones);
        step += t.elapsed().as_secs_f64();
    }
    let ms = |total: f64| 1000.0 * total / rounds as f64;
    println!("{envs} matches x {} ships, {} threads", env.ships(), rayon::current_num_threads());
    println!("  observe {:6.1} ms   ({:.2} us per ship per core)", ms(observe), 1e6 * observe / rounds as f64 * rayon::current_num_threads() as f64 / agents as f64);
    println!("  step    {:6.1} ms   (3 ticks)", ms(step));
    println!("  scripted bots {:6.1} ms (not part of training against itself)", ms(script));
}
