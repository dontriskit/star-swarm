use swarm_sim::*;

fn run(world: &mut World, ticks: u32) {
    let mut bot = ScriptedBot::default();
    for _ in 0..ticks {
        let actions: Vec<Action> = (0..world.ships.len()).map(|i| bot.act(&observe(world, i), &world.cfg)).collect();
        world.step(&actions);
    }
}

/// A point far from every base of a two- or three-team match: the north pole.
const POLE: Vec3 = Vec3::new(0.0, 0.0, 1.0);

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
fn everything_stays_on_the_planet() {
    let mut world = World::new(Config::default(), 3);
    run(&mut world, 1500);
    let on_surface = |p: Vec3| (p.len() - 1.0).abs() < 1e-4;
    assert!(world.ships.iter().all(|s| on_surface(s.pos)));
    assert!(world.rocks.iter().all(|r| on_surface(r.pos)));
    assert!(world.shots.iter().all(|s| on_surface(s.pos)));
    // Velocities and headings stay tangent to the surface, and the heading stays a unit vector.
    for s in &world.ships {
        assert!(s.vel.dot(s.pos).abs() < 0.05, "velocity leaves the surface: {:?}", s.vel);
        assert!(s.fwd.dot(s.pos).abs() < 1e-3 && (s.fwd.len() - 1.0).abs() < 1e-3, "heading {:?}", s.fwd);
    }
}

#[test]
fn bases_are_equally_far_apart() {
    let world = World::new(Config::default(), 1);
    let d01 = world.dist(world.bases[0].pos, world.bases[1].pos);
    let d12 = world.dist(world.bases[1].pos, world.bases[2].pos);
    let d20 = world.dist(world.bases[2].pos, world.bases[0].pos);
    // A third of the way around a planet of radius 1500.
    assert!((d01 - 3141.6).abs() < 1.0, "{d01}");
    assert!((d01 - d12).abs() < 1.0 && (d12 - d20).abs() < 1.0);
}

