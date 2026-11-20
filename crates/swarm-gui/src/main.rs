//! Commander GUI: the human leads team 0 by giving orders; every ship flies itself.

mod live;
mod map;

use std::cell::Cell;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{Icon, Selectable, Sizable, Theme, ThemeMode};
use gpui_kit::*;
use map::Camera;
use swarm_sim::{Class, Config, Plan, Controller, HybridBot, ModelBot, Net, Order, OrderKind, Replay, ScriptedBot, ScriptedCommander, UPGRADES, Upgrade, World, observe};

/// The team the human commands.
pub const MY_TEAM: u8 = 0;
/// Enemy commanders rethink their orders this often.
const COMMAND_EVERY: u32 = 450;
/// A click this close to one of your ships (world units) selects it.
const PICK_RADIUS: f32 = 45.0;
/// Game speeds the button cycles through; 0 means as fast as the machine can simulate.
/// How many times real time the game runs; 0 is as fast as the machine goes.
const SPEEDS: [u32; 7] = [1, 2, 4, 8, 16, 32, 0];
/// At full speed, how long one frame may spend simulating.
const FULL_SPEED_BUDGET: Duration = Duration::from_millis(12);
/// How fast the globe turns by itself, radians per second.
const SPIN: f32 = 0.12;
/// How far one press of an arrow button turns the globe, radians.
const NUDGE: f32 = 0.45;

pub const TEAM_COLORS: [u32; 6] = [0x4c9aff, 0xff8a3d, 0x5fd38d, 0xd67bff, 0xf2d04b, 0xff6b81];
const TEAM_NAMES: [&str; 6] = ["Niebiescy", "Pomarańczowi", "Zieloni", "Fioletowi", "Żółci", "Czerwoni"];

fn class_name(class: Class) -> &'static str {
    match class {
        Class::Basic => "Podstawowy",
        Class::Lancer => "Działo",
        Class::Twin => "Krążownik",
        Class::Scatter => "Łowca",
        Class::Ram => "Czołg",
    }
}

