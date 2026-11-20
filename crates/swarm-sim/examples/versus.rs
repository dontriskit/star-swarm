//! Full matches where team 0 flies on a trained policy, with no orders, and the other
//! teams on the scripted bot under scripted commanders. Then the same matches all-scripted.
//! Usage: versus POLICY.swp [MATCHES]
use std::sync::Arc;
use swarm_sim::*;

fn main() -> std::io::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: versus POLICY.swp [MATCHES]");
    let matches: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(20);
    let net = Arc::new(Net::load(&path)?);

    for pilot in ["model", "scripted bot (control)"] {
        let cfg = Config::default();
        // Per team: crystals, upgrade levels, kills, deaths, wins.
        let mut totals = vec![[0f64; 5]; cfg.teams];
        let mut minutes = 0.0;
        for seed in 1..=matches {
            let mut world = World::new(cfg.clone(), seed);
            let mut commander = ScriptedCommander::new(seed);
            let mut pilots: Vec<Box<dyn Controller>> = world
                .ships
                .iter()
                .map(|s| match (s.team == 0, pilot) {
                    (true, "model") => Box::new(ModelBot::new(net.clone(), ModelBot::HOLD)) as Box<dyn Controller>,
                    _ => Box::new(ScriptedBot),
                })
                .collect();
            while !world.done() {
                if world.tick % 450 == 0 {
                    for team in (if pilot == "model" { 1 } else { 0 })..world.bases.len() as u8 {
                        commander.command(&mut world, team);
                    }
                }
                let actions: Vec<Action> = (0..world.ships.len()).map(|i| pilots[i].act(&observe(&world, i), &world.cfg)).collect();
                world.step(&actions);
            }
            for s in &world.ships {
                let t = &mut totals[s.team as usize];
                t[0] += s.collected as f64;
                t[1] += s.levels.iter().map(|l| *l as f64).sum::<f64>();
                t[2] += s.kills as f64;
                t[3] += s.deaths as f64;
            }
            if let Some(team) = world.winner() {
                totals[team as usize][4] += 1.0;
            }
            minutes += world.tick as f64 * cfg.dt as f64 / 60.0;
        }
        let minutes = minutes / matches as f64;
        let ships = (matches * cfg.ships_per_team as u64) as f64;
        println!("{matches} matches of {minutes:.1} min on average, team 0 flown by the {pilot}:");
        for (team, t) in totals.iter().enumerate() {
            println!(
                "  team {team}: won {:3.0}%, {:5.1} crystals per ship per minute, {:4.1} upgrade levels, {:4.1} kills, {:4.1} deaths per ship",
                100.0 * t[4] / matches as f64,
                t[0] / ships / minutes,
                t[1] / ships,
                t[2] / ships,
                t[3] / ships
            );
        }
    }
    Ok(())
}
