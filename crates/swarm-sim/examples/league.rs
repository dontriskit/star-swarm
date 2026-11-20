//! Many matches of scripted bots under scripted commanders: who wins, how long matches
//! last, how far ships and bases get. Usage: league [MATCHES]
use swarm_sim::*;

fn main() {
    let matches: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(100);
    let cfg = Config::default();
    let mut wins = vec![0u32; cfg.teams];
    let (mut draws, mut minutes, mut levels) = (0u32, 0.0f64, 0.0f64);
    let mut classes = [0u32; 5];
    let mut per_upgrade = [0f64; 8];
    let (mut ships, mut collected, mut deposited, mut drones, mut base_levels, mut tiers) = (0u32, 0u64, 0u64, 0usize, 0u32, [0u32; 3]);
    for seed in 1..=matches {
        let mut world = World::new(cfg.clone(), seed);
        let mut bots: Vec<ScriptedBot> = (0..world.ships.len()).map(ScriptedBot::for_ship).collect();
        let mut commander = ScriptedCommander::new(seed);
        while !world.done() {
            if world.tick % 450 == 0 {
                for team in 0..world.bases.len() as u8 {
                    commander.command(&mut world, team);
                }
            }
            let actions: Vec<Action> = (0..world.ships.len()).map(|i| bots[i].act(&observe(&world, i), &world.cfg)).collect();
            world.step(&actions);
        }
        match world.winner() {
            Some(team) => wins[team as usize] += 1,
            None => draws += 1,
        }
        minutes += world.tick as f64 * cfg.dt as f64 / 60.0;
        levels += world.ships.iter().flat_map(|s| s.levels).map(f64::from).sum::<f64>() / world.ships.len() as f64;
        for s in &world.ships {
            classes[s.class as usize] += 1;
            tiers[s.tier as usize - 1] += 1;
            s.levels.iter().enumerate().for_each(|(u, l)| per_upgrade[u] += *l as f64);
            collected += s.collected as u64;
            deposited += s.deposited as u64;
            ships += 1;
        }
        drones += world.drones.len();
        base_levels += world.bases.iter().map(|b| b.level as u32).sum::<u32>();
    }
    let m = matches as f64;
    let per_ship_minute = |total: u64| total as f64 / ships as f64 / (minutes / m);
    println!("{matches} matches: wins per team {wins:?}, no winner in time {draws}");
    println!("  {:.1} min per match, {:.1} upgrade levels per ship at the end", minutes / m, levels / m);
    println!("  per ship per minute: {:.1} crystals collected, {:.1} handed over at the base", per_ship_minute(collected), per_ship_minute(deposited));
    println!("  base level at the end {:.2}, drones per ship {:.2}, ships by tier {:?}", base_levels as f64 / (m * cfg.teams as f64), drones as f64 / ships as f64, tiers.map(|n| format!("{:.0}%", 100.0 * n as f64 / ships as f64)));
    println!("  ships at the end by class: {:?}", CLASSES.iter().zip(classes).map(|(c, n)| format!("{c:?} {:.0}%", 100.0 * n as f64 / ships as f64)).collect::<Vec<_>>());
    println!("  mean level per upgrade: {:?}", UPGRADES.iter().zip(per_upgrade).map(|(u, l)| format!("{u:?} {:.1}", l / ships as f64)).collect::<Vec<_>>());
}