/// The icon (our own drawing, as SVG) and Polish name of an upgrade.
fn upgrade_look(upgrade: Upgrade) -> (&'static [u8], &'static str) {
    match upgrade {
        // A blade.
        Upgrade::Attack => (br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 19 L19 5 M14 5 H19 V10 M6 14 L10 18"/></svg>"#, "atak"),
        // A heart.
        Upgrade::Health => (br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 20 L4 12 A4.5 4.5 0 0 1 12 6 A4.5 4.5 0 0 1 20 12 Z"/></svg>"#, "zdrowie"),
        // Three chevrons.
        Upgrade::FireRate => (br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 6 L9 12 L4 18 M10 6 L15 12 L10 18 M16 6 L21 12 L16 18"/></svg>"#, "szybkostrzelność"),
        // A shield.
        Upgrade::Shield => (br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3 L20 6 V12 C20 17 16 20 12 21 C8 20 4 17 4 12 V6 Z"/></svg>"#, "tarcze"),
        // A dial with a needle.
        Upgrade::Speed => (br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 17 A8 8 0 1 1 20 17 M12 17 L16 10"/></svg>"#, "prędkość"),
        // An arrow going round.
        Upgrade::Regen => (br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 12 A8 8 0 1 1 17 6 M17 2 V6 H13"/></svg>"#, "regeneracja tarcz"),
        // A U-turn.
        Upgrade::Agility => (br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 8 H14 A4 4 0 0 1 14 16 H8 M11 13 L8 16 L11 19"/></svg>"#, "zwinność"),
        // Two rounds.
        Upgrade::Ammo => (br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 20 V9 L8 5 L10 9 V20 Z M14 20 V9 L16 5 L18 9 V20 Z"/></svg>"#, "amunicja"),
    }
}

fn plan_name(plan: Option<Plan>) -> &'static str {
    match plan {
        None => "brak",
        Some(Plan::Farm) => "farma",
        Some(Plan::Defend) => "obrona",
        Some(Plan::AttackA) => "atak A",
        Some(Plan::AttackB) => "atak B",
    }
}

/// How a team's ships split over the modes, e.g. "F3 O1 A2 B1" (farm, defend, attack A, attack B).
fn mode_split(world: &World, team: u8) -> String {
    let count = |mode: Plan| world.ships.iter().filter(|s| s.team == team && s.vote == Some(mode)).count();
    let parts: Vec<String> = [("F", Plan::Farm), ("O", Plan::Defend), ("A", Plan::AttackA), ("B", Plan::AttackB)]
        .iter()
        .map(|(letter, mode)| (letter, count(*mode)))
        .filter(|(_, n)| *n > 0)
        .map(|(letter, n)| format!("{letter}{n}"))
        .collect();
    if parts.is_empty() { "brak".into() } else { parts.join(" ") }
}

fn order_name(kind: OrderKind) -> &'static str {
    match kind {
        OrderKind::Mine => "Kop",
        OrderKind::Attack => "Atakuj",
        OrderKind::Defend => "Broń",
        OrderKind::Regroup => "Zbiórka",
    }
}

/// Watching training: every team flies on the newest policy saved under a directory.
struct Watch {
    dir: PathBuf,
    /// The policy file in use and when it was written.
    loaded: Option<(PathBuf, SystemTime)>,
    checked: Instant,
    /// When the current match ended, to pause briefly before the next one.
    ended: Option<Instant>,
    /// The trainer's own match, read as it is played. When there is one, the window shows
    /// it instead of playing matches of its own.
    stream: Option<live::Stream>,
    /// Set when the replayed match no longer matches the trainer's.
    drifted: bool,
    /// The latest lines of the run's metrics, newest last, for the progress table.
    progress: Vec<[String; 5]>,
    /// When the stream was opened and, every few seconds since, how many ticks had arrived:
    /// the trainer's pace, for playing its bursts back at an even speed.
    arrivals: std::collections::VecDeque<(Instant, usize)>,
}

impl Watch {
    /// The most recently written `policy.swp` one level under the directory.
    fn newest(dir: &Path) -> Option<(PathBuf, SystemTime)> {
        std::fs::read_dir(dir)
            .ok()?
            .filter_map(|run| {
                let path = run.ok()?.path().join("policy.swp");
                let written = std::fs::metadata(&path).ok()?.modified().ok()?;
                Some((path, written))
            })
            .max_by_key(|(_, written)| *written)
    }

    /// The most recently written `live.swl` one level under the directory.
    fn newest_stream(dir: &Path) -> Option<PathBuf> {
        std::fs::read_dir(dir)
            .ok()?
            .filter_map(|run| {
                let path = run.ok()?.path().join("live.swl");
                let written = std::fs::metadata(&path).ok()?.modified().ok()?;
                Some((path, written))
            })
            .max_by_key(|(_, written)| *written)
            .map(|(path, _)| path)
    }

    /// The last few training lines of a run as table rows: update, matches won, crystals handed
    /// over per ship per minute, upgrade levels per ship with the base level, share of ships above tier 1.
    fn progress(run_file: &Path) -> Vec<[String; 5]> {
        let text = std::fs::read_to_string(run_file.with_file_name("metrics.jsonl")).unwrap_or_default();
        let rows: Vec<[String; 5]> = text
            .lines()
            .filter(|l| l.contains("\"kind\": \"train\"") && l.contains("\"win_rate\""))
            .map(|line| {
                let number = |key: &str| -> f64 {
                    let Some(start) = line.find(&format!("\"{key}\": ")) else { return 0.0 };
                    let rest = &line[start + key.len() + 4..];
                    rest[..rest.find([',', '}', ']']).unwrap_or(rest.len())].trim().trim_start_matches('[').parse().unwrap_or(0.0)
                };
                [
                    format!("{:.0}", number("update")),
                    format!("{:.0}%", number("win_rate") * 100.0),
                    format!("{:.1}", number("deposited_per_min")),
                    format!("{:.1} · baza {:.1}", number("levels"), number("base_level")),
                    // "tiers" is a list starting with the share still at tier 1.
                    format!("{:.0}%", (1.0 - number("tiers")) * 100.0),
                ]
            })
            .collect();
        rows[rows.len().saturating_sub(8)..].to_vec()
    }

    /// The last training line of the run's metrics, boiled down to the numbers worth watching.
    fn report(policy: &Path) -> String {
        let run = policy.parent().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let metrics = policy.with_file_name("metrics.jsonl");
        let text = std::fs::read_to_string(metrics).unwrap_or_default();
        let Some(line) = text.lines().rev().find(|l| l.contains("\"kind\": \"train\"") && l.contains("\"win_rate\"")) else {
            return format!("Przebieg {run}: trening jeszcze nic nie zapisał.");
        };
        let number = |key: &str| -> Option<f64> {
            let rest = &line[line.find(&format!("\"{key}\": "))? + key.len() + 4..];
            rest[..rest.find([',', '}']).unwrap_or(rest.len())].trim().parse().ok()
        };
        format!(
            "Przebieg {run}, aktualizacja {:.0}: w treningu wygrywa {:.0}% meczów, mecz trwa {:.1} min, {:.1} poziomu ulepszeń na statek.",
            number("update").unwrap_or(0.0),
            number("win_rate").unwrap_or(0.0) * 100.0,
            number("minutes").unwrap_or(0.0),
            number("levels").unwrap_or(0.0),
        )
    }
}

struct Game {
    world: World,
    seed: u64,
    /// One controller per ship.
    pilots: Vec<Box<dyn Controller>>,
    /// Set when a trained network flies your fleet instead of the scripted bot.
    model: Option<Arc<Net>>,
    /// Set when the window is showing training live.
    watch: Option<Watch>,
    commander: ScriptedCommander,
    /// What has been played so far, ready to save.
    recording: Replay,
    /// Set when watching a saved match instead of playing.
    playback: Option<Replay>,
    /// With `--wins`: where training runs are, the saved matches found there (newest first),
    /// and which of them is on screen.
    wins_dir: Option<PathBuf>,
    wins: Vec<PathBuf>,
    win_at: usize,
    /// In a kept match: how many teams (the first ones) flew on the trained policy.
    models: usize,
    selected: BTreeSet<usize>,
    order: OrderKind,
    paused: bool,
    speed: u32,
    last_frame: Instant,
    backlog: f32,
    /// Where the map was last painted, for turning clicks into world positions.
    map_bounds: Rc<Cell<Bounds<Pixels>>>,
    /// The side of the planet on screen.
    camera: Camera,
    /// Whether the globe turns by itself.
    spin: bool,
    /// Where a right-button drag that turns the globe was last seen.
    drag: Option<Point<Pixels>>,
    status: String,
}

impl Game {
    /// Your fleet flies on a trained network while its order is "Kop" (the network was trained
    /// without orders); ships given any other order are flown by the scripted bot. The other
    /// teams stay scripted.
    fn with_model(seed: u64, net: Arc<Net>) -> Self {
        let mut game = Game::new(seed, None);
        for i in game.my_ships().collect::<Vec<_>>() {
            game.pilots[i] = Box::new(HybridBot::new(net.clone(), false));
        }
        game.model = Some(net);
        game.status = "Twoja flota leci na modelu. Inny rozkaz niż Kop oddaje wybrane statki botowi skryptowemu.".into();
        game
    }

    fn new(seed: u64, playback: Option<Replay>) -> Self {
        let seed = playback.as_ref().map_or(seed, |r| r.seed);
        let world = playback.as_ref().map_or_else(|| World::new(Config::default(), seed), Replay::world);
        let camera = Camera::home(&world);
        let mut world = world;
        // You give the orders for your own team; its ships' votes are not counted.
        world.commanded[MY_TEAM as usize] = playback.is_none();
        Game {
            recording: Replay::new(seed, &world.cfg),
            status: match &playback {
                Some(r) => format!("Powtórka: {} s gry.", (r.ticks() as f32 * world.cfg.dt) as u32),
                None => "Wybierz rozkaz, potem kliknij punkt na mapie.".into(),
            },
            pilots: (0..world.ships.len()).map(|i| Box::new(ScriptedBot::for_ship(i)) as Box<dyn Controller>).collect(),
            model: None,
            watch: None,
            world,
            seed,
            commander: ScriptedCommander::new(seed),
            playback,
            wins_dir: None,
            wins: Vec::new(),
            win_at: 0,
            models: 0,
            selected: BTreeSet::new(),
            order: OrderKind::Mine,
            paused: false,
            speed: 1,
            last_frame: Instant::now(),
            backlog: 0.0,
            map_bounds: Rc::new(Cell::new(Bounds::default())),
            camera,
            spin: false,
            drag: None,
        }
    }

    /// Orders are given in a live match, not while watching a replay or training.
    fn takes_orders(&self) -> bool {
        self.playback.is_none() && self.watch.is_none()
    }

    /// Shows training live: every ship of every team flies on the newest policy under `dir`.
    fn watching(seed: u64, dir: PathBuf) -> Self {
        let mut game = Game::new(seed, None);
        game.watch = Some(Watch { dir, loaded: None, checked: Instant::now(), ended: None, stream: None, drifted: false, progress: Vec::new(), arrivals: Default::default() });
        game.status = "Czekam na pierwszy zapisany model (uruchom train/live.sh obok treningu).".into();
        // Nobody is steering, so let the planet turn and show every side in time.
        game.spin = true;
        // Follow the trainer at its own even pace.
        game.speed = 0;
        game.reload();
        game
    }

    /// Starts a fresh match on the newest saved policy, if there is one to load.
    fn reload(&mut self) {
        let Some(watch) = &self.watch else { return };
        let Some((path, written)) = Watch::newest(&watch.dir) else { return };
        // A file caught mid-write fails to load; keep what is playing and try again later.
        let Ok(net) = Net::load(&path) else { return };
        let net = Arc::new(net);
        let seed = self.seed.wrapping_add(1);
        let mut fresh = Game::new(seed, None);
        fresh.pilots = (0..fresh.world.ships.len()).map(|_| Box::new(ModelBot::new(net.clone(), ModelBot::HOLD)) as Box<dyn Controller>).collect();
        // Nobody commands here: every team follows its own ships' votes.
        fresh.world.commanded.iter_mut().for_each(|c| *c = false);
        fresh.status = Watch::report(&path);
        fresh.watch = Some(Watch { dir: watch.dir.clone(), loaded: Some((path, written)), checked: Instant::now(), ended: None, stream: None, drifted: false, progress: Vec::new(), arrivals: Default::default() });
        *self = Game { map_bounds: self.map_bounds.clone(), speed: self.speed, paused: self.paused, camera: self.camera, spin: self.spin, ..fresh };
    }

    /// Plays the trainer's own match from its stream, as far as the chosen speed allows, and
    /// returns false when there is no stream to play.
    fn play_stream(&mut self, elapsed: f32) -> bool {
        let Some(watch) = &mut self.watch else { return false };
        // Every couple of seconds: switch to a newer stream (the next training stage), refresh the table.
        if watch.checked.elapsed() >= Duration::from_secs(2) {
            watch.checked = Instant::now();
            if let Some(path) = Watch::newest_stream(&watch.dir) {
                if watch.stream.as_ref().is_none_or(|s| s.path != path) {
                    watch.stream = live::Stream::open(&path).ok();
                    watch.drifted = false;
                    watch.arrivals.clear();
                }
                watch.progress = Watch::progress(&path);
            }
        }
        let Some(stream) = &mut watch.stream else { return false };
        stream.poll();
        // Until the trainer has written a match from its start there is nothing of its to show.
        if stream.episodes == 0 {
            return false;
        }
        // The trainer plays in bursts: a stretch of the match, then a pause while it learns. Its
        // pace over the last minute is how fast the match can be shown without running dry.
        let now = Instant::now();
        if watch.arrivals.back().is_none_or(|(at, _)| now.duration_since(*at) >= Duration::from_secs(2)) {
            watch.arrivals.push_back((now, stream.ticks));
            while watch.arrivals.front().is_some_and(|(at, _)| now.duration_since(*at) > Duration::from_secs(60)) {
                watch.arrivals.pop_front();
            }
        }
        let pace = match (watch.arrivals.front(), watch.arrivals.back()) {
            (Some((t0, n0)), Some((t1, n1))) if t1.duration_since(*t0) >= Duration::from_secs(10) => Some((n1 - n0) as f32 / t1.duration_since(*t0).as_secs_f32()),
            _ => None,
        };
        // At a fixed speed the trainer may run ahead; past a minute's worth, skip forward to stay current.
        let backlog = stream.backlog();
        let budget = if self.paused {
            0
        } else if backlog > 1800 {
            usize::MAX
        } else if self.speed == 0 {
            // Even playback at the trainer's own pace, a little faster with a lot waiting and a
            // little slower with almost nothing left, so the picture neither lags nor stalls.
            let rate = pace.unwrap_or(1.0 / self.world.cfg.dt) * (backlog as f32 / 300.0).clamp(0.6, 1.6);
            self.backlog += elapsed * rate;
            let whole = self.backlog as usize;
            self.backlog -= whole as f32;
            whole
        } else {
            self.backlog += elapsed * self.speed as f32;
            let whole = (self.backlog / self.world.cfg.dt) as usize;
            self.backlog -= whole as f32 * self.world.cfg.dt;
            whole
        };
        let started = Instant::now();
        let mut played = 0;
        while played < budget && started.elapsed() < FULL_SPEED_BUDGET {
            match stream.queue.pop_front() {
                Some(live::Item::Episode(cfg, seed)) => {
                    self.world = World::new(*cfg, seed);
                    self.seed = seed;
                    self.selected.clear();
                }
                Some(live::Item::Tick(actions)) if actions.len() == self.world.ships.len() => {
                    self.world.step(&actions);
                    played += 1;
                }
                // Ticks from before the first match header cannot be placed; drop them.
                Some(live::Item::Tick(_)) => {}
                Some(live::Item::Hash(hash)) => watch.drifted |= hash != self.world.state_hash(),
                None => break,
            }
        }
        let run = stream.path.parent().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let behind = stream.backlog() as f32 * self.world.cfg.dt;
        self.status = if watch.drifted {
            format!("Przebieg {run}: podgląd rozjechał się z treningiem (inna wersja gry w oknie niż w trenerze).")
        } else {
            let tempo = pace.map_or(String::new(), |p| format!(" Trener gra {:.1}× szybciej niż czas rzeczywisty.", p * self.world.cfg.dt));
            format!("Na żywo: mecz treningowy z przebiegu {run}, seed {}. W buforze {behind:.0} s gry.{tempo}", self.seed)
        };
        true
    }

    /// While watching training: move on to a newer policy once the match in progress has had its time.
    fn follow_training(&mut self) {
        let ticks = self.world.tick;
        let done = self.world.done();
        let Some(watch) = &mut self.watch else { return };
        if done && watch.ended.is_none() {
            watch.ended = Some(Instant::now());
        }
        if watch.checked.elapsed() < Duration::from_secs(2) {
            return;
        }
        watch.checked = Instant::now();
        let newer = Watch::newest(&watch.dir).is_some_and(|(path, written)| watch.loaded.as_ref() != Some(&(path, written)));
        // Leave a finished match on screen for a moment; cut a running one short only after 20 s of play.
        let rested = watch.ended.is_some_and(|at| at.elapsed() > Duration::from_secs(3));
        if rested || (newer && (watch.loaded.is_none() || ticks > 600)) {
            self.reload();
        }
    }

    fn running(&self) -> bool {
        !self.paused && !self.world.done()
    }

    fn my_ships(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.world.ships.len()).filter(|i| self.world.ships[*i].team == MY_TEAM)
    }

    /// Runs as many fixed simulation ticks as real time has earned.
    fn advance(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;
        if self.spin {
            self.camera.turn(elapsed * SPIN, 0.0);
        }
        // With a live stream the trainer's match is the show; nothing is simulated here on its own.
        if self.play_stream(elapsed) {
            return;
        }
        self.follow_training();
        if !self.running() {
            return;
        }
        if self.speed == 0 {
            // Full speed: simulate for as long as a frame can spare, then draw.
            while now.elapsed() < FULL_SPEED_BUDGET && self.running() {
                self.tick();
            }
            self.backlog = 0.0;
            return;
        }
        self.backlog += elapsed * self.speed as f32;
        while self.backlog >= self.world.cfg.dt && self.running() {
            self.backlog -= self.world.cfg.dt;
            self.tick();
        }
    }

    fn tick(&mut self) {
        let actions = match &self.playback {
            Some(replay) => match replay.actions(self.world.tick) {
                Some(actions) => actions,
                None => {
                    self.paused = true;
                    self.status = "Koniec powtórki.".into();
                    return;
                }
            },
            None => {
                if self.world.tick % COMMAND_EVERY == 0 {
                    for team in (0..self.world.bases.len() as u8).filter(|t| *t != MY_TEAM) {
                        self.commander.command(&mut self.world, team);
                    }
                }
                (0..self.world.ships.len()).map(|i| self.pilots[i].act(&observe(&self.world, i), &self.world.cfg)).collect()
            }
        };
        self.world.step(&actions);
        self.recording.record(&self.world, &actions);
        if let Some(replay) = &self.playback {
            if replay.hash_at(self.world.tick).is_some_and(|h| h != self.world.state_hash()) {
                self.paused = true;
                self.status = format!("Powtórka rozjechała się z nagraniem w ticku {}.", self.world.tick);
            }
        }
        if self.world.done() && self.watch.is_none() {
            self.status = match self.world.winner() {
                Some(team) => format!("Koniec meczu. Wygrywają {}: ich baza została jako jedyna.", TEAM_NAMES[team as usize % TEAM_NAMES.len()]),
                None => "Koniec czasu. Nikt nie zniszczył wszystkich cudzych baz.".into(),
            };
        }
    }

    fn team_score(&self, team: u8) -> u32 {
        self.world.ships.iter().filter(|s| s.team == team).map(|s| s.score).sum()
    }

    /// The ships an order goes to: the selection, or the whole fleet when nothing is selected.
    fn addressed(&self) -> Vec<usize> {
        if self.selected.is_empty() { self.my_ships().collect() } else { self.selected.iter().copied().collect() }
    }

    fn give(&mut self, order: Order) {
        let ships = self.addressed();
        for &i in &ships {
            self.world.set_order(i, order);
        }
        let who = if self.selected.is_empty() { "cała flota".to_string() } else { format!("{} zaznaczone", ships.len()) };
        self.status = format!("{}: {}.", order_name(order.kind), who);
    }

    fn choose_order(&mut self, kind: OrderKind) {
        if !self.takes_orders() {
            return;
        }
        self.order = kind;
        if kind == OrderKind::Mine {
            self.give(Order::default());
        } else {
            self.status = format!("{}: kliknij punkt na mapie.", order_name(kind));
        }
    }

    fn click_map(&mut self, position: Point<Pixels>) {
        let view = map::View::new(self.map_bounds.get(), self.camera);
        let Some(at) = view.to_world(position) else { return };
        let near = self
            .my_ships()
            .filter(|i| self.world.ships[*i].alive)
            .map(|i| (i, self.world.dist(self.world.ships[i].pos, at)))
            .filter(|(_, d)| *d < PICK_RADIUS)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((ship, _)) = near {
            self.toggle(ship);
        } else if self.takes_orders() && self.order != OrderKind::Mine {
            self.give(Order { kind: self.order, target: at });
        }
    }

    fn toggle(&mut self, ship: usize) {
        if !self.selected.remove(&ship) {
            self.selected.insert(ship);
        }
    }

    /// The saved matches of the team being trained under a runs directory, newest first.
    fn find_wins(dir: &std::path::Path) -> Vec<PathBuf> {
        // Training keeps them in RUNS/wins; older runs kept them in RUNS/<run>/wins.
        let runs = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|run| run.path().join("wins"));
        let mut found: Vec<(std::time::SystemTime, PathBuf)> = std::iter::once(dir.join("wins"))
            .chain(runs)
            .flat_map(|wins| std::fs::read_dir(wins).into_iter().flatten().flatten())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "replay"))
            .filter_map(|p| Some((p.metadata().ok()?.modified().ok()?, p)))
            .collect();
        found.sort();
        found.into_iter().rev().map(|(_, p)| p).collect()
    }

    /// Shows the matches training has kept: the ones the team being trained won, destroyed a
    /// base in, or came first in on time.
    fn showing_wins(dir: PathBuf) -> Self {
        let mut game = Game::new(1, None);
        game.wins_dir = Some(dir);
        game.show_win(0);
        game
    }

    /// Puts the saved match at this place in the list (0 is the newest) on screen.
    fn show_win(&mut self, at: usize) {
        let Some(dir) = self.wins_dir.clone() else { return };
        let wins = Self::find_wins(&dir);
        if wins.is_empty() {
            // Nothing to show: an empty, stopped screen with the reason, not a game of its own.
            self.status = format!("W {}/wins nie ma jeszcze zapisanych meczów. Trening zapisuje tam mecze, w których jego drużyna wygrała, zburzyła bazę albo skończyła na pierwszym miejscu. Spróbuj ponownie później („Nowy mecz” sprawdza jeszcze raz).", dir.display());
            self.wins = wins;
            self.paused = true;
            return;
        }
        let at = at.min(wins.len() - 1);
        let path = &wins[at];
        let name = path.file_stem().and_then(|n| n.to_str()).unwrap_or("");
        // "...-m2": the first two teams flew on the policy, the rest on the scripted bot.
        let models = name.rsplit_once("-m").and_then(|(_, n)| n.parse().ok()).unwrap_or(1);
        let what = if name.contains("wygrana") {
            "wygrana"
        } else if name.contains("zburzona-baza") {
            "zburzona baza wroga"
        } else if name.contains("rekord") {
            "rekord oddanych kryształów (mecz przegrany lub nierozstrzygnięty)"
        } else {
            "pierwsze miejsce po czasie"
        };
        let run = path.parent().and_then(|p| p.parent()).and_then(|p| p.file_name()).and_then(|n| n.to_str()).unwrap_or("?").to_string();
        match Replay::load(path) {
            Ok(replay) => {
                let status = format!("Mecz {} z {} (od najnowszego): {what}, przebieg {run}.", at + 1, wins.len());
                let fresh = Game::new(0, Some(replay));
                // A kept match never starts at full speed: it would be over before it could be watched.
                let speed = if self.speed == 0 { 8 } else { self.speed };
                *self = Game { map_bounds: self.map_bounds.clone(), speed, spin: self.spin, wins_dir: Some(dir), wins, win_at: at, models, status, ..fresh };
            }
            Err(e) => self.status = format!("Nie da się odczytać {}: {e} (zapis z innej wersji gry?)", path.display()),
        }
    }

    fn restart(&mut self) {
        if self.watch.is_some() {
            return self.reload();
        }
        if self.wins_dir.is_some() {
            return self.show_win(self.win_at);
        }
        let seed = self.seed.wrapping_add(1);
        let fresh = match self.model.clone() {
            Some(net) => Game::with_model(seed, net),
            None => Game::new(seed, None),
        };
        *self = Game { map_bounds: self.map_bounds.clone(), speed: self.speed, camera: self.camera, spin: self.spin, ..fresh };
    }

    fn save(&mut self) {
        let path = format!("star-swarm-{}.replay", self.seed);
        self.status = match self.recording.save(&path) {
            Ok(()) => format!("Zapisano {path} ({} s gry).", (self.recording.ticks() as f32 * self.world.cfg.dt) as u32),
            Err(e) => format!("Nie udało się zapisać {path}: {e}"),
        };
    }

    fn panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let cfg = &self.world.cfg;
        let left = (cfg.max_ticks.saturating_sub(self.world.tick)) as f32 * cfg.dt;
        let muted = rgb(0x8a93a6);

        let teams = (0..self.world.bases.len()).map(|t| {
            let base = &self.world.bases[t];
            // Hit points, level, and crystals handed over towards the next level.
            let base_text = if !base.alive() {
                "zniszczona".to_string()
            } else {
                match self.world.cfg.next_base_level_at(base.level) {
                    Some(next) => format!("{:.0} · poz. {} · {}/{}", base.hp, base.level, base.deposited, next),
                    None => format!("{:.0} · poz. {}", base.hp, base.level),
                }
            };
            let name = TEAM_NAMES[t % TEAM_NAMES.len()];
            let crystals: u32 = self.world.ships.iter().filter(|s| s.team == t as u8).map(|s| s.collected).sum();
            let live = self.watch.as_ref().is_some_and(|w| w.stream.is_some());
            // In a live or kept training match the first teams fly on the policy, the rest on the bot.
            let flown = self.watch.as_ref().and_then(|w| w.stream.as_ref()).map_or(self.models, |s| s.models);
            let who = match (self.watch.is_some(), t as u8 == MY_TEAM, self.model.is_some()) {
                _ if self.wins_dir.is_some() && self.playback.is_none() => format!("{name} (—)"),
                _ if self.wins_dir.is_some() || live => format!("{name} ({})", if t < flown { "model" } else { "skrypt" }),
                (true, _, _) => format!("{name} (model)"),
                (_, true, true) => format!("{name} (Ty, model)"),
                (_, true, false) => format!("{name} (Ty)"),
                (_, false, _) => format!("{name} (skrypt)"),
            };
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(div().size_3().rounded_sm().bg(rgb(TEAM_COLORS[t % TEAM_COLORS.len()])))
                .child(div().flex_1().child(who))
                .child(div().w(px(92.)).text_color(muted).child(if self.world.commanded[t] { "rozkazy".to_string() } else { mode_split(&self.world, t as u8) }))
                .child(div().w(px(176.)).flex().justify_end().child(base_text))
                .child(div().w(px(72.)).flex().justify_end().child(crystals.to_string()))
                .child(div().w(px(56.)).flex().justify_end().child(self.team_score(t as u8).to_string()))
        });
        let teams_header = div()
            .flex()
            .gap_2()
            .text_color(muted)
            .child(div().flex_1().child("Drużyna"))
            .child(div().w(px(92.)).child("Tryby"))
            .child(div().w(px(176.)).flex().justify_end().child("Baza: HP · poziom · oddane"))
            .child(div().w(px(72.)).flex().justify_end().child("Kryształy"))
            .child(div().w(px(56.)).flex().justify_end().child("Wynik"));

        let orders = [OrderKind::Mine, OrderKind::Attack, OrderKind::Defend, OrderKind::Regroup].map(|kind| {
            let button = Button::new(("order", kind as usize)).small().label(order_name(kind)).selected(self.order == kind);
            let button = if self.order == kind { button.primary() } else { button };
            #[cfg(test)]
            let button = button.debug_selector(move || format!("order-{}", kind as usize));
            button.on_click(cx.listener(move |this, _, _, cx| {
                this.choose_order(kind);
                cx.notify();
            }))
        });

        let column = |width: f32, text: String| div().w(px(width)).child(text);
        // A value with its share of the maximum, coloured by how good that is: green, amber, red.
        // For the hold it runs the other way: a full hold is crystals not handed over.
        let (good, fair, bad) = (rgb(0x5ad67d), rgb(0xf0c24a), rgb(0xf06a5a));
        let gauge = move |width: f32, value: f32, max: f32, full_is_bad: bool| {
            let share = if max > 0.0 { (value / max).clamp(0.0, 1.0) } else { 0.0 };
            let color = match (full_is_bad, share) {
                (false, s) if s >= 0.66 => good,
                (false, s) if s >= 0.33 => fair,
                (false, _) => bad,
                (true, s) if s >= 0.95 => bad,
                (true, s) if s >= 0.6 => fair,
                (true, _) => good,
            };
            div().w(px(width)).text_color(color).child(format!("{value:.0}/{max:.0} {:.0}%", share * 100.0))
        };
        let header = div()
            .flex()
            .px_2()
            .text_xs()
            .text_color(muted)
            .child(column(62., "Statek".into()))
            .child(column(98., "Kadłub".into()))
            .child(column(98., "Tarcza".into()))
            .child(column(98., "Ładunek".into()))
            .child(column(48., "Punkty".into()))
            .child(column(82., "Amun.".into()))
            .child(column(56., "Dopalacz".into()))
            .child("Rozkaz");
        let fleet = self.my_ships().enumerate().map(|(n, i)| {
            let ship = &self.world.ships[i];
            let picked = self.selected.contains(&i);
            let row = div()
                .id(("ship", i))
                .flex()
                .px_2()
                .py_1()
                .rounded_md()
                .cursor_pointer()
                .text_xs()
                .bg(if picked { rgb(0x25467a) } else { rgb(0x19202e) })
                .hover(|style| style.bg(rgb(0x2b3b58)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.toggle(i);
                    cx.notify();
                }))
                .child(column(62., format!("Statek {}", n + 1)));
            #[cfg(test)]
            let row = row.debug_selector(move || format!("ship-{n}"));
            if ship.alive {
                row.child(gauge(98., ship.hull.max(0.0), cfg.hull_max(ship), false))
                    .child(gauge(98., ship.shield, cfg.shield_max(ship), false))
                    .child(gauge(98., ship.cargo as f32, cfg.cargo_max(ship) as f32, true))
                    .child(div().w(px(48.)).text_color(if ship.points > 0 { good } else { muted }).child(ship.points.to_string()))
                    .child(gauge(82., ship.ammo, cfg.ammo_max(ship), false))
                    .child(div().w(px(56.)).text_color(if ship.boost >= 0.66 { good } else if ship.boost >= 0.33 { fair } else { bad }).child(format!("{:.0}%", ship.boost * 100.0)))
                    .child(if ship.docked { "w bazie" } else { order_name(ship.order.kind) })
            } else if self.world.bases[MY_TEAM as usize].alive() {
                row.child(div().text_color(muted).child("odradza się"))
            } else {
                row.child(div().text_color(muted).child("zniszczony, baza nie istnieje"))
            }
        });

        // What every ship has become: class, tier, its run of kills, and the level of each upgrade.
        let legend = div().flex().flex_wrap().gap_x_3().text_xs().text_color(muted).children(UPGRADES.iter().map(|u| {
            let (icon, name) = upgrade_look(*u);
            div().flex().items_center().gap_1().child(Icon::default().data(icon).xsmall()).child(name)
        }));
        let tech = self.world.ships.iter().enumerate().map(|(i, ship)| {
            let color = rgb(TEAM_COLORS[ship.team as usize % TEAM_COLORS.len()]);
            let cap = cfg.max_level(ship.tier);
            let levels = UPGRADES.iter().map(|u| {
                let level = ship.levels[*u as usize];
                // Dim when nothing was bought, bright at this tier's cap.
                let tone = if level == 0 { rgb(0x4a5263) } else if level >= cap { rgb(0xf2d04b) } else { rgb(0xd7dce5) };
                div()
                    .w(px(36.))
                    .flex()
                    .items_center()
                    .gap_0p5()
                    .text_color(tone)
                    .child(Icon::default().data(upgrade_look(*u).0).xsmall())
                    .child(level.to_string())
            });
            div()
                .flex()
                .items_center()
                .text_xs()
                .child(div().size_2().mr_1().rounded_sm().bg(color))
                .child(div().w(px(22.)).text_color(muted).child(format!("{}", i % cfg.ships_per_team + 1)))
                .child(div().w(px(104.)).whitespace_nowrap().child(format!("{} T{}", class_name(ship.class), ship.tier)))
                .child(div().w(px(40.)).text_color(muted).child(if ship.tier > 1 { format!("♥{} ⚔{}", ship.lives, ship.streak) } else { format!("⚔{}", ship.streak) }))
                .child(div().w(px(52.)).text_color(muted).child(plan_name(ship.vote)))
                .child(div().w(px(58.)).text_color(muted).child({
                    // Drones by kind: eco, shield, combat.
                    let count = |kind| self.world.drones.iter().filter(|d| d.owner == Some(i as u16) && d.kind == kind).count();
                    format!("e{} t{} b{} r{} R{} m{} M{}", count(swarm_sim::DroneKind::Eco), count(swarm_sim::DroneKind::Shield), count(swarm_sim::DroneKind::Combat), ship.ord[0], ship.ord[1], ship.ord[2], ship.ord[3])
                }))
                .children(levels)
        });

        div()
            .id("panel")
            .overflow_y_scroll()
            .w(px(690.))
            .h_full()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .bg(rgb(0x121722))
            .text_sm()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(div().text_xl().child("Star Swarm"))
                    .child(format!("{:02}:{:02}", left as u32 / 60, left as u32 % 60)),
            )
            .child(div().flex().flex_col().gap_1().child(teams_header).children(teams))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(Button::new("pause").small().label(if self.paused { "Wznów" } else { "Pauza" }).on_click(cx.listener(
                        |this, _, _, cx| {
                            this.paused = !this.paused;
                            cx.notify();
                        },
                    )))
                    // Slower, the speed itself (a click goes one faster, and round to the slowest), faster.
                    .child(Button::new("slower").small().label("−").on_click(cx.listener(|this, _, _, cx| {
                        let at = SPEEDS.iter().position(|s| *s == this.speed).unwrap_or(0);
                        this.speed = SPEEDS[at.saturating_sub(1)];
                        cx.notify();
                    })))
                    .child(Button::new("speed").small().label(match (self.speed, self.watch.as_ref().is_some_and(|w| w.stream.is_some())) {
                        (0, true) => "Szybkość: płynnie".to_string(),
                        (0, false) => "Szybkość: pełna".to_string(),
                        (speed, _) => format!("Szybkość ×{speed}"),
                    }).on_click(cx.listener(
                        |this, _, _, cx| {
                            let at = SPEEDS.iter().position(|s| *s == this.speed).unwrap_or(0);
                            this.speed = SPEEDS[(at + 1) % SPEEDS.len()];
                            cx.notify();
                        },
                    )))
                    .child(Button::new("faster").small().label("+").on_click(cx.listener(|this, _, _, cx| {
                        let at = SPEEDS.iter().position(|s| *s == this.speed).unwrap_or(0);
                        this.speed = SPEEDS[(at + 1).min(SPEEDS.len() - 1)];
                        cx.notify();
                    })))
                    .child(Button::new("restart").small().label("Nowy mecz").on_click(cx.listener(|this, _, _, cx| {
                        this.restart();
                        cx.notify();
                    })))
                    .child(Button::new("save").small().label("Zapisz powtórkę").on_click(cx.listener(|this, _, _, cx| {
                        this.save();
                        cx.notify();
                    })))
                    // With --wins: step through the kept matches.
                    .children(self.wins_dir.is_some().then_some([("newer", "◀ Nowszy", false), ("older", "Starszy ▶", true)]).into_iter().flatten().map(|(id, label, older)| {
                        Button::new(id).small().label(label).on_click(cx.listener(move |this, _, _, cx| {
                            this.show_win(if older { this.win_at + 1 } else { this.win_at.saturating_sub(1) });
                            cx.notify();
                        }))
                    })),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(div().text_color(muted).child("Glob"))
                    .children([("left", "◀", -NUDGE, 0.0), ("right", "▶", NUDGE, 0.0), ("up", "▲", 0.0, NUDGE), ("down", "▼", 0.0, -NUDGE)].map(
                        |(id, label, yaw, pitch)| {
                            Button::new(id).small().label(label).on_click(cx.listener(move |this, _, _, cx| {
                                // A nudge moves the view by the same share of what is on screen at any zoom.
                                this.camera.turn(yaw / this.camera.zoom, pitch / this.camera.zoom);
                                cx.notify();
                            }))
                        },
                    ))
                    .child(Button::new("home").small().label("Moja baza").on_click(cx.listener(|this, _, _, cx| {
                        this.camera = Camera::home(&this.world);
                        cx.notify();
                    })))
                    .children([("zoom-out", "−", 1.0 / 1.5), ("zoom-in", "+", 1.5)].map(|(id, label, factor)| {
                        Button::new(id).small().label(label).on_click(cx.listener(move |this, _, _, cx| {
                            this.camera.zoom_by(factor);
                            cx.notify();
                        }))
                    }))
                    .child(Button::new("spin").small().label(if self.spin { "Obrót: wł." } else { "Obrót: wył." }).on_click(cx.listener(
                        |this, _, _, cx| {
                            this.spin = !this.spin;
                            cx.notify();
                        },
                    )))
                    .child(div().text_xs().text_color(muted).child("przeciągnij prawym przyciskiem, kółko przybliża")),
            )
            .child(div().text_color(muted).child(if self.watch.is_some() {
                "Podgląd treningu (rozkazy wyłączone)"
            } else if self.wins_dir.is_some() && self.playback.is_none() {
                "Brak zapisanych meczów do pokazania (szczegóły niżej)"
            } else if self.playback.is_some() {
                "Powtórka (rozkazy wyłączone)"
            } else if self.model.is_some() {
                "Rozkaz (Kop: leci model; pozostałe: skrypt)"
            } else {
                "Rozkaz"
            }))
            .child(div().flex().gap_2().children(orders))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(div().text_color(muted).child("Twoja flota"))
                    .child(Button::new("all").small().ghost().label("Odznacz wszystkie").on_click(cx.listener(|this, _, _, cx| {
                        this.selected.clear();
                        cx.notify();
                    }))),
            )
            .child(div().flex().flex_col().gap_1().child(header).children(fleet))
            .child(div().text_color(muted).child(
                "Bez zaznaczenia rozkaz dostaje cała flota. Kliknij statek na mapie albo na liście, żeby dowodzić wybranymi.",
            ))
            .child(div().child(self.status.clone()))
            .children(self.watch.as_ref().filter(|w| !w.progress.is_empty()).map(|watch| {
                // How training is going: the latest measurements of the run being watched.
                let cell = |text: String, width: f32| div().w(px(width)).flex().justify_end().child(text);
                let head = ["Aktualizacja", "Wygrane", "Oddane/min", "Ulepszenia · poz. bazy", "Powyżej T1"];
                let widths = [96., 72., 96., 168., 88.];
                div()
                    .flex()
                    .flex_col()
                    .text_xs()
                    .child(div().text_sm().text_color(muted).child("Postęp treningu (niebiescy, ostatnie pomiary)"))
                    .child(div().flex().text_color(muted).children(head.iter().zip(widths).map(|(h, w)| cell(h.to_string(), w))))
                    .children(watch.progress.iter().map(|row| div().flex().children(row.iter().zip(widths).map(|(v, w)| cell(v.clone(), w)))))
            }))
            .child(div().text_color(muted).child("Drzewko: typ, tier, życia (od tieru 2) i seria zabójstw, tryb, drony (eko, tarcza, bojowy), rakiety (r małe, R duże) i miny (m małe, M duże) oraz poziom każdego ulepszenia"))
            .child(legend)
            .child(div().flex().flex_col().children(tech))
    }
}

