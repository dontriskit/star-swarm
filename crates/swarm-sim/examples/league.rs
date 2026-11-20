//! Many matches of scripted bots under scripted commanders: who wins, how long matches
//! last, how far ships get. Usage: league [MATCHES]
use swarm_sim::*;

fn main() {
    let matches: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(100);
    let cfg = Config::default();
    let mut wins = vec![0u32; cfg.teams];
    let (mut draws, mut minutes, mut levels, mut tier2) = (0u32, 0.0f64, 0.0f64, 0u32);
    for seed in 1..=matches {
        let mut world = World::new(cfg.clone(), seed);
        let mut bot = ScriptedBot;
        let mut commander = ScriptedCommander::new(seed);
        while !world.done() {
            if world.tick % 450 == 0 {
                for team in 0..world.bases.len() as u8 {
                    commander.command(&mut world, team);
                }
            }
            let actions: Vec<Action> = (0..world.ships.len()).map(|i| bot.act(&observe(&world, i), &world.cfg)).collect();
            world.step(&actions);
        }
        match world.winner() {
            Some(team) => wins[team as usize] += 1,
            None => draws += 1,
        }
        minutes += world.tick as f64 * cfg.dt as f64 / 60.0;
        levels += world.ships.iter().flat_map(|s| s.levels).map(f64::from).sum::<f64>() / world.ships.len() as f64;
        tier2 += world.ships.iter().filter(|s| s.tier == 2).count() as u32;
    }
    let m = matches as f64;
    println!("{matches} matches: wins per team {wins:?}, no winner in time {draws}");
    println!("  {:.1} min per match, {:.1} upgrade levels per ship at the end, {:.1} tier-2 ships per match", minutes / m, levels / m, tier2 as f64 / m);
}
