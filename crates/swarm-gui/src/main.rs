//! Commander GUI: the human leads team 0 by giving orders; every ship flies itself.

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
use swarm_sim::{Class, Config, Controller, HybridBot, ModelBot, Net, Order, OrderKind, Replay, ScriptedBot, ScriptedCommander, UPGRADES, Upgrade, World, observe};
#[cfg(test)]
use swarm_sim::Vec2;

/// The team the human commands.
pub const MY_TEAM: u8 = 0;
/// Enemy commanders rethink their orders this often.
const COMMAND_EVERY: u32 = 450;
/// A click this close to one of your ships (world units) selects it.
const PICK_RADIUS: f32 = 45.0;
const SPEEDS: [u32; 4] = [1, 2, 4, 8];

pub const TEAM_COLORS: [u32; 6] = [0x4c9aff, 0xff8a3d, 0x5fd38d, 0xd67bff, 0xf2d04b, 0xff6b81];
const TEAM_NAMES: [&str; 6] = ["Niebiescy", "Pomarańczowi", "Zieloni", "Fioletowi", "Żółci", "Czerwoni"];

fn class_name(class: Class) -> &'static str {
    match class {
        Class::Basic => "Podstawowy",
        Class::Lancer => "Lanca",
        Class::Twin => "Bliźniak",
        Class::Scatter => "Rozrzut",
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
    selected: BTreeSet<usize>,
    order: OrderKind,
    paused: bool,
    speed: u32,
    last_frame: Instant,
    backlog: f32,
    /// Where the map was last painted, for turning clicks into world positions.
    map_bounds: Rc<Cell<Bounds<Pixels>>>,
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
            selected: BTreeSet::new(),
            order: OrderKind::Mine,
            paused: false,
            speed: 1,
            last_frame: Instant::now(),
            backlog: 0.0,
            map_bounds: Rc::new(Cell::new(Bounds::default())),
        }
    }

    /// Orders are given in a live match, not while watching a replay or training.
    fn takes_orders(&self) -> bool {
        self.playback.is_none() && self.watch.is_none()
    }

    /// Shows training live: every ship of every team flies on the newest policy under `dir`.
    fn watching(seed: u64, dir: PathBuf) -> Self {
        let mut game = Game::new(seed, None);
        game.watch = Some(Watch { dir, loaded: None, checked: Instant::now(), ended: None });
        game.status = "Czekam na pierwszy zapisany model (uruchom train/live.sh obok treningu).".into();
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
        fresh.status = Watch::report(&path);
        fresh.watch = Some(Watch { dir: watch.dir.clone(), loaded: Some((path, written)), checked: Instant::now(), ended: None });
        *self = Game { map_bounds: self.map_bounds.clone(), speed: self.speed, paused: self.paused, ..fresh };
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
        self.follow_training();
        if !self.running() {
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
        let view = map::View::new(self.map_bounds.get(), self.world.cfg.world_size);
        let Some(at) = view.to_world(position) else { return };
        let near = self
            .my_ships()
            .filter(|i| self.world.ships[*i].alive)
            .map(|i| (i, self.world.delta(self.world.ships[i].pos, at).len()))
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

    fn restart(&mut self) {
        if self.watch.is_some() {
            return self.reload();
        }
        let seed = self.seed.wrapping_add(1);
        let fresh = match self.model.clone() {
            Some(net) => Game::with_model(seed, net),
            None => Game::new(seed, None),
        };
        *self = Game { map_bounds: self.map_bounds.clone(), speed: self.speed, ..fresh };
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
            let base_text = if base.alive() { format!("{:.0}", base.hp) } else { "zniszczona".to_string() };
            let name = TEAM_NAMES[t % TEAM_NAMES.len()];
            let crystals: u32 = self.world.ships.iter().filter(|s| s.team == t as u8).map(|s| s.collected).sum();
            let who = match (self.watch.is_some(), t as u8 == MY_TEAM, self.model.is_some()) {
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
                .child(div().w(px(84.)).flex().justify_end().child(base_text))
                .child(div().w(px(72.)).flex().justify_end().child(crystals.to_string()))
                .child(div().w(px(56.)).flex().justify_end().child(self.team_score(t as u8).to_string()))
        });
        let teams_header = div()
            .flex()
            .gap_2()
            .text_color(muted)
            .child(div().flex_1().child("Drużyna"))
            .child(div().w(px(84.)).flex().justify_end().child("Baza (HP)"))
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
        let header = div()
            .flex()
            .px_2()
            .text_color(muted)
            .child(column(72., "Statek".into()))
            .child(column(72., "Kadłub".into()))
            .child(column(64., "Tarcza".into()))
            .child(column(68., "Ładunek".into()))
            .child(column(56., "Amun.".into()))
            .child(column(64., "Dopalacz".into()))
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
                .bg(if picked { rgb(0x25467a) } else { rgb(0x19202e) })
                .hover(|style| style.bg(rgb(0x2b3b58)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.toggle(i);
                    cx.notify();
                }))
                .child(column(72., format!("Statek {}", n + 1)));
            #[cfg(test)]
            let row = row.debug_selector(move || format!("ship-{n}"));
            if ship.alive {
                row.child(column(72., format!("{:.0}/{:.0}", ship.hull.max(0.0), cfg.hull_max(ship))))
                    .child(column(64., format!("{:.0}/{:.0}", ship.shield, cfg.shield_max(ship))))
                    .child(column(68., format!("{}/{}", ship.cargo, cfg.cargo_max(ship))))
                    .child(column(56., format!("{:.0}/{:.0}", ship.ammo, cfg.ammo_max(ship))))
                    .child(column(64., format!("{:.0}%", ship.boost * 100.0)))
                    .child(order_name(ship.order.kind))
            } else if self.world.bases[MY_TEAM as usize].alive() {
                row.child(div().text_color(muted).child("odradza się"))
            } else {
                row.child(div().text_color(muted).child("zniszczony, baza nie istnieje"))
            }
        });

        // What every ship has become: class, tier, lives, and the level of each upgrade.
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
                .child(div().w(px(40.)).text_color(muted).child(format!("♥{}", ship.lives)))
                .children(levels)
        });

        div()
            .id("panel")
            .overflow_y_scroll()
            .w(px(540.))
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
                    .child(Button::new("speed").small().label(format!("Szybkość ×{}", self.speed)).on_click(cx.listener(
                        |this, _, _, cx| {
                            let at = SPEEDS.iter().position(|s| *s == this.speed).unwrap_or(0);
                            this.speed = SPEEDS[(at + 1) % SPEEDS.len()];
                            cx.notify();
                        },
                    )))
                    .child(Button::new("restart").small().label("Nowy mecz").on_click(cx.listener(|this, _, _, cx| {
                        this.restart();
                        cx.notify();
                    })))
                    .child(Button::new("save").small().label("Zapisz powtórkę").on_click(cx.listener(|this, _, _, cx| {
                        this.save();
                        cx.notify();
                    }))),
            )
            .child(div().text_color(muted).child(if self.watch.is_some() {
                "Podgląd treningu: wszystkie drużyny lecą na najnowszym modelu (rozkazy wyłączone)"
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
            .child(div().text_color(muted).child("Drzewko: typ, tier, życia i poziom każdego ulepszenia"))
            .child(legend)
            .child(div().flex().flex_col().children(tech))
    }
}

impl Render for Game {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.advance();
        if self.running() || self.watch.is_some() {
            window.request_animation_frame();
        }
        let world = self.world.clone();
        let selected = self.selected.clone();
        let bounds = self.map_bounds.clone();
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
                    .child(
                        canvas(
                            move |area, _, _| {
                                bounds.set(area);
                                area
                            },
                            move |area, _, window, _| map::paint(&world, &selected, area, window),
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
            _ => fail("usage: swarm-gui [--seed N] [--replay FILE | --model POLICY.swp | --watch RUNS_DIR]"),
        }
    }
    application().with_assets(assets::Assets).run(move |cx| {
        init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
        open_window(WindowOptions::default(), cx, move |_, cx| {
            cx.new(|_| match (watch, model) {
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

    /// A game whose map was painted into a 600 px square at the window origin.
    fn game() -> Game {
        let game = Game::new(1, None);
        game.map_bounds.set(Bounds::new(point(px(0.), px(0.)), size(px(600.), px(600.))));
        game
    }

    fn on_screen(game: &Game, at: Vec2) -> Point<Pixels> {
        let scale = 600.0 / game.world.cfg.world_size;
        point(px(at.x * scale), px(at.y * scale))
    }

    #[core::prelude::v1::test]
    fn clicking_the_map_sends_the_whole_fleet_when_nothing_is_selected() {
        let mut game = game();
        game.choose_order(OrderKind::Attack);
        let target = Vec2::new(400.0, 400.0);
        game.click_map(on_screen(&game, target));
        for ship in &game.world.ships {
            let expected = if ship.team == MY_TEAM { OrderKind::Attack } else { OrderKind::Mine };
            assert_eq!(ship.order.kind, expected);
            if ship.team == MY_TEAM {
                assert!(game.world.delta(ship.order.target, target).len() < 1.0);
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
        game.click_map(on_screen(&game, Vec2::new(1200.0, 1200.0)));
        for &i in &mine {
            let expected = if i == picked { OrderKind::Regroup } else { OrderKind::Mine };
            assert_eq!(game.world.ships[i].order.kind, expected);
        }
    }

    #[core::prelude::v1::test]
    fn mine_applies_at_once_and_clicks_outside_the_map_do_nothing() {
        let mut game = game();
        game.choose_order(OrderKind::Defend);
        game.click_map(point(px(900.), px(100.)));
        assert!(game.world.ships.iter().all(|s| s.order.kind == OrderKind::Mine));
        game.click_map(on_screen(&game, Vec2::new(300.0, 300.0)));
        game.choose_order(OrderKind::Mine);
        assert!(game.world.ships.iter().all(|s| s.order.kind == OrderKind::Mine));
    }

    #[core::prelude::v1::test]
    fn a_played_match_saves_as_a_replay_that_plays_back_identically() {
        let mut game = game();
        game.choose_order(OrderKind::Attack);
        game.click_map(on_screen(&game, Vec2::new(500.0, 1900.0)));
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

/// Kept apart from `tests`: gpui's test macro emits a plain `#[test]`, which a
/// glob import of gpui would resolve back to the macro itself.
#[cfg(test)]
mod window_tests {
    use crate::Game;
    use gpui_kit::{Entity, Modifiers, MouseButton, Pixels, Point, TestAppContext, VisualTestContext, init, point, px, size};
    use swarm_sim::{OrderKind, Vec2};

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

        // Choose "Zbiórka", then click a point on the map.
        let regroup = centre(cx, "order-3");
        cx.simulate_click(regroup, Modifiers::default());
        cx.run_until_parked();
        assert_eq!(game.read_with(cx, |g, _| g.order), OrderKind::Regroup);

        cx.simulate_click(point(px(300.), px(400.)), Modifiers::default());
        cx.run_until_parked();
        game.read_with(cx, |g, _| {
            for (i, ship) in g.world.ships.iter().enumerate() {
                let expected = if i == picked[0] { OrderKind::Regroup } else { OrderKind::Mine };
                assert_eq!(ship.order.kind, expected, "ship {i}");
            }
            // The map is the 660 px square in the 660 x 800 area left of the 540 px panel.
            let scale = g.world.cfg.world_size / 660.0;
            let expected = Vec2::new(300.0 * scale, (400.0 - 70.0) * scale);
            let target = g.world.ships[picked[0]].order.target;
            assert!(g.world.delta(target, expected).len() < 2.0, "target {target:?}");
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