impl Render for Game {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.advance();
        if self.running() || self.watch.is_some() || self.spin {
            window.request_animation_frame();
        }
        let world = self.world.clone();
        let selected = self.selected.clone();
        let bounds = self.map_bounds.clone();
        let camera = self.camera;
        // Playing, you see what your team sees. Watching training or a replay, you see everything.
        let fog = self.watch.is_none() && self.playback.is_none();
        div()
            .size_full()
            .flex()
            .bg(rgb(0x07090e))
            .text_color(rgb(0xd7dce5))
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseDownEvent, _, cx| {
                            this.click_map(event.position);
                            cx.notify();
                        }),
                    )
                    // The wheel zooms.
                    .on_scroll(cx.listener(|this, event: &ScrollEvent, _, cx| {
                        let lines = f32::from(event.delta.pixel_delta(px(20.0)).y) / 20.0;
                        this.camera.zoom_by(1.15f32.powf(lines.clamp(-5.0, 5.0)));
                        cx.notify();
                    }))
                    // Dragging with the right button turns the globe.
                    .on_mouse_down(MouseButton::Right, cx.listener(|this, event: &MouseDownEvent, _, _| this.drag = Some(event.position)))
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                        let (Some(from), Some(MouseButton::Right)) = (this.drag, event.pressed_button) else {
                            this.drag = None;
                            return;
                        };
                        let globe = map::View::new(this.map_bounds.get(), this.camera).radius;
                        let (dx, dy) = (f32::from(event.position.x - from.x), f32::from(event.position.y - from.y));
                        // Grab-and-pull: the surface follows the pointer.
                        this.camera.turn(-dx / globe, dy / globe);
                        this.drag = Some(event.position);
                        cx.notify();
                    }))
                    .child(
                        canvas(
                            move |area, _, _| {
                                bounds.set(area);
                                area
                            },
                            move |area, _, window, _| map::paint(&world, &selected, camera, fog, area, window),
                        )
                        .size_full(),
                    ),
            )
            .child(self.panel(cx))
    }
}

