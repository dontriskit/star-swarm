//! How fast do scripted teams empty the farms around their base, and how far do they go for
//! crystals? Prints, minute by minute, for team 0: crystals handed over, the share of farms
//! with no rock standing within 2000, 4000 and 8000 of the base, and how far from home its
//! ships are on average.
//!
//!     cargo run --release -p swarm-sim --example fields [SEED] [REGROW_TICKS]

use swarm_sim::*;

fn main() {
    let mut args = std::env::args().skip(1);
    let seed: u64 = args.next().and_then(|a| a.parse().ok()).unwrap_or(1);
    let mut cfg = Config::default();
    if let Some(regrow) = args.next().and_then(|a| a.parse().ok()) {
        cfg.rock_regrow = regrow;
    }
    let mut world = World::new(cfg, seed);
    let mut bots: Vec<ScriptedBot> = (0..world.ships.len()).map(ScriptedBot::for_ship).collect();
    let home = world.bases[0].pos;
    let standing: u32 = world.rocks.iter().map(|r| (world.cfg.rock_packs[r.size as usize - 1].0 + world.cfg.rock_packs[r.size as usize - 1].1) / 2).sum();
    println!("{} farms, {} rocks, about {} crystals standing; regrow {} s", world.farms.len(), world.rocks.len(), standing, world.cfg.rock_regrow / 20);
    println!("min  handed-over  per-ship/min  empty<2000  empty<4000  empty<8000  mean-dist-from-home  base-lvl");
    let mut last = 0;
    while !world.done() {
        let actions: Vec<Action> = (0..world.ships.len()).map(|i| bots[i].act(&observe(&world, i), &world.cfg)).collect();
        world.step(&actions);
        if world.tick % 1200 == 0 {
            let empty = |within: f32| {
                let near: Vec<usize> = (0..world.farms.len()).filter(|f| world.dist(home, world.farms[*f].pos) < within).collect();
                format!("{:>3}/{:<3}", near.iter().filter(|f| world.farm_live[**f] == 0).count(), near.len())
            };
            let mine: Vec<&Ship> = world.ships.iter().filter(|s| s.team == 0 && s.alive).collect();
            let away = mine.iter().map(|s| world.dist(home, s.pos)).sum::<f32>() / mine.len().max(1) as f32;
            let handed = world.bases[0].deposited;
            println!(
                "{:>3}  {:>11}  {:>12.1}  {:>10}  {:>10}  {:>10}  {:>19.0}  {:>8}",
                world.tick / 1200,
                handed,
                (handed - last) as f32 / mine.len().max(1) as f32,
                empty(2000.0),
                empty(4000.0),
                empty(8000.0),
                away,
                world.bases[0].level
            );
            last = handed;
        }
    }
}
