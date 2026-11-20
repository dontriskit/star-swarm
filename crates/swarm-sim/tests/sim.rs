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
    // A third of the way around a planet of radius 1200.
    assert!((d01 - 2513.3).abs() < 1.0, "{d01}");
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
    // Once around is 1885 units; at about 150 a second that is roughly 13 s after getting up to speed.
    assert!(farthest > 900.0, "only got {farthest} away, half-way around is 942");
    assert!(back.is_some_and(|t| (360..480).contains(&t)), "back at the start after {back:?} ticks");
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
    world.seen_base_hp.iter_mut().for_each(|seen| seen.push(100.0));
    world.seen_base_hp.push(vec![100.0; 3]);
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
    for (class, lasers) in [(Class::Basic, 1), (Class::Lancer, 1), (Class::Twin, 2), (Class::Scatter, 3), (Class::Ram, 1)] {
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
fn each_ship_follows_its_own_mode_and_teammates_see_it() {
    let cfg = Config { rock_count: 0, ..Config::default() };
    let mut world = World::new(cfg, 4);
    let n = world.ships.len();
    let per_team = world.cfg.ships_per_team;
    let pick = |world: &mut World, modes: &[(usize, Plan)]| {
        let mut actions = vec![Action::default(); n];
        for (ship, mode) in modes {
            actions[*ship].vote = Some(*mode);
        }
        for _ in 0..world.cfg.plan_ticks {
            world.step(&actions);
        }
    };

    // Nobody has picked a mode: orders untouched.
    pick(&mut world, &[]);
    assert_eq!(world.plans[0], None);
    assert!(world.ships.iter().all(|s| s.order.kind == OrderKind::Mine));

    // Three ships of team 0 go for the first enemy base, one defends, the rest stay as they were.
    pick(&mut world, &[(0, Plan::AttackA), (1, Plan::AttackA), (2, Plan::AttackA), (3, Plan::Defend)]);
    for i in 0..3 {
        assert_eq!(world.ships[i].order.kind, OrderKind::Attack);
        assert!(world.dist(world.ships[i].order.target, world.bases[1].pos) < 1.0);
    }
    assert_eq!(world.ships[3].order.kind, OrderKind::Defend);
    assert!(world.dist(world.ships[3].order.target, world.bases[0].pos) < 1.0);
    assert!((4..per_team).all(|i| world.ships[i].order.kind == OrderKind::Mine), "ships without a mode keep their orders");
    assert!(world.ships.iter().filter(|s| s.team != 0).all(|s| s.order.kind == OrderKind::Mine), "other teams are not affected");
    assert_eq!(world.plans[0], Some(Plan::AttackA), "the most common mode is the team's headline");

    // Team radar: a ship sees every teammate's mode, wherever they are, and how the team splits.
    let seen = observe(&world, 4);
    assert_eq!(seen.vote, None);
    assert_eq!(seen.mates.len(), per_team - 1);
    assert_eq!(seen.mates.iter().filter(|m| m.mode == Some(Plan::AttackA)).count(), 3);
    assert_eq!(seen.mates.iter().filter(|m| m.mode == Some(Plan::Defend)).count(), 1);
    assert!((seen.votes[Plan::AttackA as usize] - 3.0 / per_team as f32).abs() < 1e-6);

    // A mode stands until changed; a second enemy base can be picked too.
    pick(&mut world, &[(0, Plan::AttackB)]);
    assert!(world.dist(world.ships[0].order.target, world.bases[2].pos) < 1.0);
    assert!(world.dist(world.ships[1].order.target, world.bases[1].pos) < 1.0);

    // A team commanded from outside keeps the orders it was given.
    world.commanded[0] = true;
    pick(&mut world, &[(1, Plan::Farm)]);
    assert_eq!(world.ships[1].order.kind, OrderKind::Attack);
}

#[test]
fn team_radar_reports_enemies_a_teammate_sees() {
    let cfg = Config { teams: 2, ships_per_team: 2, rock_count: 0, turret_damage: 0.0, ..Config::default() };
    let mut world = World::new(cfg, 4);
    // Ship 0 at the pole; its teammate 1500 away with an enemy right next to it; the other enemy far off.
    world.ships[0].pos = POLE;
    world.ships[1].pos = world.offset(POLE, 0.3, 1500.0);
    world.ships[2].pos = world.offset(world.ships[1].pos, 1.0, 200.0);
    world.ships[3].pos = world.offset(POLE, 3.4, 2500.0);
    // A moved ship needs its nose back on the surface where it now is.
    world.face(0, world.ships[1].pos);
    world.face(1, world.ships[2].pos);
    let seen = observe(&world, 0);
    assert!(seen.ships.is_empty(), "nothing is within the ship's own sight");
    assert_eq!(seen.mates.len(), 1);
    assert!((seen.mates[0].dist - 1500.0).abs() < 1.0, "the teammate is on the radar at any distance");
    assert_eq!(seen.contacts.len(), 1, "only the enemy a teammate can see is reported");
    assert!((seen.contacts[0].len() - world.dist(POLE, world.ships[2].pos)).abs() < 1.0);
    // The teammate sees that enemy itself, so for it the enemy is no radar contact.
    let mate = observe(&world, 1);
    assert_eq!((mate.ships.len(), mate.contacts.len()), (1, 0));
}

#[test]
fn an_enemy_bases_health_is_only_known_from_what_the_team_has_seen() {
    let cfg = Config { teams: 2, ships_per_team: 1, rock_count: 0, turret_damage: 0.0, ..Config::default() };
    let mut world = World::new(cfg, 4);
    world.ships[0].pos = POLE;
    world.ships[1].pos = POLE;
    world.bases[1].hp = 400.0;
    world.step(&[Action::default(); 2]);
    // Nobody of team 0 is near base 1: it still believes the base is whole. Team 1 knows its own base.
    assert_eq!(observe(&world, 0).enemy_bases[0].hp, 1.0);
    assert_eq!(observe(&world, 1).base.hp, 0.4);
    // A scout within view range brings the news.
    world.ships[0].pos = world.offset(world.bases[1].pos, 0.0, 300.0);
    world.step(&[Action::default(); 2]);
    assert_eq!(observe(&world, 0).enemy_bases[0].hp, 0.4);
}

/// Two enemy ships fly straight at each other from 300 apart; returns what each lost (shield and hull).
fn ram(first: Class, second: Class) -> (f32, f32) {
    let cfg = Config { teams: 2, ships_per_team: 1, rock_count: 0, turret_damage: 0.0, ..Config::default() };
    let mut world = World::new(cfg, 3);
    world.ships[0].pos = POLE;
    world.ships[1].pos = world.offset(POLE, 0.3, 300.0);
    let (a, b) = (world.ships[0].pos, world.ships[1].pos);
    world.face(0, b);
    world.face(1, a);
    for (i, class) in [first, second].into_iter().enumerate() {
        world.ships[i].tier = 2;
        world.ships[i].class = class;
        world.ships[i].hull = world.cfg.hull_max(&world.ships[i]);
        world.ships[i].shield = world.cfg.shield_max(&world.ships[i]);
    }
    let full: Vec<f32> = world.ships.iter().map(|s| s.hull + s.shield).collect();
    let go = [Action { thrust: true, ..Action::default() }; 2];
    for _ in 0..50 {
        if world.step(&go).iter().any(|e| matches!(e, Event::Crashed { .. })) {
            break;
        }
    }
    let lost = |i: usize| full[i] - (world.ships[i].hull.max(0.0) + world.ships[i].shield);
    (lost(0), lost(1))
}

#[test]
fn a_ram_wins_a_collision_and_a_lancer_is_fragile() {
    let (twin_a, twin_b) = ram(Class::Twin, Class::Twin);
    assert!(twin_a > 10.0 && (twin_a - twin_b).abs() < 1.0, "two alike take the same: {twin_a} and {twin_b}");

    let (rammer, victim) = ram(Class::Ram, Class::Twin);
    assert!(victim > twin_a * 3.0, "a Ram hits much harder: {victim} against {twin_a}");
    assert!(rammer < twin_a / 2.0, "and takes much less: {rammer} against {twin_a}");

    let (_, lancer) = ram(Class::Twin, Class::Lancer);
    assert!(lancer > twin_a * 1.3, "a Lancer takes more from the same blow: {lancer}");

    // Hull by class: the Ram is the toughest, the Lancer the thinnest.
    let world = World::new(Config { rock_count: 0, ..Config::default() }, 1);
    let hull = |class: Class| {
        let mut s = world.ships[0].clone();
        s.tier = 2;
        s.class = class;
        world.cfg.hull_max(&s)
    };
    assert!(hull(Class::Ram) > hull(Class::Twin) && hull(Class::Twin) > hull(Class::Lancer));
}