fn main() {
    let mut seed = 1;
    let mut playback = None;
    let mut model = None;
    let mut watch = None;
    let mut wins = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match (arg.as_str(), args.next()) {
            ("--seed", Some(v)) => seed = v.parse().unwrap_or_else(|_| fail(&format!("--seed wants a number, got {v}"))),
            ("--replay", Some(path)) => {
                playback = Some(Replay::load(&path).unwrap_or_else(|e| fail(&format!("cannot read replay {path}: {e}"))))
            }
            ("--model", Some(path)) => {
                model = Some(Arc::new(Net::load(&path).unwrap_or_else(|e| fail(&format!("cannot read policy {path}: {e}")))))
            }
            ("--watch", Some(dir)) => watch = Some(PathBuf::from(dir)),
            ("--wins", Some(dir)) => wins = Some(PathBuf::from(dir)),
            _ => fail("usage: swarm-gui [--seed N] [--replay FILE | --model POLICY.swp | --watch RUNS_DIR | --wins RUNS_DIR]"),
        }
    }
    application().with_assets(assets::Assets).run(move |cx| {
        init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
        open_window(WindowOptions::default(), cx, move |_, cx| {
            cx.new(|_| match (watch, model) {
                _ if wins.is_some() => Game::showing_wins(wins.unwrap()),
                (Some(dir), _) => Game::watching(seed, dir),
                (None, Some(net)) if playback.is_none() => Game::with_model(seed, net),
                _ => Game::new(seed, playback),
            })
        })
            .expect("failed to open window");
    });
}

