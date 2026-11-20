//! Guards against the observation or the simulation changing by accident: bots, trained
//! policies and the live view depend on every number. A change made for speed has to
//! reproduce these hashes bit for bit; a change to the rules or to what ships observe means
//! recording them again, on purpose. (Last recorded when weapons and rocks doubled and ships began to learn how hard enemies hit.)
use swarm_sim::*;

/// The observation fingerprint and the state hash the match ends in.
fn fingerprint() -> (u64, u64) {
    let mut world = World::new(Config { rock_count: 150, ..Config::default() }, 3);
    let mut bots: Vec<ScriptedBot> = (0..world.ships.len()).map(ScriptedBot::for_ship).collect();
    let mut commander = ScriptedCommander::new(3);
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for tick in 0..1500 {
        if tick % 450 == 0 {
            for team in 0..world.bases.len() as u8 {
                commander.command(&mut world, team);
            }
        }
        let seen: Vec<Observation> = (0..world.ships.len()).map(|i| observe(&world, i)).collect();
        if tick % 25 == 0 {
            for obs in &seen {
                for v in obs.to_vec(&world.cfg) {
                    for b in v.to_bits().to_le_bytes() {
                        h = (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3);
                    }
                }
            }
        }
        // Some ships vote, so the radar and vote fields are exercised too.
        let actions: Vec<Action> = seen
            .iter()
            .enumerate()
            .map(|(i, obs)| {
                let mut act = bots[i].act(obs, &world.cfg);
                if i % 5 == 0 {
                    act.vote = Some(PLANS[(tick / 300 + i) % PLANS.len()]);
                }
                act
            })
            .collect();
        world.step(&actions);
    }
    (h, world.state_hash())
}

#[test]
fn observations_and_the_match_are_what_they_were() {
    let (seen, state) = fingerprint();
    assert_eq!(seen, RECORDED, "the observation vector changed; got {seen:#018x}");
    assert_eq!(state, RECORDED_STATE, "the match itself went differently; got state {state:#018x}");
}

const RECORDED: u64 = 0x0ef86d2311825ff2;
/// Recorded from the straightforward step function; a faster one has to end in the same state.
const RECORDED_STATE: u64 = 0xa3e5f1a4ea19a703;
