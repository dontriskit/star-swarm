//! Plays one match of scripted bots under scripted commanders and saves it as a replay.
//! Usage: record [SEED] [FILE]
use swarm_sim::*;

fn main() -> std::io::Result<()> {
    let mut args = std::env::args().skip(1);
    let seed = args.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let path = args.next().unwrap_or_else(|| format!("star-swarm-{seed}.replay"));
    let mut world = World::new(Config::default(), seed);
    let mut replay = Replay::new(seed, world.ships.len());
    let mut bot = ScriptedBot;
    let mut commander = ScriptedCommander::new(seed);
    let mut raids = 0;
    while !world.done() {
        if world.tick % 450 == 0 {
            for team in 0..world.bases.len() as u8 {
                raids += commander.command(&mut world, team);
            }
        }
        let actions: Vec<Action> = (0..world.ships.len()).map(|i| bot.act(&observe(&world, i), &world.cfg)).collect();
        world.step(&actions);
        replay.record(&world, &actions);
    }
    replay.save(&path)?;
    println!("{path}: {} ticks, {raids} raid orders, final state hash {:016x}", replay.ticks(), world.state_hash());
    Ok(())
}