fn fail(message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(2)
}

#[cfg(test)]
mod tests {
    // `use super::*` brings in gpui's own `test` macro, so the standard one is named in full.
    use super::*;
    use swarm_sim::Vec3;

    /// A game whose map was painted into a 600 px square at the window origin.
    fn game() -> Game {
        let game = Game::new(1, None);
        game.map_bounds.set(Bounds::new(point(px(0.), px(0.)), size(px(600.), px(600.))));
        game
    }

    fn on_screen(game: &Game, at: Vec3) -> Point<Pixels> {
        map::View::new(game.map_bounds.get(), game.camera).at(at).expect("the position is on the visible side")
    }

    /// A spot on the visible side, away from the ships gathered at the base.
    fn spot(game: &Game) -> Vec3 {
        game.world.offset(game.world.bases[MY_TEAM as usize].pos, 0.5, 400.0)
    }

    #[core::prelude::v1::test]
    fn the_view_opens_on_your_own_base_and_clicks_map_back_to_the_planet() {
        let mut game = game();
        let base = game.world.bases[MY_TEAM as usize].pos;
        let middle = on_screen(&game, base);
        assert!(game.camera.zoom > 1.2, "on a big planet the view opens zoomed in on the home field");
        assert!((f32::from(middle.x) - 300.0).abs() < 0.5 && (f32::from(middle.y) - 300.0).abs() < 0.5);
        let view = map::View::new(game.map_bounds.get(), game.camera);
        let there = spot(&game);
        let back = view.to_world(view.at(there).unwrap()).unwrap();
        assert!(game.world.dist(there, back) < 1.0);
        // Zoomed all the way out the whole globe fits.
        game.camera.zoom = 1.0;
        let view = map::View::new(game.map_bounds.get(), game.camera);
        // The far side of the planet is not drawn, and the corner of the canvas is not on the globe.
        assert!(view.at(base.scale(-1.0)).is_none());
        assert!(view.to_world(point(px(2.), px(2.))).is_none());
    }

