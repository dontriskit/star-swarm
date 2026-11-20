use swarm_sim::replay::{decode, encode};
use swarm_sim::*;

#[test]
fn every_action_survives_encoding() {
    let buys: Vec<Option<Buy>> = std::iter::once(None)
        .chain(UPGRADES.iter().copied().map(Buy::Upgrade).chain(TIER2.iter().copied().map(Buy::Tier)).map(Some))
        .collect();
    let votes: Vec<Option<Plan>> = std::iter::once(None).chain(PLANS.iter().copied().map(Some)).collect();
    for thrust in [false, true] {
        for turn in [-1, 0, 1] {
            for (fire, boost) in [(false, false), (false, true), (true, false), (true, true)] {
                for &buy in &buys {
                    for &vote in &votes {
                        let a = Action { thrust, turn, fire, boost, buy, vote };
                        assert_eq!(decode(encode(&a)), a);
                    }
                }
            }
        }
    }
}

#[test]
fn saved_replay_reproduces_the_match() {
    let seed = 11;
    let mut world = World::new(Config::default(), seed);
    let mut replay = Replay::new(seed, &world.cfg);
    let mut bot = ScriptedBot::default();
    let mut commander = ScriptedCommander::new(seed);
    for tick in 0..1500 {
        if tick % 450 == 0 {
            for team in 0..world.bases.len() as u8 {
                commander.command(&mut world, team);
            }
        }
        let actions: Vec<Action> = (0..world.ships.len()).map(|i| bot.act(&observe(&world, i), &world.cfg)).collect();
        world.step(&actions);
        replay.record(&world, &actions);
    }

    let path = std::env::temp_dir().join(format!("star-swarm-test-{}.replay", std::process::id()));
    replay.save(&path).unwrap();
    let loaded = Replay::load(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(loaded, replay);
    assert_eq!(loaded.ticks(), 1500);

    // Playback needs no bots and no orders: the recorded actions are enough.
    let mut again = loaded.world();
    while let Some(actions) = loaded.actions(again.tick) {
        again.step(&actions);
        if let Some(hash) = loaded.hash_at(again.tick) {
            assert_eq!(again.state_hash(), hash, "diverged at tick {}", again.tick);
        }
    }
    assert_eq!(again.state_hash(), world.state_hash());
}
