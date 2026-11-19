use swarm_sim::*;

fn run(world: &mut World, ticks: u32) {
    let mut bot = ScriptedBot;
    for _ in 0..ticks {
        let actions: Vec<Action> = (0..world.ships.len()).map(|i| bot.act(&observe(world, i), &world.cfg)).collect();
        world.step(&actions);
    }
}

#[test]
fn same_seed_same_match() {
    let mut a = World::new(Config::default(), 7);
    let mut b = World::new(Config::default(), 7);
    for _ in 0..20 {
        run(&mut a, 100);
        run(&mut b, 100);
        assert_eq!(a.state_hash(), b.state_hash(), "diverged by tick {}", a.tick);
    }
}

#[test]
fn different_seed_different_match() {
    let mut a = World::new(Config::default(), 7);
    let mut b = World::new(Config::default(), 8);
    run(&mut a, 100);
    run(&mut b, 100);
    assert_ne!(a.state_hash(), b.state_hash());
}

#[test]
fn scripted_bots_mine_and_upgrade() {
    let mut world = World::new(Config::default(), 1);
    run(&mut world, 3000);
    let collected: u32 = world.ships.iter().map(|s| s.collected).sum();
    let levels: u32 = world.ships.iter().flat_map(|s| s.levels).map(u32::from).sum();
    assert!(collected > 100, "collected only {collected}");
    assert!(levels > 10, "bought only {levels} upgrade levels");
}

#[test]
fn observation_vector_has_fixed_length_and_is_finite() {
    let mut world = World::new(Config::default(), 3);
    run(&mut world, 500);
    for i in 0..world.ships.len() {
        let v = observe(&world, i).to_vec(&world.cfg);
        assert_eq!(v.len(), OBS_LEN);
        assert!(v.iter().all(|x| x.is_finite()));
    }
}

#[test]
fn regroup_order_gathers_the_team() {
    let cfg = Config { rock_count: 0, ..Config::default() };
    let mut world = World::new(cfg, 5);
    let target = Vec2::new(1200.0, 1200.0);
    let team: Vec<usize> = (0..world.ships.len()).filter(|i| world.ships[*i].team == 0).collect();
    for &i in &team {
        world.set_order(i, Order { kind: OrderKind::Regroup, target });
    }
    let spread = |w: &World| team.iter().map(|&i| w.delta(w.ships[i].pos, target).len()).sum::<f32>() / team.len() as f32;
    let before = spread(&world);
    run(&mut world, 600);
    let after = spread(&world);
    assert!(after < 200.0 && after < before / 2.0, "mean distance {before} -> {after}");
}

#[test]
fn buying_spends_cargo_and_stops_at_the_cap() {
    let cfg = Config { rock_count: 0, ..Config::default() };
    let mut world = World::new(cfg, 2);
    world.ships[0].cargo = 1000;
    let mut actions = vec![Action::default(); world.ships.len()];
    actions[0].buy = Some(Upgrade::Damage);
    for _ in 0..10 {
        world.step(&actions);
    }
    assert_eq!(world.ships[0].levels[Upgrade::Damage as usize], 3);
    assert_eq!(world.ships[0].cargo, 1000 - (10 + 20 + 30));
}