    #[core::prelude::v1::test]
    fn turning_the_globe_brings_the_far_side_into_view() {
        let mut game = game();
        game.camera.zoom = 1.0;
        let other = game.world.bases[1].pos;
        // Half a turn in steps; the second base is a third of the way around.
        let mut seen = false;
        for _ in 0..7 {
            game.camera.turn(NUDGE, 0.0);
            let view = map::View::new(game.map_bounds.get(), game.camera);
            seen |= view.at(other).is_some_and(|p| (f32::from(p.x) - 300.0).abs() < 120.0);
            assert!((game.camera.center.len() - 1.0).abs() < 1e-4 && game.camera.up.dot(game.camera.center).abs() < 1e-4);
        }
        assert!(seen, "the second base never came near the middle of the view");
    }

    #[core::prelude::v1::test]
    fn clicking_the_map_sends_the_whole_fleet_when_nothing_is_selected() {
        let mut game = game();
        game.choose_order(OrderKind::Attack);
        let target = spot(&game);
        game.click_map(on_screen(&game, target));
        for ship in &game.world.ships {
            let expected = if ship.team == MY_TEAM { OrderKind::Attack } else { OrderKind::Mine };
            assert_eq!(ship.order.kind, expected);
            if ship.team == MY_TEAM {
                assert!(game.world.dist(ship.order.target, target) < 1.0);
            }
        }
    }