#[test]
fn a_ship_flying_straight_comes_back_around_the_planet() {
    let cfg = Config { teams: 1, ships_per_team: 1, rock_count: 0, radius: 300.0, ..Config::default() };
    let mut world = World::new(cfg, 1);
    let start = world.ships[0].pos;
    let go = [Action { thrust: true, ..Action::default() }];
    let mut farthest = 0.0f32;
    let mut back = None;
    for tick in 0..600 {
        world.step(&go);
        let away = world.dist(start, world.ships[0].pos);
        farthest = farthest.max(away);
        if tick > 100 && away < 30.0 {
            back = Some(tick);
            break;
        }
    }
    // Once around is 1885 units; at about 240 a second that is roughly 8 s after getting up to speed.
    assert!(farthest > 900.0, "only got {farthest} away, half-way around is 942");
    assert!(back.is_some_and(|t| (220..330).contains(&t)), "back at the start after {back:?} ticks");
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
fn what_a_ship_sees_is_in_its_own_frame() {
    let cfg = Config { teams: 2, ships_per_team: 1, rock_count: 0, turret_damage: 0.0, ..Config::default() };
    let mut world = World::new(cfg, 1);
    world.ships[0].pos = POLE;
    // The other ship sits 200 away; the observer first faces it, then turns a quarter to the left.
    world.ships[1].pos = world.offset(POLE, 0.7, 200.0);
    world.face(0, world.ships[1].pos);
    let ahead = observe(&world, 0).ships[0];
    assert!((ahead.pos.x - 200.0).abs() < 0.5 && ahead.pos.y.abs() < 0.5, "{:?}", ahead.pos);
    assert!((ahead.dist - 200.0).abs() < 0.5);

    world.step(&[Action { turn: 1, ..Action::default() }, Action::default()]);
    let after = observe(&world, 0).ships[0];
    assert!(after.pos.y < -10.0, "after turning left the target is on the right: {:?}", after.pos);
}

#[test]
fn regroup_order_gathers_the_team() {
    let cfg = Config { rock_count: 0, ..Config::default() };
    let mut world = World::new(cfg, 5);
    // 700 from the team's base, far from every other base.
    let target = world.offset(world.bases[0].pos, 1.6, 700.0);
    let team: Vec<usize> = (0..world.ships.len()).filter(|i| world.ships[*i].team == 0).collect();
    for &i in &team {
        world.set_order(i, Order { kind: OrderKind::Regroup, target });
    }
    let spread = |w: &World| team.iter().map(|&i| w.dist(w.ships[i].pos, target)).sum::<f32>() / team.len() as f32;
    let before = spread(&world);
    run(&mut world, 600);
    let after = spread(&world);
    assert!(after < 200.0 && after < before / 2.0, "mean distance {before} -> {after}");
}

#[test]
fn each_tier_has_a_higher_upgrade_cap_and_starts_again_without_upgrades() {
    let cfg = Config { rock_count: 0, ..Config::default() };
    let mut world = World::new(cfg, 2);
    world.ships[0].cargo = 1000;
    let mut actions = vec![Action::default(); world.ships.len()];
    let buy = |world: &mut World, actions: &mut Vec<Action>, what: Buy, times: usize| {
        actions[0].buy = Some(what);
        for _ in 0..times {
            world.step(actions);
        }
    };

    // Tier 1 stops at level 3: 10 + 20 + 30 crystals.
    buy(&mut world, &mut actions, Buy::Upgrade(Upgrade::Attack), 10);
    assert_eq!(world.ships[0].levels[Upgrade::Attack as usize], 3);
    assert_eq!(world.ships[0].cargo, 1000 - 60);
    let full_tier1 = world.cfg.damage(&world.ships[0]);

    // Tier 2: paid for once, the class is chosen, the upgrades are gone.
    buy(&mut world, &mut actions, Buy::Tier(Class::Twin), 1);
    assert_eq!((world.ships[0].tier, world.ships[0].class), (2, Class::Twin));
    assert_eq!(world.ships[0].levels, [0; 8]);
    assert_eq!(world.ships[0].cargo, 1000 - 60 - 60);
    assert_eq!(world.ships[0].hull, world.cfg.hull_max(&world.ships[0]));

    // Tier 2 stops at level 4, tier 3 at level 5, and tier 3 keeps the class.
    buy(&mut world, &mut actions, Buy::Upgrade(Upgrade::Attack), 10);
    assert_eq!(world.ships[0].levels[Upgrade::Attack as usize], 4);
    world.ships[0].cargo = 1000;
    buy(&mut world, &mut actions, Buy::Tier(Class::Lancer), 1);
    assert_eq!((world.ships[0].tier, world.ships[0].class, world.ships[0].cargo), (3, Class::Twin, 1000 - 120));
    buy(&mut world, &mut actions, Buy::Upgrade(Upgrade::Attack), 10);
    assert_eq!(world.ships[0].levels[Upgrade::Attack as usize], 5);
    buy(&mut world, &mut actions, Buy::Tier(Class::Twin), 3);
    assert_eq!(world.ships[0].tier, 3, "there is no tier 4");
    let _ = full_tier1;
}

#[test]
fn a_fresh_ship_of_the_next_tier_is_a_little_worse_than_a_full_one_below() {
    let world = World::new(Config { rock_count: 0, ..Config::default() }, 2);
    let cfg = &world.cfg;
    let ship = |tier: u8, level: u8| {
        let mut s = world.ships[0].clone();
        s.tier = tier;
        s.levels = [level; 8];
        s
    };
    for tier in 1..TIERS {
        let (full, fresh, upgraded) = (ship(tier, cfg.max_level(tier)), ship(tier + 1, 0), ship(tier + 1, cfg.max_level(tier + 1)));
        for stat in [Config::hull_max, Config::shield_max, Config::ammo_max, Config::turn_rate_of] {
            let (a, b, c) = (stat(cfg, &full), stat(cfg, &fresh), stat(cfg, &upgraded));
            assert!(b < a && b > a * 0.9, "tier {} fresh {b} against tier {tier} full {a}", tier + 1);
            assert!(c > a, "tier {} upgraded {c} against tier {tier} full {a}", tier + 1);
        }
    }
    // Bigger ships take up more room.
    assert!(cfg.radius_of(&ship(3, 0)) > cfg.radius_of(&ship(2, 0)) && cfg.radius_of(&ship(2, 0)) > cfg.radius_of(&ship(1, 0)));
}

/// Two teams, no rocks; ship 0 (team 0) sits 200 in front of team 1's base, pointing at it.
fn siege(cfg: Config) -> World {
    let cfg = Config { teams: 2, ships_per_team: 1, rock_count: 0, ..cfg };
    let mut world = World::new(cfg, 4);
    let target = world.bases[1].pos;
    world.ships[0].pos = world.offset(target, 0.0, 200.0);
    world.face(0, target);
    // Park the defender far away so it neither blocks shots nor respawns into them.
    world.ships[1].pos = POLE;
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
    assert!(world.tick < 900, "a 100 HP base should not take {} ticks", world.tick);
}

#[test]
fn a_team_without_a_base_gets_no_new_ships() {
    let mut world = siege(Config::default());
    // A third team keeps the match going after team 1 loses its base.
    world.bases.push(Base { pos: Vec3::new(0.0, 0.0, -1.0), hp: 100.0, cooldown: 0 });
    world.plans.push(None);
    world.commanded.push(false);
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

/// Two ships of opposing teams near the north pole, `gap` apart, ship 0 pointing at ship 1;
/// no rocks, no turrets, no base anywhere near.
fn duel(cfg: Config, gap: f32) -> World {
    let cfg = Config { teams: 2, ships_per_team: 1, rock_count: 0, turret_damage: 0.0, ..cfg };
    let mut world = World::new(cfg, 6);
    world.ships[1].pos = POLE;
    world.ships[0].pos = world.offset(POLE, 0.3, gap);
    world.face(0, POLE);
    world
}

#[test]
fn shields_take_the_damage_first_and_grow_back() {
    let mut world = duel(Config::default(), 150.0);
    let mut actions = [Action { fire: true, ..Action::default() }, Action::default()];
    while !world.step(&actions).iter().any(|e| matches!(e, Event::Hit { victim: 1, .. })) {}
    actions[0].fire = false;
    assert_eq!(world.ships[1].hull, 100.0);
    assert!(world.ships[1].shield < 40.0);
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

#[test]
fn three_deaths_demote_a_ship() {
    let mut world = duel(Config::default(), 150.0);
    let idle = [Action::default(); 2];
    let kill = |world: &mut World| {
        world.ships[1].hull = -1.0;
        world.step(&idle);
        assert!(!world.ships[1].alive);
        for _ in 0..world.cfg.respawn_ticks + 1 {
            world.step(&idle);
        }
        assert!(world.ships[1].alive);
    };

    // A tier-3 ship with upgrades above the lower tiers' caps.
    world.ships[1].tier = 3;
    world.ships[1].class = Class::Lancer;
    world.ships[1].levels = [5, 4, 3, 2, 1, 0, 5, 5];
    kill(&mut world);
    kill(&mut world);
    assert_eq!((world.ships[1].tier, world.ships[1].lives), (3, 1), "two deaths cost lives only");
    kill(&mut world);
    assert_eq!((world.ships[1].tier, world.ships[1].class, world.ships[1].lives), (2, Class::Lancer, 3), "tier 2 keeps the class");
    assert_eq!(world.ships[1].levels, [4, 4, 3, 2, 1, 0, 4, 4], "levels above the tier-2 cap are lost");
    for _ in 0..3 {
        kill(&mut world);
    }
    assert_eq!((world.ships[1].tier, world.ships[1].class), (1, Class::Basic));
    assert_eq!(world.ships[1].levels, [3, 3, 3, 2, 1, 0, 3, 3], "levels above the tier-1 cap are lost");

    // Three more deaths at tier 1 wipe the upgrades.
    for _ in 0..3 {
        kill(&mut world);
    }
    assert_eq!(world.ships[1].levels, [0; 8]);
    assert_eq!(world.ships[1].tier, 1);
}

#[test]
fn shots_weaken_with_distance_and_stop_at_their_range() {
    let hit_from = |gap: f32| {
        let mut world = duel(Config::default(), gap);
        let mut actions = [Action { fire: true, ..Action::default() }, Action::default()];
        for _ in 0..120 {
            let hit = world.step(&actions).iter().find_map(|e| match e {
                Event::Hit { victim: 1, damage, .. } => Some(*damage),
                _ => None,
            });
            actions[0].fire = false;
            if hit.is_some() {
                return hit;
            }
        }
        None
    };
    let (near, far) = (hit_from(60.0).unwrap(), hit_from(420.0).unwrap());
    assert!(near > 9.0 && near <= 10.0, "point blank did {near}");
    assert!(far < 6.0 && far >= 4.0, "at the edge of its range did {far}");
    assert_eq!(hit_from(600.0), None, "a basic laser reaches 470");
}

#[test]
fn ammo_runs_out_and_comes_back() {
    let mut world = duel(Config::default(), 150.0);
    world.ships[0].class = Class::Scatter;
    let fire = [Action { fire: true, ..Action::default() }, Action::default()];
    let mut volleys = 0;
    for _ in 0..600 {
        let before = world.shots.len();
        world.step(&fire);
        volleys += (world.shots.len() > before) as u32;
    }
    // A Scatter spends three shots a volley: 30 on board plus 4 a second over 20 s is about 36 volleys,
    // not the 120 its cooldown alone allows.
    assert!((34..=38).contains(&volleys), "fired {volleys} volleys");
    assert!(world.ships[0].ammo < 3.0, "less than one volley left");
    for _ in 0..600 {
        world.step(&[Action::default(); 2]);
    }
    assert_eq!(world.ships[0].ammo, world.cfg.ammo_max(&world.ships[0]));
}

#[test]
fn boosting_is_faster_until_the_tank_is_empty() {
    let top_speed = |boost: bool| {
        let mut world = duel(Config::default(), 900.0);
        let go = [Action { thrust: true, boost, ..Action::default() }, Action::default()];
        (0..45).map(|_| { world.step(&go); world.ships[0].vel.len() }).fold(0.0, f32::max)
    };
    assert!(top_speed(true) > top_speed(false) * 1.3);

    let mut world = duel(Config::default(), 900.0);
    let go = [Action { thrust: true, boost: true, ..Action::default() }, Action::default()];
    for _ in 0..90 {
        world.step(&go);
    }
    assert!(world.ships[0].boost < 0.01, "the tank lasts two seconds");
    for _ in 0..90 {
        world.step(&go);
    }
    assert!(world.ships[0].vel.len() <= world.cfg.max_speed * 1.01, "an empty tank gives no extra speed");
}

#[test]
fn each_class_fires_its_own_volley() {
    for (class, lasers) in [(Class::Basic, 1), (Class::Lancer, 1), (Class::Twin, 2), (Class::Scatter, 3)] {
        let mut world = duel(Config::default(), 150.0);
        world.ships[0].class = class;
        world.step(&[Action { fire: true, ..Action::default() }, Action::default()]);
        assert_eq!(world.shots.len(), lasers, "{class:?}");
    }
}

#[test]
fn carried_crystals_take_damage_before_the_hull_and_fall_out() {
    let mut world = duel(Config::default(), 150.0);
    world.ships[1].shield = 0.0;
    world.ships[1].shield_wait = u16::MAX;
    world.ships[1].cargo = 12;
    let fire = [Action { fire: true, ..Action::default() }, Action::default()];

    // The first hits cost crystals, not hull.
    let mut hits = 0;
    while hits < 1 {
        hits += world.step(&fire).iter().filter(|e| matches!(e, Event::Hit { victim: 1, .. })).count();
    }
    assert_eq!(world.ships[1].hull, 100.0);
    assert!(world.ships[1].cargo < 12 && world.ships[1].cargo >= 12 - 9, "cargo {}", world.ships[1].cargo);
    assert!(!world.crystals.is_empty(), "the lost crystals lie on the map");
    // They land outside the ship's own reach, so it does not simply scoop them back up.
    assert!(world.crystals.iter().all(|c| world.dist(c.pos, world.ships[1].pos) > world.cfg.pickup_radius));

    // Keep firing: the hull starts to go only once the hold is empty.
    for _ in 0..90 {
        world.step(&fire);
    }
    assert_eq!(world.ships[1].cargo, 0);
    assert!(world.ships[1].hull < 100.0);
}

#[test]
fn crystal_armor_can_be_switched_off() {
    let mut world = duel(Config { crystal_armor: 0.0, ..Config::default() }, 150.0);
    world.ships[1].shield = 0.0;
    world.ships[1].shield_wait = u16::MAX;
    world.ships[1].cargo = 12;
    let fire = [Action { fire: true, ..Action::default() }, Action::default()];
    while !world.step(&fire).iter().any(|e| matches!(e, Event::Hit { victim: 1, .. })) {}
    assert_eq!(world.ships[1].cargo, 12);
    assert!(world.ships[1].hull < 100.0);
}

#[test]
fn ships_take_up_room() {
    // Two teammates flying at each other end up apart, not on top of each other, and unhurt.
    let cfg = Config { teams: 1, ships_per_team: 2, rock_count: 0, ..Config::default() };
    let mut world = World::new(cfg, 3);
    world.ships[0].pos = POLE;
    world.ships[1].pos = world.offset(POLE, 0.3, 200.0);
    let (a, b) = (world.ships[0].pos, world.ships[1].pos);
    world.face(0, b);
    world.face(1, a);
    let go = [Action { thrust: true, ..Action::default() }; 2];
    let mut closest = f32::MAX;
    for _ in 0..120 {
        world.step(&go);
        closest = closest.min(world.dist(world.ships[0].pos, world.ships[1].pos));
    }
    assert!(closest > 20.0, "two ships of radius 12 came within {closest}");
    assert_eq!(world.ships[0].hull, 100.0);

    // Enemies that ram each other are hurt.
    let cfg = Config { teams: 2, ships_per_team: 1, rock_count: 0, turret_damage: 0.0, ..Config::default() };
    let mut world = World::new(cfg, 3);
    world.ships[0].pos = POLE;
    world.ships[1].pos = world.offset(POLE, 0.3, 200.0);
    let (a, b) = (world.ships[0].pos, world.ships[1].pos);
    world.face(0, b);
    world.face(1, a);
    let mut rammed = false;
    for _ in 0..120 {
        rammed |= world.step(&go).iter().any(|e| matches!(e, Event::Crashed { .. }));
    }
    assert!(rammed);
    assert!(world.ships[0].shield < 40.0 && world.ships[1].shield < 40.0);
}

#[test]
fn the_plan_with_the_most_votes_becomes_the_teams_order() {
    let cfg = Config { rock_count: 0, ..Config::default() };
    let mut world = World::new(cfg, 4);
    let n = world.ships.len();
    let vote = |plans: [Option<Plan>; 5]| {
        let mut actions = vec![Action::default(); n];
        for (i, plan) in plans.into_iter().enumerate() {
            actions[i].vote = plan;
        }
        actions
    };
    let count = |world: &mut World, actions: &[Action]| {
        for _ in 0..world.cfg.plan_ticks {
            world.step(actions);
        }
    };

    // Nobody has voted: no plan, orders untouched.
    count(&mut world, &vote([None; 5]));
    assert_eq!(world.plans[0], None);
    assert!(world.ships.iter().all(|s| s.order.kind == OrderKind::Mine));

    // Three of five vote to attack the first enemy base: the whole team gets that order.
    count(&mut world, &vote([Some(Plan::AttackA), Some(Plan::AttackA), Some(Plan::AttackA), Some(Plan::Farm), None]));
    assert_eq!(world.plans[0], Some(Plan::AttackA));
    let target = world.bases[1].pos;
    for s in world.ships.iter().filter(|s| s.team == 0) {
        assert_eq!(s.order.kind, OrderKind::Attack);
        assert!(world.dist(s.order.target, target) < 1.0);
    }
    assert!(world.ships.iter().filter(|s| s.team != 0).all(|s| s.order.kind == OrderKind::Mine), "other teams are not affected");

    // Votes stand until changed; a ship sees how its team is voting.
    let seen = observe(&world, 4);
    assert_eq!(seen.vote, None);
    assert!((seen.votes[Plan::AttackA as usize] - 0.6).abs() < 1e-6 && (seen.votes[Plan::Farm as usize] - 0.2).abs() < 1e-6);

    // The vote swings to defending the base, and to the second enemy base.
    count(&mut world, &vote([Some(Plan::Defend); 5]));
    assert!(world.ships.iter().filter(|s| s.team == 0).all(|s| s.order.kind == OrderKind::Defend && world.dist(s.order.target, world.bases[0].pos) < 1.0));
    count(&mut world, &vote([Some(Plan::AttackB); 5]));
    assert!(world.ships.iter().filter(|s| s.team == 0).all(|s| world.dist(s.order.target, world.bases[2].pos) < 1.0));

    // A team commanded from outside keeps the orders it was given.
    world.commanded[0] = true;
    count(&mut world, &vote([Some(Plan::Farm); 5]));
    assert!(world.ships.iter().filter(|s| s.team == 0).all(|s| s.order.kind == OrderKind::Attack));
}
