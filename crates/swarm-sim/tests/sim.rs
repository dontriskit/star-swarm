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
    // Away from every base: turrets shoot ships that gather next to an enemy base.
    let target = Vec2::new(1800.0, 600.0);
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
fn buying_spends_cargo_and_stops_at_the_tier_cap() {
    let cfg = Config { rock_count: 0, ..Config::default() };
    let mut world = World::new(cfg, 2);
    world.ships[0].cargo = 1000;
    let mut actions = vec![Action::default(); world.ships.len()];
    actions[0].buy = Some(Buy::Upgrade(Upgrade::Attack));
    for _ in 0..10 {
        world.step(&actions);
    }
    // Tier 1 stops at level 3: 10 + 20 + 30 crystals.
    assert_eq!(world.ships[0].levels[Upgrade::Attack as usize], 3);
    assert_eq!(world.ships[0].cargo, 1000 - 60);

    actions[0].buy = Some(Buy::Tier);
    world.step(&actions);
    world.step(&actions);
    assert_eq!(world.ships[0].tier, 2);
    assert_eq!(world.ships[0].cargo, 1000 - 60 - 60, "the tier is paid for once");
    assert_eq!(world.ships[0].hull, world.cfg.hull_max(&world.ships[0]));
    assert!(world.cfg.hull_max(&world.ships[0]) > world.cfg.hull);

    // Tier 2 raises the cap to level 5: 40 + 50 more crystals.
    actions[0].buy = Some(Buy::Upgrade(Upgrade::Attack));
    for _ in 0..10 {
        world.step(&actions);
    }
    assert_eq!(world.ships[0].levels[Upgrade::Attack as usize], 5);
    assert_eq!(world.ships[0].cargo, 1000 - 60 - 60 - 90);
}

/// Two teams, no rocks; ship 0 (team 0) sits in front of team 1's base, pointing at it.
fn siege(cfg: Config) -> World {
    let cfg = Config { teams: 2, ships_per_team: 1, rock_count: 0, ..cfg };
    let mut world = World::new(cfg, 4);
    let target = world.bases[1].pos;
    world.ships[0].pos = world.wrap(Vec2::new(target.x - 200.0, target.y));
    world.ships[0].heading = 0.0;
    // Park the defender far away so it neither blocks shots nor respawns into them.
    world.ships[1].pos = world.wrap(Vec2::new(target.x, target.y + 900.0));
    world
}

#[test]
fn destroying_every_other_base_wins_the_match() {
    // An undefended base of 100 HP falls to ten shots.
    let mut world = siege(Config { base_hp: 100.0, turret_damage: 0.0, ..Config::default() });
    assert_eq!(world.bases[1].hp, 100.0);
    let fire = [Action { fire: true, ..Action::default() }, Action::default()];
    let mut destroyed = false;
    while !world.done() {
        destroyed |= world.step(&fire).iter().any(|e| matches!(e, Event::BaseDestroyed { team: 1, by: 0 }));
    }
    assert!(destroyed);
    assert_eq!(world.winner(), Some(0));
    assert!(world.tick < 600, "ten shots of 10 should not take {} ticks", world.tick);
}

#[test]
fn a_team_without_a_base_gets_no_new_ships() {
    let mut world = siege(Config::default());
    // A third team keeps the match going after team 1 loses its base.
    world.bases.push(Base { pos: Vec2::new(100.0, 100.0), hp: 100.0, cooldown: 0 });
    world.bases[1].hp = 0.0;
    world.ships[1].alive = false;
    world.ships[1].respawn = 1;
    for _ in 0..200 {
        world.step(&[Action::default(); 2]);
    }
    assert!(!world.ships[1].alive);
    assert_eq!(world.winner(), None);
}

#[test]
fn a_turret_destroys_a_lone_basic_ship_before_the_base_falls() {
    let mut world = siege(Config::default());
    let fire = [Action { fire: true, ..Action::default() }, Action::default()];
    let mut killed_by_turret = false;
    for _ in 0..600 {
        killed_by_turret |= world.step(&fire).iter().any(|e| matches!(e, Event::Killed { by: None, victim: 0 }));
        if killed_by_turret {
            break;
        }
    }
    assert!(killed_by_turret, "the attacker survived 20 s in front of a turret");
    assert!(world.bases[1].alive());
    assert!(world.bases[1].hp < world.cfg.base_hp, "the attacker should have landed some shots first");
    assert_eq!(world.winner(), None);
}

#[test]
fn shields_take_the_damage_first_and_grow_back() {
    let cfg = Config { teams: 2, ships_per_team: 1, rock_count: 0, turret_damage: 0.0, ..Config::default() };
    let mut world = World::new(cfg, 6);
    let at = world.ships[1].pos;
    world.ships[0].pos = world.wrap(Vec2::new(at.x - 150.0, at.y));
    world.ships[0].heading = 0.0;
    let mut actions = [Action { fire: true, ..Action::default() }, Action::default()];
    // One shot of 10 against 40 shield.
    while !world.step(&actions).iter().any(|e| matches!(e, Event::Hit { victim: 1, .. })) {}
    actions[0].fire = false;
    assert_eq!(world.ships[1].hull, 100.0);
    assert!(world.ships[1].shield <= 30.0);
    // Let shots already in flight land or expire, then wait out the delay.
    for _ in 0..40 {
        world.step(&actions);
    }
    let low = world.ships[1].shield;
    for _ in 0..400 {
        world.step(&actions);
    }
    assert!(world.ships[1].shield > low);
    assert_eq!(world.ships[1].shield, world.cfg.shield_max(&world.ships[1]));
}