    #[core::prelude::v1::test]
    fn clicking_a_ship_selects_it_and_orders_go_only_to_the_selection() {
        let mut game = game();
        let mine: Vec<usize> = game.my_ships().collect();
        game.click_map(on_screen(&game, game.world.ships[mine[0]].pos));
        assert_eq!(game.selected.len(), 1);
        let picked = *game.selected.iter().next().unwrap();

        game.choose_order(OrderKind::Regroup);
        game.click_map(on_screen(&game, spot(&game)));
        for &i in &mine {
            let expected = if i == picked { OrderKind::Regroup } else { OrderKind::Mine };
            assert_eq!(game.world.ships[i].order.kind, expected);
        }
    }

    #[core::prelude::v1::test]
    fn mine_applies_at_once_and_clicks_off_the_globe_do_nothing() {
        let mut game = game();
        game.camera.zoom = 1.0;
        game.choose_order(OrderKind::Defend);
        game.click_map(point(px(2.), px(2.)));
        assert!(game.world.ships.iter().all(|s| s.order.kind == OrderKind::Mine));
        game.click_map(on_screen(&game, spot(&game)));
        game.choose_order(OrderKind::Mine);
        assert!(game.world.ships.iter().all(|s| s.order.kind == OrderKind::Mine));
    }

    #[core::prelude::v1::test]
    fn a_played_match_saves_as_a_replay_that_plays_back_identically() {
        let mut game = game();
        game.choose_order(OrderKind::Attack);
        game.click_map(on_screen(&game, spot(&game)));
        for _ in 0..900 {
            game.tick();
        }
        let mut watch = Game::new(0, Some(game.recording.clone()));
        for _ in 0..900 {
            watch.tick();
        }
        assert!(!watch.paused, "{}", watch.status);
        assert_eq!(watch.world.state_hash(), game.world.state_hash());
    }
}

