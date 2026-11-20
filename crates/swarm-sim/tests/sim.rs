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
    run(&mut world, 9000);
    let collected: u32 = world.ships.iter().map(|s| s.collected).sum();
    let handed_over: u32 = world.ships.iter().map(|s| s.deposited).sum();
    let levels: u32 = world.ships.iter().flat_map(|s| s.levels).map(u32::from).sum();
    assert!(collected > 300, "collected only {collected}");
    assert!(handed_over > 100, "handed over only {handed_over} at the bases");
    assert!(world.drones.len() + levels as usize > 3, "bought only {} drones and {levels} upgrade levels", world.drones.len());
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
    // Points pay for everything; the shop is inside the base.
    world.ships[0].points = 1000;
    world.bases[0].level = 3;
    world.ships[0].pos = world.offset(world.bases[0].pos, 0.0, 20.0);
    world.face(0, world.bases[0].pos);
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
    assert_eq!(world.ships[0].points, 1000 - 60);
    let full_tier1 = world.cfg.damage(&world.ships[0]);

    // Tier 2: paid for once, the class is chosen, the upgrades are gone.
    buy(&mut world, &mut actions, Buy::Tier(Class::Twin), 1);
    assert_eq!((world.ships[0].tier, world.ships[0].class), (2, Class::Twin));
    assert_eq!(world.ships[0].levels, [0; 8]);
    assert_eq!(world.ships[0].points, 1000 - 60 - 150);
    assert_eq!(world.ships[0].hull, world.cfg.hull_max(&world.ships[0]));

    // Tier 2 stops at level 4, tier 3 at level 5, and tier 3 keeps the class.
    buy(&mut world, &mut actions, Buy::Upgrade(Upgrade::Attack), 10);
    assert_eq!(world.ships[0].levels[Upgrade::Attack as usize], 4);
    world.ships[0].points = 1000;
    buy(&mut world, &mut actions, Buy::Tier(Class::Lancer), 1);
    assert_eq!((world.ships[0].tier, world.ships[0].class, world.ships[0].points), (3, Class::Twin, 1000 - 300));
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
    // An undefended base of 30 HP falls to thirty basic shots.
    let mut world = siege(Config { base_hp: 30.0, turret_damage: 0.0, ..Config::default() });
    assert_eq!(world.bases[1].hp, 30.0);
    let fire = [Action { fire: true, ..Action::default() }, Action::default()];
    let mut destroyed = false;
    while !world.done() {
        destroyed |= world.step(&fire).iter().any(|e| matches!(e, Event::BaseDestroyed { team: 1, by: 0 }));
    }
    assert!(destroyed);
    assert_eq!(world.winner(), Some(0));
    assert!(world.tick < 900, "a 30 HP base should not take {} ticks", world.tick);
}

#[test]
fn a_team_without_a_base_gets_no_new_ships() {
    let mut world = siege(Config::default());
    // A third team keeps the match going after team 1 loses its base.
    world.bases.push(Base { pos: Vec3::new(0.0, 0.0, -1.0), hp: 100.0, cooldown: 0, level: 1, deposited: 0, spill: 0.0 });
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
    assert!(world.ships[1].shield < 100.0);
    // Let shots already in flight land or expire, then wait out the delay.
    for _ in 0..40 {
        world.step(&actions);
    }
    let low = world.ships[1].shield;
    // Unupgraded shields grow back slowly: a point a second.
    for _ in 0..400 {
        world.step(&actions);
    }
    assert!(world.ships[1].shield > low);
    assert_eq!(world.ships[1].shield, world.cfg.shield_max(&world.ships[1]));
}

