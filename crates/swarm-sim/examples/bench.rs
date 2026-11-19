//! One full match of scripted bots: prints throughput and what the bots achieved.
use std::time::Instant;
use swarm_sim::*;

fn main() {
    let seed = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let mut world = World::new(Config::default(), seed);
    let mut bot = ScriptedBot;
    let start = Instant::now();
    while !world.done() {
        let actions: Vec<Action> = (0..world.ships.len()).map(|i| bot.act(&observe(&world, i), &world.cfg)).collect();
        world.step(&actions);
    }
    let secs = start.elapsed().as_secs_f64();
    let minutes = world.tick as f32 * world.cfg.dt / 60.0;
    let n = world.ships.len() as f32;
    let sum = |f: fn(&Ship) -> u32| world.ships.iter().map(f).sum::<u32>();
    println!("seed {seed}: {} ticks, {} ships, {} rocks", world.tick, world.ships.len(), world.rocks.len());
    println!("  {:.0} steps/s ({:.0} ship-steps/s), observations and bots included", world.tick as f64 / secs, world.tick as f64 * n as f64 / secs);
    println!("  crystals per ship per minute: {:.1}", sum(|s| s.collected) as f32 / n / minutes);
    println!("  upgrade levels per ship: {:.1} of {}", sum(|s| s.levels.iter().map(|l| *l as u32).sum()) as f32 / n, 8 * world.cfg.upgrade_max as u32);
    println!("  kills {}, deaths {} (crashes and kills)", sum(|s| s.kills), sum(|s| s.deaths));
    println!("  state hash {:016x}", world.state_hash());
}
