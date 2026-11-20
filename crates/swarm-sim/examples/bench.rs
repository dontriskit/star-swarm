//! One full match of scripted bots: prints throughput and what the bots achieved.
use std::time::Instant;
use swarm_sim::*;

fn main() {
    let seed = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let mut world = World::new(Config::default(), seed);
    let mut bot = ScriptedBot;
    let mut commander = ScriptedCommander::new(seed);
    let start = Instant::now();
    while !world.done() {
        if world.tick % 450 == 0 {
            for team in 0..world.bases.len() as u8 {
                commander.command(&mut world, team);
            }
        }
        let actions: Vec<Action> = (0..world.ships.len()).map(|i| bot.act(&observe(&world, i), &world.cfg)).collect();
        world.step(&actions);
    }
    let secs = start.elapsed().as_secs_f64();
    let minutes = world.tick as f32 * world.cfg.dt / 60.0;
    println!("  match length: {:.1} min", minutes);
    let n = world.ships.len() as f32;
    let sum = |f: fn(&Ship) -> u32| world.ships.iter().map(f).sum::<u32>();
    println!("seed {seed}: {} ticks, {} ships, {} rocks", world.tick, world.ships.len(), world.rocks.len());
    println!("  {:.0} steps/s ({:.0} ship-steps/s), observations and bots included", world.tick as f64 / secs, world.tick as f64 * n as f64 / secs);
    println!("  crystals per ship per minute: {:.1}", sum(|s| s.collected) as f32 / n / minutes);
    println!("  upgrade levels per ship: {:.1} of {}, ships at tier 2: {}", sum(|s| s.levels.iter().map(|l| *l as u32).sum()) as f32 / n, 6 * world.cfg.top_level() as u32, world.ships.iter().filter(|s| s.tier == 2).count());
    println!("  kills {}, deaths {} (crashes and kills)", sum(|s| s.kills), sum(|s| s.deaths));
    println!("  winner: {:?}, base hp {:?}", world.winner(), world.bases.iter().map(|b| b.hp.max(0.0)).collect::<Vec<_>>());
    println!("  state hash {:016x}", world.state_hash());
}