#[test]
fn three_deaths_demote_a_ship() {
    // Lives are off by default; a match can still be set up with them.
    let mut world = duel(Config { lives: 3, ..Config::default() }, 150.0);
    let idle = [Action::default(); 2];
    let kill = |world: &mut World| {
        // Well below zero: a ship that respawned inside its base is docked and being repaired.
        world.ships[1].hull = -1000.0;
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
    // Full power over the first 60% of the range (282 of 470), then fading to nothing.
    let (near, middle, far) = (hit_from(60.0).unwrap(), hit_from(250.0).unwrap(), hit_from(420.0).unwrap());
    assert_eq!((near, middle), (2.0, 2.0), "full power inside the full-power range");
    assert!(far < 0.8 && far > 0.1, "near the edge of its range did {far}");
    assert_eq!(hit_from(600.0), None, "a basic laser reaches 470");
}

#[test]
fn dying_costs_points_and_a_run_of_kills_pays_until_it_is_ended() {
    let mut world = World::new(Config { teams: 2, ships_per_team: 2, rock_count: 0, turret_damage: 0.0, ..Config::default() }, 6);
    let idle = vec![Action::default(); 4];
    // Ship 0 (team 0) destroys ship 2 (team 1) over and over.
    let kill = |world: &mut World, victim: usize, by: u16| {
        world.ships[victim].hull = -1000.0;
        world.ships[victim].last_hit_by = Some(by);
        world.step(&idle);
        for _ in 0..world.cfg.respawn_ticks + 1 {
            world.step(&idle);
        }
    };
    world.ships[2].points = 100;
    world.ships[2].levels[Upgrade::Attack as usize] = 2;
    kill(&mut world, 2, 0);
    assert_eq!(world.ships[2].points, 60, "20 and a fifth of the 100");
    assert_eq!(world.ships[2].levels[Upgrade::Attack as usize], 2, "nothing else is lost");
    kill(&mut world, 2, 0);
    assert_eq!((world.ships[0].streak, world.ships[0].points), (2, 0), "two kills are no run yet");
    kill(&mut world, 2, 0);
    kill(&mut world, 2, 0);
    assert_eq!((world.ships[0].streak, world.ships[0].points), (4, 60), "30 for the third and for the fourth");
    world.ships[0].pos = world.offset(world.ships[2].pos, 0.3, 200.0);
    assert_eq!(observe(&world, 2).ships.iter().map(|s| s.streak).fold(0.0, f32::max), 4.0, "the run shows");
    // Ship 3 ends the run: 10 a kill of it to each ship of its team, and the run is gone.
    let before = (world.ships[2].points, world.ships[3].points);
    kill(&mut world, 0, 3);
    assert_eq!((world.ships[2].points, world.ships[3].points), (before.0 + 40, before.1 + 40));
    assert_eq!((world.ships[0].streak, world.ships[0].points), (0, 60 - 20 - 12));
}

#[test]
fn a_base_under_fire_sheds_crystals() {
    let mut world = duel(Config { turret_damage: 0.0, ..Config::default() }, 150.0);
    let base = world.bases[1].pos;
    world.ships[0].pos = world.offset(base, 0.3, 200.0);
    world.face(0, base);
    world.ships[1].pos = world.offset(base, 2.0, 900.0);
    let fire = [Action { fire: true, ..Action::default() }, Action::default()];
    let mut dealt = 0.0;
    let mut seen = 0;
    while dealt < 30.0 {
        dealt += world.step(&fire).iter().map(|e| if let Event::BaseHit { damage, .. } = e { *damage } else { 0.0 }).sum::<f32>();
        seen = seen.max(world.crystals.iter().filter(|c| c.full == 10).count());
    }
    assert!(seen >= 1, "a pack of 10 for every 10 damage");
    assert!(world.crystals.iter().all(|c| world.dist(c.pos, base) > world.cfg.base_radius));
}

#[test]
fn a_team_learns_how_hard_an_enemy_hits_by_being_hit() {
    let mut world = duel(Config::default(), 150.0);
    assert_eq!(observe(&world, 1).ships[0].threat, 0.0, "nothing is known of a ship that has not hit yet");
    let fire = [Action { fire: true, ..Action::default() }, Action::default()];
    while !world.step(&fire).iter().any(|e| matches!(e, Event::Hit { victim: 1, .. })) {}
    assert_eq!(observe(&world, 1).ships[0].threat, 2.0 / 100.0, "a basic shot does 2 to a hull of 100");
    // An upgrade shows only once it has been felt.
    world.ships[0].levels[Upgrade::Attack as usize] = 2;
    assert_eq!(observe(&world, 1).ships[0].threat, 0.02);
    while !world.step(&fire).iter().any(|e| matches!(e, Event::Hit { victim: 1, damage, .. } if *damage > 2.5)) {}
    assert!((observe(&world, 1).ships[0].threat - 0.028).abs() < 1e-6);
    assert_eq!(observe(&world, 0).ships[0].threat, 0.0, "the other side has learned nothing");
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
    // A Scatter volley is three shots of 8 damage, 2.4 ammo: 30 on board plus 4 a second over 20 s
    // is about 45 volleys, not the 120 its cooldown alone allows.
    assert!((43..=47).contains(&volleys), "fired {volleys} volleys");
    assert!(world.ships[0].ammo < 2.4, "less than one volley left");
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
    world.ships[1].cargo = 10;
    let fire = [Action { fire: true, ..Action::default() }, Action::default()];

    // The first hits cost crystals, not hull.
    let mut hits = 0;
    while hits < 3 {
        hits += world.step(&fire).iter().filter(|e| matches!(e, Event::Hit { victim: 1, .. })).count();
    }
    assert_eq!(world.ships[1].hull, 100.0);
    assert!(world.ships[1].cargo < 10 && world.ships[1].cargo >= 4, "cargo {}", world.ships[1].cargo);
    assert!(!world.crystals.is_empty(), "the lost crystals lie on the map");
    // They land outside the ship's own reach, so it does not simply scoop them back up.
    let ship = &world.ships[1];
    assert!(world.crystals.iter().all(|c| world.dist(c.pos, ship.pos) > world.cfg.pickup_reach(world.cfg.radius_of(ship), c.value)));

    // Keep firing: the hull starts to go only once the hold is empty.
    for _ in 0..150 {
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
    assert!(world.ships[0].shield < 100.0 && world.ships[1].shield < 100.0);
}

#[test]
fn attacking_is_a_team_decision_and_farming_the_default() {
    let cfg = Config { rock_count: 0, ..Config::default() };
    let mut world = World::new(cfg, 4);
    let n = world.ships.len();
    let per_team = world.cfg.ships_per_team;
    assert_eq!(per_team, 9);
    let pick = |world: &mut World, modes: &[(usize, Plan)]| {
        let mut actions = vec![Action::default(); n];
        for (ship, mode) in modes {
            actions[*ship].vote = Some(*mode);
        }
        for _ in 0..world.cfg.plan_ticks {
            world.step(&actions);
        }
    };
    let kinds = |world: &World| world.ships[..per_team].iter().map(|s| s.order.kind).collect::<Vec<_>>();

    // Nobody on any team has picked a mode: orders untouched, as for scripted teams.
    pick(&mut world, &[]);
    assert_eq!(world.plans[0], None);

    // One ship defends: it guards the base; the rest, having picked nothing, farm near the team.
    pick(&mut world, &[(3, Plan::Defend)]);
    assert_eq!(world.ships[3].order.kind, OrderKind::Defend);
    assert!(world.dist(world.ships[3].order.target, world.bases[0].pos) < 1.0);
    assert!(kinds(&world).iter().enumerate().all(|(i, k)| i == 3 || *k == OrderKind::Mine));

    // Four of nine want to attack: not half the team, so nobody attacks.
    pick(&mut world, &[(0, Plan::AttackA), (1, Plan::AttackA), (2, Plan::AttackB), (5, Plan::AttackA)]);
    assert!(kinds(&world).iter().all(|k| *k != OrderKind::Attack), "{:?}", kinds(&world));

    // A fifth joins: the whole team attacks, defender included, at the base most of them picked.
    pick(&mut world, &[(4, Plan::AttackA)]);
    assert!(kinds(&world).iter().all(|k| *k == OrderKind::Attack), "{:?}", kinds(&world));
    assert!(world.ships[..per_team].iter().all(|s| world.dist(s.order.target, world.bases[1].pos) < 1.0));
    assert_eq!(world.plans[0], Some(Plan::AttackA));
    assert!(world.ships.iter().filter(|s| s.team != 0).all(|s| s.order.kind == OrderKind::Mine), "other teams are not affected");

    // Team radar: a ship sees every teammate's mode and how the team splits.
    // (The radar shows the six nearest of the eight teammates; the split counts them all.)
    let seen = observe(&world, 6);
    assert_eq!(seen.mates.len(), 6);
    assert!((seen.votes[Plan::AttackA as usize] - 4.0 / per_team as f32).abs() < 1e-6);
    assert!((seen.votes[Plan::AttackB as usize] - 1.0 / per_team as f32).abs() < 1e-6);

    // Two change their minds: the attack is off, and everyone is back to their own mode.
    pick(&mut world, &[(0, Plan::Farm), (1, Plan::Farm)]);
    assert_eq!(world.ships[3].order.kind, OrderKind::Defend);
    assert_eq!(world.ships[4].order.kind, OrderKind::Mine);

    // A team commanded from outside keeps the orders it was given.
    world.commanded[0] = true;
    pick(&mut world, &[(0, Plan::AttackA), (1, Plan::AttackA), (2, Plan::AttackA), (4, Plan::AttackA), (5, Plan::AttackA)]);
    assert_eq!(world.ships[0].order.kind, OrderKind::Mine);
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
    // (Team 1's base reads against the 1000 hit points of a new base.)
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

#[test]
fn harder_hitting_shots_cost_more_ammo() {
    let world = duel(Config::default(), 150.0);
    let cost = |class: Class, attack: u8| {
        let mut s = world.ships[0].clone();
        s.class = class;
        s.levels[Upgrade::Attack as usize] = attack;
        world.cfg.volley_cost(&s)
    };
    // One ammo buys one point of damage: a basic shot of 1 costs 1, a Lancer's 3 costs 3.
    assert!((cost(Class::Basic, 0) - 1.0).abs() < 1e-6);
    assert!((cost(Class::Lancer, 0) - 3.0).abs() < 1e-6);
    assert!((cost(Class::Twin, 0) - 2.2).abs() < 1e-6, "two lasers of 1.1");
    // Attack upgrades make every shot hit harder and cost more.
    assert!(cost(Class::Basic, 3) > cost(Class::Basic, 0) * 1.5);

    // A Lancer's full hold of 30 is ten shots; the eleventh has to wait for ammo to come back.
    let mut world = duel(Config { ammo_regen: 0.0, ..Config::default() }, 150.0);
    world.ships[0].class = Class::Lancer;
    let fire = [Action { fire: true, ..Action::default() }, Action::default()];
    let mut shots = 0;
    for _ in 0..300 {
        let before = world.shots.len();
        world.step(&fire);
        shots += (world.shots.len() > before) as u32;
    }
    assert_eq!(shots, 10);
}

/// One ship of one team, parked inside its base with crystals on board; no rocks.
fn at_home(cargo: u32) -> World {
    let cfg = Config { teams: 2, ships_per_team: 1, rock_count: 0, turret_damage: 0.0, ..Config::default() };
    let mut world = World::new(cfg, 2);
    world.ships[0].pos = world.offset(world.bases[0].pos, 0.0, 20.0);
    world.face(0, world.bases[0].pos);
    world.ships[1].pos = POLE;
    world.ships[0].cargo = cargo;
    world
}

const UNLOAD: Action = Action { thrust: false, turn: 0, fire: false, boost: false, deposit: true, buy: None, vote: None };

#[test]
fn crystals_are_handed_over_inside_the_base_a_tenth_of_a_hold_a_second() {
    let mut world = at_home(45);
    let unload = [UNLOAD, Action::default()];
    // A tier-1 hold is 150, so 15 crystals a second: after one second, 15 of the 45.
    for _ in 0..30 {
        world.step(&unload);
    }
    assert_eq!((world.ships[0].cargo, world.ships[0].points), (30, 15));
    // The ship can stop whenever it likes, and carry on later.
    for _ in 0..30 {
        world.step(&[Action::default(); 2]);
    }
    assert_eq!((world.ships[0].cargo, world.ships[0].points), (30, 15));
    for _ in 0..90 {
        world.step(&unload);
    }
    assert_eq!((world.ships[0].cargo, world.ships[0].points, world.bases[0].deposited), (0, 45, 45));

    // Next to the base is not inside it: nothing is handed over 100 away.
    let mut world = at_home(45);
    world.ships[0].pos = world.offset(world.bases[0].pos, 0.0, 100.0);
    for _ in 0..60 {
        world.step(&unload);
    }
    assert_eq!((world.ships[0].cargo, world.ships[0].points), (45, 0));
}

#[test]
fn the_shop_is_inside_the_base() {
    let buy = [Action { buy: Some(Buy::Upgrade(Upgrade::Attack)), ..Action::default() }, Action::default()];
    let mut world = at_home(0);
    world.ships[0].points = 100;
    world.ships[0].pos = world.offset(world.bases[0].pos, 0.0, 100.0);
    world.step(&buy);
    assert_eq!((world.ships[0].points, world.ships[0].levels[Upgrade::Attack as usize]), (100, 0), "outside the base nothing can be bought");
    world.ships[0].pos = world.offset(world.bases[0].pos, 0.0, 20.0);
    world.step(&buy);
    assert_eq!((world.ships[0].points, world.ships[0].levels[Upgrade::Attack as usize]), (90, 1));
    assert!(observe(&world, 0).in_base);
}

#[test]
fn enough_crystals_raise_the_base_a_level_and_let_ships_move_up_a_tier() {
    let mut world = at_home(0);
    world.ships[0].points = 1000;
    let tier_up = [Action { buy: Some(Buy::Tier(Class::Twin)), ..Action::default() }, Action::default()];
    // Tier 2 costs a full tier-1 hold, 150.
    world.step(&tier_up);
    assert_eq!((world.ships[0].tier, world.ships[0].points), (2, 850));
    world.step(&tier_up);
    assert_eq!(world.ships[0].tier, 2, "tier 3 needs a level-2 base");

    // The team has handed over 995 crystals so far; this ship brings the rest.
    world.bases[0].deposited = 995;
    world.ships[0].cargo = 10;
    let mut levelled = false;
    for _ in 0..30 {
        levelled |= world.step(&[UNLOAD, Action::default()]).iter().any(|e| matches!(e, Event::BaseLevel { team: 0, level: 2 }));
    }
    assert!(levelled);
    assert_eq!(world.bases[0].level, 2);
    assert_eq!(world.bases[0].hp, 1200.0, "the level comes with 200 more hit points");
    assert_eq!(observe(&world, 0).base_level, 2);

    world.step(&tier_up);
    assert_eq!(world.ships[0].tier, 3);
    assert_eq!(world.cfg.next_base_level_at(2), Some(2500));
    assert_eq!(world.cfg.next_base_level_at(7), None);
}

#[test]
fn points_repair_the_base() {
    let mut world = at_home(0);
    world.ships[0].points = 25;
    world.bases[0].hp = 500.0;
    let repair = [Action { buy: Some(Buy::Repair), ..Action::default() }, Action::default()];
    for _ in 0..5 {
        world.step(&repair);
    }
    // Two repairs of 10 points for 10 hit points each; 5 points are not enough for a third.
    assert_eq!((world.bases[0].hp, world.ships[0].points), (520.0, 5));
    world.bases[0].hp = 1000.0;
    world.ships[0].points = 50;
    world.step(&repair);
    assert_eq!(world.ships[0].points, 50, "a whole base takes no repairs");
}

/// A ship with a drone of the given kind, bought at its base and then moved with it to the pole;
/// the enemy ship waits 1500 away.
fn with_drone(kind: DroneKind) -> World {
    let mut world = at_home(0);
    world.ships[0].points = 100;
    world.step(&[Action { buy: Some(Buy::Drone(kind)), ..Action::default() }, Action::default()]);
    assert_eq!(world.drones.len(), 1);
    world.ships[0].pos = POLE;
    world.ships[0].fwd = geo::bearing(POLE, 0.3);
    world.drones[0].pos = world.offset(POLE, 2.0, 20.0);
    world.ships[1].pos = world.offset(POLE, 2.0, 1500.0);
    world
}

#[test]
fn drones_are_bought_and_sold_in_the_base_and_limited_by_tier() {
    let mut world = at_home(0);
    world.ships[0].points = 200;
    let buy = |kind| [Action { buy: Some(Buy::Drone(kind)), ..Action::default() }, Action::default()];
    world.step(&buy(DroneKind::Combat));
    world.step(&buy(DroneKind::Eco));
    assert_eq!(world.drones.len(), 1, "a tier-1 ship has room for one drone");
    assert_eq!(world.ships[0].points, 140, "a combat drone costs 60");
    assert_eq!(observe(&world, 0).drone_kinds, [0, 0, 1]);
    // Sold back for half.
    world.step(&[Action { buy: Some(Buy::SellDrone), ..Action::default() }, Action::default()]);
    assert_eq!((world.drones.len(), world.ships[0].points), (0, 170));
}

#[test]
fn an_eco_drone_mines_for_its_owner_and_can_be_taken_when_the_owner_dies() {
    let mut world = with_drone(DroneKind::Eco);
    // A small rock 150 away: the ship does nothing, the drone goes and breaks it and brings the crystals in.
    let spot = world.offset(POLE, 0.3, 150.0);
    world.rocks.push(Rock { pos: spot, vel: Vec3::default(), size: 1, hp: 10.0, last_hit_by: None });
    let idle = [Action::default(); 2];
    for _ in 0..600 {
        world.step(&idle);
        if world.ships[0].cargo > 0 {
            break;
        }
    }
    assert!(world.ships[0].cargo > 0, "the drone gathered nothing");

    // The owner is destroyed: the drone is left where it is, and the enemy that flies up takes it.
    world.ships[0].hull = -1.0;
    world.step(&idle);
    world.step(&idle);
    assert_eq!(world.drones[0].owner, None);
    world.ships[1].pos = world.drones[0].pos;
    world.step(&idle);
    assert_eq!(world.drones[0].owner, Some(1));
}

#[test]
fn an_eco_drones_shots_do_nothing_to_ships_but_a_combat_drones_do() {
    for (kind, hurts) in [(DroneKind::Eco, false), (DroneKind::Combat, true)] {
        let mut world = with_drone(kind);
        // An enemy 150 from the owner, with a rock right behind it for the eco drone to shoot at.
        world.ships[1].pos = world.offset(POLE, 0.3, 150.0);
        world.rocks.push(Rock { pos: world.offset(POLE, 0.3, 190.0), vel: Vec3::default(), size: 1, hp: 500.0, last_hit_by: None });
        let idle = [Action::default(); 2];
        let mut hit = false;
        for _ in 0..300 {
            hit |= world.step(&idle).iter().any(|e| matches!(e, Event::Hit { victim: 1, .. }));
        }
        assert_eq!(hit, hurts, "{kind:?}");
    }
}

#[test]
fn a_shield_drone_takes_the_shots_meant_for_its_owner_until_it_breaks() {
    let mut world = with_drone(DroneKind::Shield);
    // The enemy sits 300 from the owner and fires straight at it.
    world.ships[1].pos = world.offset(POLE, 0.3, 300.0);
    world.face(1, POLE);
    let fire = [Action::default(), Action { fire: true, ..Action::default() }];
    // The drone has 30 hit points; a basic shot does 2.
    for _ in 0..60 {
        world.step(&fire);
    }
    assert_eq!(world.drones.len(), 1);
    assert!(world.drones[0].hp < world.cfg.drone_hp, "the drone was not in the way");
    assert_eq!(world.ships[0].shield, world.cfg.shield_max(&world.ships[0]), "the owner was hit while the drone stood");
    for _ in 0..200 {
        world.step(&fire);
    }
    assert!(world.drones.is_empty(), "the drone should have broken");
    assert!(world.ships[0].shield < world.cfg.shield_max(&world.ships[0]), "with the drone gone the shots reach the owner");
}

#[test]
fn flying_into_a_rock_damages_it_and_costs_shield() {
    let cfg = Config { teams: 1, ships_per_team: 1, rock_count: 0, ..Config::default() };
    let mut world = World::new(cfg, 2);
    world.ships[0].pos = POLE;
    let spot = world.offset(POLE, 0.3, 250.0);
    world.face(0, spot);
    world.rocks.push(Rock { pos: spot, vel: Vec3::default(), size: 2, hp: 40.0, last_hit_by: None });
    let go = [Action { thrust: true, ..Action::default() }];
    let mut blow = 0.0;
    for _ in 0..120 {
        for e in world.step(&go) {
            if let Event::RockHit { by: 0, damage, .. } = e {
                blow += damage;
            }
        }
        if blow > 0.0 {
            break;
        }
    }
    // At close to full speed (150) the rock takes about 30 and the ship's shield about 22.
    assert!(blow > 15.0 && blow < 31.0, "the rock took {blow}");
    assert!((world.rocks[0].hp - (40.0 - blow)).abs() < 1e-3);
    let lost = world.cfg.shield_max(&world.ships[0]) - world.ships[0].shield;
    assert!(lost > 10.0 && lost < blow, "the ship lost {lost} shield");
    assert_eq!(world.ships[0].hull, 100.0, "the shield took all of it");

    // With the shields gone the hull pays, and the hold is left alone: a rock is no attacker.
    let cfg = Config { teams: 1, ships_per_team: 1, rock_count: 0, ..Config::default() };
    let mut world = World::new(cfg, 2);
    world.ships[0].pos = POLE;
    let spot = world.offset(POLE, 0.3, 250.0);
    world.face(0, spot);
    world.rocks.push(Rock { pos: spot, vel: Vec3::default(), size: 5, hp: 250.0, last_hit_by: None });
    world.ships[0].shield = 0.0;
    world.ships[0].shield_wait = u16::MAX;
    world.ships[0].cargo = 40;
    let mut lost = 0.0;
    for _ in 0..120 {
        let events = world.step(&go);
        lost += events.iter().map(|e| if let Event::HullLost { amount, .. } = e { *amount } else { 0.0 }).sum::<f32>();
        if events.iter().any(|e| matches!(e, Event::Crashed { .. })) {
            break;
        }
    }
    assert!(world.ships[0].hull < 100.0);
    assert!((100.0 - world.ships[0].hull - lost).abs() < 1e-3, "the lost hull is reported");
    assert_eq!(world.ships[0].cargo, 40, "flying into a rock knocks no crystals out");
    assert!(world.crystals.is_empty());
}

#[test]
fn a_pack_of_crystals_fades_to_nothing() {
    let cfg = Config { teams: 1, ships_per_team: 1, rock_count: 0, ..Config::default() };
    let mut world = World::new(cfg, 3);
    let life = world.cfg.crystal_ticks;
    assert_eq!(life, 300, "ten seconds");
    let far = world.offset(world.ships[0].pos, 0.0, 600.0);
    world.crystals.push(Crystal { pos: far, ticks: life, value: 100, full: 100, boost_cell: false });
    let idle = vec![Action::default(); 1];
    let mut last = 100;
    for tick in 1..life {
        world.step(&idle);
        let value = world.crystals[0].value;
        assert!(value <= last && value > 0);
        last = value;
        if tick == life / 2 {
            assert_eq!(value, 50, "half its worth at half its time");
        }
    }
    world.step(&idle);
    assert!(world.crystals.is_empty(), "gone when its time is up");
}

#[test]
fn small_rocks_are_quick_work_and_big_ones_pay_more() {
    let cfg = Config::default();
    assert_eq!([1, 2, 3, 4, 5].map(|s| cfg.rock_hp_of(s)), [20.0, 80.0, 180.0, 320.0, 500.0]);
    assert_eq!(cfg.rock_packs, [(8, 15), (16, 40), (41, 70), (71, 110), (111, 150)]);
    // Per shot a basic ship gets the most out of the smallest rocks.
    let per_hp = |size: u8| (cfg.rock_packs[size as usize - 1].0 + cfg.rock_packs[size as usize - 1].1) as f32 / 2.0 / cfg.rock_hp_of(size);
    assert!((2..=5).all(|s| per_hp(1) > per_hp(s)));

    // Every size turns up, small ones most often, and a broken rock leaves one pack in its size's range.
    let world = World::new(Config { rock_count: 2000, ..Config::default() }, 9);
    let count = |size: u8| world.rocks.iter().filter(|r| r.size == size).count();
    assert!(count(1) > count(3) && count(3) > count(5) && count(5) > 0);
}

#[test]
fn a_broken_rock_leaves_one_pack_and_a_ship_takes_what_fits() {
    let cfg = Config { teams: 2, ships_per_team: 1, rock_count: 0, turret_damage: 0.0, boost_cell_chance: 0.0, ..Config::default() };
    let mut world = World::new(cfg, 2);
    world.ships[0].pos = POLE;
    world.ships[1].pos = world.offset(POLE, 2.0, 1500.0);
    // An XXL rock about to break, 300 away so the ship is not standing on it.
    let spot = world.offset(POLE, 0.3, 300.0);
    world.rocks.push(Rock { pos: spot, vel: Vec3::default(), size: 5, hp: 0.0, last_hit_by: Some(0) });
    world.step(&[Action::default(); 2]);
    assert_eq!(world.crystals.len(), 1);
    let pack = world.crystals[0].value;
    assert!((111..=150).contains(&pack), "an XXL pack of {pack}");

    // A tier-1 ship holds 150; with 100 on board it takes 50 and the rest of the pack stays.
    world.ships[0].pos = world.crystals[0].pos;
    world.ships[0].cargo = 100;
    let taken: u32 = world.step(&[Action::default(); 2]).iter().map(|e| if let Event::Collected { ship: 0, amount } = e { *amount } else { 0 }).sum();
    assert_eq!((taken, world.ships[0].cargo), (50, 150));
    assert_eq!(world.crystals.iter().find(|c| !c.boost_cell).map(|c| c.value), Some(pack - 50));
}

#[test]
fn a_ship_passing_close_to_a_pack_picks_it_up() {
    let cfg = Config { teams: 2, ships_per_team: 1, rock_count: 0, turret_damage: 0.0, ..Config::default() };
    let grab = |gap: f32, value: u32| {
        let mut world = World::new(cfg.clone(), 2);
        world.ships[0].pos = POLE;
        world.ships[1].pos = world.offset(POLE, 2.0, 1500.0);
        world.crystals.push(Crystal { pos: world.offset(POLE, 0.3, gap), ticks: 100, value, full: 1_000_000, boost_cell: false });
        world.step(&[Action::default(); 2]);
        world.ships[0].cargo
    };
    // A small pack of 9 has radius 16.5: a tier-1 ship (radius 12) takes it from within 45.
    assert_eq!(grab(42.0, 9), 9);
    assert_eq!(grab(49.0, 9), 0);
    // A big pack is a bigger target: 49 crystals have radius 22.5, so the reach is 57.
    assert_eq!(grab(54.0, 49), 49);
    assert_eq!(grab(61.0, 49), 0);
}

#[test]
fn long_range_radar_shows_far_enemies_and_which_way_the_rocks_are() {
    let cfg = Config { teams: 2, ships_per_team: 1, rock_count: 0, turret_damage: 0.0, ..Config::default() };
    let mut world = World::new(cfg, 4);
    let far_ahead = world.offset(POLE, 0.3, 1250.0);
    world.ships[0].pos = POLE;
    world.face(0, far_ahead);
    // An enemy 1250 away: beyond clear sight (1040), inside radar range (1500).
    world.ships[1].pos = far_ahead;
    let seen = observe(&world, 0);
    assert!(seen.ships.is_empty(), "too far to see clearly or aim at");
    assert_eq!(seen.contacts.len(), 1);
    assert!((seen.contacts[0].x - 1250.0).abs() < 1.0 && seen.contacts[0].y.abs() < 1.0, "dead ahead: {:?}", seen.contacts[0]);
    // Beyond radar range it is not on the radar at all.
    world.ships[1].pos = world.offset(POLE, 0.3, 1700.0);
    assert!(observe(&world, 0).contacts.is_empty());

    // Rocks: a big one far ahead and a small one to the left show up as weight in those directions.
    world.rocks.push(Rock { pos: world.offset(POLE, 0.3, 1350.0), vel: Vec3::default(), size: 5, hp: 250.0, last_hit_by: None });
    let to_the_left = world.offset(POLE, 0.3 + std::f32::consts::FRAC_PI_2, 1200.0);
    world.rocks.push(Rock { pos: to_the_left, vel: Vec3::default(), size: 1, hp: 10.0, last_hit_by: None });
    let radar = observe(&world, 0).rock_radar;
    assert_eq!(radar[0] + radar[7], 5.0, "the big rock is dead ahead: {radar:?}");
    assert_eq!(radar[1] + radar[2], 1.0, "the small rock is to the left: {radar:?}");
    assert_eq!(radar.iter().sum::<f32>(), 6.0);
    assert!(observe(&world, 0).rocks.is_empty(), "neither is close enough to see clearly");
}

#[test]
fn a_base_has_two_places_and_only_ships_at_rest_take_them() {
    let cfg = Config { teams: 2, ships_per_team: 4, rock_count: 0, turret_damage: 0.0, ..Config::default() };
    let mut world = World::new(cfg, 2);
    let base = world.bases[0].pos;
    // Three ships at rest inside the base, well clear of each other, and a fourth flying through it.
    for i in 0..4 {
        world.ships[i].pos = world.offset(base, i as f32 * 2.1, if i < 3 { 38.0 } else { 5.0 });
        world.ships[i].vel = Vec3::default();
        world.ships[i].cargo = 60;
        world.ships[i].points = 50;
    }
    // It leaves through the gap between the first two.
    world.ships[3].vel = geo::bearing(world.ships[3].pos, 1.05).scale(120.0);
    let n = world.ships.len();
    let mut unload = vec![Action::default(); n];
    for a in unload.iter_mut().take(4) {
        *a = Action { deposit: true, buy: Some(Buy::Upgrade(Upgrade::Attack)), ..Action::default() };
    }
    world.step(&unload);
    let docked: Vec<bool> = world.ships[..4].iter().map(|s| s.docked).collect();
    assert_eq!(docked, [true, true, false, false], "two places, first come first served; a moving ship does not dock");
    // Only the docked ships bought and are unloading.
    assert_eq!(world.ships[..4].iter().map(|s| s.points).collect::<Vec<_>>(), [40, 40, 50, 50]);
    for _ in 0..30 {
        world.step(&unload);
    }
    assert!(world.ships[0].cargo < 60 && world.ships[1].cargo < 60);
    assert_eq!(world.ships[2].cargo, 60, "the third waits");
    // Everyone on the team can see that the base is full.
    let seen = observe(&world, 2);
    assert_eq!((seen.in_base, seen.base_docked, seen.base_slots), (false, 2, 2));
    assert!(observe(&world, 0).in_base);

    // One leaves: it gives up its place as soon as it moves, and the waiting ship takes it.
    let mut leave = unload.clone();
    leave[0] = Action { thrust: true, ..Action::default() };
    // Straight out, away from the others.
    world.face(0, world.offset(base, 0.0, 500.0));
    for _ in 0..30 {
        world.step(&leave);
    }
    assert!(!world.ships[0].docked && world.ships[2].docked);
}

#[test]
fn the_hull_mends_from_crystals_first_and_in_the_base() {
    // A ship missing 30 hull picks up a pack of 50: 30 mend the hull, 20 go into the hold.
    let cfg = Config { teams: 2, ships_per_team: 1, rock_count: 0, turret_damage: 0.0, ..Config::default() };
    let mut world = World::new(cfg, 2);
    world.ships[0].pos = POLE;
    world.ships[1].pos = world.offset(POLE, 2.0, 1500.0);
    world.ships[0].hull = 70.0;
    world.crystals.push(Crystal { pos: POLE, ticks: 100, value: 50, full: 1_000_000, boost_cell: false });
    let taken: u32 = world.step(&[Action::default(); 2]).iter().map(|e| if let Event::Collected { ship: 0, amount } = e { *amount } else { 0 }).sum();
    assert_eq!((world.ships[0].hull, world.ships[0].cargo, taken), (100.0, 20, 50));
    // Out in the open nothing else mends it.
    world.ships[0].hull = 70.0;
    for _ in 0..300 {
        world.step(&[Action::default(); 2]);
    }
    assert_eq!(world.ships[0].hull, 70.0);
    // A ship with a full hold still takes crystals while its hull needs them.
    world.ships[0].cargo = world.cfg.cargo_max(&world.ships[0]);
    world.crystals.push(Crystal { pos: POLE, ticks: 100, value: 10, full: 1_000_000, boost_cell: false });
    world.step(&[Action::default(); 2]);
    assert_eq!(world.ships[0].hull, 80.0);

    // Docked in the base: a tenth of the hull a second.
    let mut world = at_home(0);
    world.ships[0].hull = 40.0;
    for _ in 0..30 {
        world.step(&[Action::default(); 2]);
    }
    assert!((world.ships[0].hull - 50.0).abs() < 0.5, "after one second: {}", world.ships[0].hull);
    // Next to the base but not in it: nothing.
    let mut world = at_home(0);
    world.ships[0].pos = world.offset(world.bases[0].pos, 0.0, 100.0);
    world.ships[0].hull = 40.0;
    for _ in 0..30 {
        world.step(&[Action::default(); 2]);
    }
    assert_eq!(world.ships[0].hull, 40.0);
}

#[test]
fn a_destroyed_ship_comes_back_whole_and_empty_and_pays_in_points() {
    let mut world = with_drone(DroneKind::Eco);
    world.ships[0].cargo = 80;
    world.ships[0].points = 55;
    world.ships[0].levels[Upgrade::Attack as usize] = 2;
    world.ships[0].hull = -1000.0;
    let idle = [Action::default(); 2];
    world.step(&idle);
    assert!(!world.ships[0].alive);
    assert!(world.crystals.iter().any(|c| c.value == 40), "half of what it carried was left behind as a pack");
    for _ in 0..world.cfg.respawn_ticks + 1 {
        world.step(&idle);
    }
    let ship = &world.ships[0];
    assert!(ship.alive);
    assert_eq!(ship.cargo, 0, "the hold is empty");
    assert_eq!((ship.hull, ship.shield), (world.cfg.hull_max(ship), world.cfg.shield_max(ship)), "hull and shields are whole");
    assert_eq!(ship.points, 55 - 20 - 11, "it pays 20 points and a fifth of what it had");
    assert_eq!(ship.levels[Upgrade::Attack as usize], 2, "and so are upgrades, while it has lives left");
    assert_eq!(observe(&world, 0).drones, 0, "its drones are no longer its own");
    assert_eq!(world.drones.len(), 1, "they stay where they were, for anyone to take");
}

#[test]
fn a_ship_sees_the_twelve_nearest_rocks_within_its_wider_view() {
    let cfg = Config { teams: 1, ships_per_team: 1, rock_count: 0, ..Config::default() };
    let mut world = World::new(cfg, 2);
    world.ships[0].pos = POLE;
    world.ships[0].fwd = geo::bearing(POLE, 0.0);
    // Fifteen rocks in a line, 70 apart from 100 out: the last one in view is at 1010.
    for k in 0..15 {
        world.rocks.push(Rock { pos: world.offset(POLE, 0.0, 100.0 + 70.0 * k as f32), vel: Vec3::default(), size: 1, hp: 10.0, last_hit_by: None });
    }
    let seen = observe(&world, 0);
    assert_eq!(seen.rocks.len(), 12);
    assert!((seen.rocks[0].dist - 100.0).abs() < 0.5 && (seen.rocks[11].dist - 870.0).abs() < 0.5, "nearest first, out to the twelfth");
    assert!(world.cfg.view_range > 1000.0 && world.cfg.view_range < world.cfg.radar_range);
    // All twelve are in the vector a policy gets: the last six have their rows near the end.
    let v = seen.to_vec(&world.cfg);
    let present = v[OBS_LEN - 73..OBS_LEN - 13].chunks(10).filter(|row| row[0] == 1.0).count();
    assert_eq!(present, 6);
}