/// The live view against a real batched environment, as the trainer drives it.
#[cfg(test)]
mod live_tests {
    use crate::live::{Item, Stream};
    use swarm_env::{Rewards, VecEnv};
    use swarm_sim::{Config, World};

    #[core::prelude::v1::test]
    fn replaying_the_stream_reproduces_the_trainers_match() {
        // Short matches against scripted teams, so several begin and end while streaming.
        let cfg = Config { rock_count: 60, radius: 500.0, max_ticks: 300, base_hp: 100.0, turret_damage: 0.0, ..Config::default() };
        let mut env = VecEnv::with_learners(cfg, 4, 7, 3, Rewards::default(), 1);
        let path = std::env::temp_dir().join(format!("star-swarm-live-{}.swl", std::process::id()));
        env.stream_to(&path).unwrap();
        let mut stream = Stream::open(&path).unwrap();

        let agents = env.agents();
        let (mut actions, mut rewards, mut dones) = (vec![0u16; agents], vec![0f32; agents], vec![0u8; agents]);
        let mut world: Option<World> = None;
        let (mut matches, mut ticks, mut hashes) = (0, 0, 0);
        for step in 0..260 {
            env.scripted(&mut actions);
            env.step(&actions, &mut rewards, &mut dones);
            // Read in small pieces, as a window polling a file that is still being written does.
            if step % 7 == 0 || step == 259 {
                stream.poll();
            }
            while let Some(item) = stream.queue.pop_front() {
                match item {
                    Item::Episode(cfg, seed) => {
                        world = Some(World::new(*cfg, seed));
                        matches += 1;
                    }
                    Item::Tick(actions) => {
                        world.as_mut().expect("ticks come after a match header").step(&actions);
                        ticks += 1;
                    }
                    Item::Hash(hash) => {
                        assert_eq!(world.as_ref().unwrap().state_hash(), hash, "drifted from the trainer at tick {ticks}");
                        hashes += 1;
                    }
                }
            }
        }
        std::fs::remove_file(&path).unwrap();
        // 780 ticks from the very first one: two whole matches of 300 and the start of a third.
        assert_eq!(matches, 3);
        assert!(hashes >= 2 && ticks == 780, "{ticks} ticks, {hashes} hashes");
        // The window's world is the trainer's world, tick for tick.
        assert_eq!(world.unwrap().state_hash(), env.world(0).state_hash());
    }

    #[core::prelude::v1::test]
    fn a_stream_joined_mid_match_starts_with_the_next_match() {
        let cfg = Config { rock_count: 20, radius: 500.0, max_ticks: 300, ..Config::default() };
        let mut env = VecEnv::with_learners(cfg, 2, 7, 3, Rewards::default(), 0);
        let agents = env.agents();
        let (actions, mut rewards, mut dones) = (vec![0u16; agents], vec![0f32; agents], vec![0u8; agents]);
        for _ in 0..10 {
            env.step(&actions, &mut rewards, &mut dones);
        }
        let path = std::env::temp_dir().join(format!("star-swarm-late-{}.swl", std::process::id()));
        env.stream_to(&path).unwrap();
        let mut stream = Stream::open(&path).unwrap();
        for _ in 0..50 {
            env.step(&actions, &mut rewards, &mut dones);
        }
        stream.poll();
        assert_eq!((stream.episodes, stream.queue.len()), (0, 0), "nothing of the match already under way is streamed");
        for _ in 0..60 {
            env.step(&actions, &mut rewards, &mut dones);
        }
        stream.poll();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(stream.episodes, 1);
        assert!(matches!(stream.queue.front(), Some(Item::Episode(..))));
    }
}

/// Kept apart from `tests`: gpui's test macro emits a plain `#[test]`, which a
/// glob import of gpui would resolve back to the macro itself.
#[cfg(test)]
mod window_tests {
    use crate::{Game, map};
    use gpui_kit::{Entity, Modifiers, MouseButton, Pixels, Point, TestAppContext, VisualTestContext, init, px, size};
    use swarm_sim::OrderKind;

    /// Drives the real window: events go through gpui's hit testing and the
    /// handlers wired up in `render`, not straight into `Game`.
    #[gpui_kit::test]
    fn clicks_in_the_window_reach_the_fleet(cx: &mut TestAppContext) {
        cx.update(init);
        let (game, cx) = cx.add_window_view(|_, _| Game { paused: true, ..Game::new(1, None) });
        cx.simulate_resize(size(px(1200.), px(800.)));
        cx.run_until_parked();

        let centre = |cx: &mut VisualTestContext, name: &'static str| cx.debug_bounds(name).unwrap_or_else(|| panic!("{name} was not painted")).center();

        // Select the first ship from the list.
        let row = centre(cx, "ship-0");
        cx.simulate_click(row, Modifiers::default());
        cx.run_until_parked();
        let picked = game.read_with(cx, |g, _| g.selected.iter().copied().collect::<Vec<_>>());
        assert_eq!(picked.len(), 1);

        // Choose "Zbiórka", then click a point on the globe, away from the ships at the base.
        let regroup = centre(cx, "order-3");
        cx.simulate_click(regroup, Modifiers::default());
        cx.run_until_parked();
        assert_eq!(game.read_with(cx, |g, _| g.order), OrderKind::Regroup);

        let (click, expected) = game.read_with(cx, |g, _| {
            let view = map::View::new(g.map_bounds.get(), g.camera);
            let target = g.world.offset(g.world.bases[0].pos, 0.5, 400.0);
            (view.at(target).unwrap(), target)
        });
        cx.simulate_click(click, Modifiers::default());
        cx.run_until_parked();
        game.read_with(cx, |g, _| {
            for (i, ship) in g.world.ships.iter().enumerate() {
                let expected = if i == picked[0] { OrderKind::Regroup } else { OrderKind::Mine };
                assert_eq!(ship.order.kind, expected, "ship {i}");
            }
            let target = g.world.ships[picked[0]].order.target;
            assert!(g.world.dist(target, expected) < 10.0, "target {target:?}");
        });
    }

    /// In a running match the window is rebuilt every frame, so a redraw
    /// always lands between a press and its release. Clicks must survive it.
    #[gpui_kit::test]
    fn clicks_survive_a_redraw_between_press_and_release(cx: &mut TestAppContext) {
        cx.update(init);
        let (game, cx) = cx.add_window_view(|_, _| Game { paused: true, ..Game::new(1, None) });
        cx.simulate_resize(size(px(1200.), px(800.)));
        cx.run_until_parked();

        let slow_click = |cx: &mut VisualTestContext, game: &Entity<Game>, at: Point<Pixels>| {
            cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::default());
            game.update(cx, |_, cx| cx.notify());
            cx.run_until_parked();
            cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::default());
            cx.run_until_parked();
        };

        let row = cx.debug_bounds("ship-1").expect("ship row was not painted").center();
        slow_click(cx, &game, row);
        assert_eq!(game.read_with(cx, |g, _| g.selected.len()), 1);

        let attack = cx.debug_bounds("order-1").expect("order button was not painted").center();
        slow_click(cx, &game, attack);
        assert_eq!(game.read_with(cx, |g, _| g.order), OrderKind::Attack);
    }
}
