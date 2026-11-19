//! Commander GUI: the human leads team 0 by giving orders; every ship flies itself.

mod map;

use std::cell::Cell;
use std::collections::BTreeSet;
use std::rc::Rc;
use std::time::Instant;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{Selectable, Sizable, Theme, ThemeMode};
use gpui_kit::*;
use swarm_sim::{Config, Controller, Order, OrderKind, Replay, ScriptedBot, ScriptedCommander, World, observe};
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

fn order_name(kind: OrderKind) -> &'static str {
    match kind {
        OrderKind::Mine => "Kop",
        OrderKind::Attack => "Atakuj",
        OrderKind::Defend => "Broń",
        OrderKind::Regroup => "Zbiórka",
    }
}

struct Game {
    world: World,
    seed: u64,
    bot: ScriptedBot,
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
    fn new(seed: u64, playback: Option<Replay>) -> Self {
        let seed = playback.as_ref().map_or(seed, |r| r.seed);
        let world = playback.as_ref().map_or_else(|| World::new(Config::default(), seed), Replay::world);
        Game {
            recording: Replay::new(seed, &world.cfg),
            status: match &playback {
                Some(r) => format!("Powtórka: {} s gry.", (r.ticks() as f32 * world.cfg.dt) as u32),
                None => "Wybierz rozkaz, potem kliknij punkt na mapie.".into(),
            },
            world,
            seed,
            bot: ScriptedBot,
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
                (0..self.world.ships.len()).map(|i| self.bot.act(&observe(&self.world, i), &self.world.cfg)).collect()
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
        if self.world.done() {
            let best = (0..self.world.bases.len()).max_by_key(|t| self.team_score(*t as u8)).unwrap_or(0);
            self.status = format!("Koniec meczu. Wygrywają {}.", TEAM_NAMES[best % TEAM_NAMES.len()]);
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
        self.order = kind;
        if self.playback.is_some() {
            return;
        }
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
        } else if self.playback.is_none() && self.order != OrderKind::Mine {
            self.give(Order { kind: self.order, target: at });
        }
    }

    fn toggle(&mut self, ship: usize) {
        if !self.selected.remove(&ship) {
            self.selected.insert(ship);
        }
    }

    fn restart(&mut self) {
        let seed = self.seed.wrapping_add(1);
        *self = Game { map_bounds: self.map_bounds.clone(), speed: self.speed, ..Game::new(seed, None) };
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
            let alive = self.world.ships.iter().filter(|s| s.team == t as u8 && s.alive).count();
            let name = TEAM_NAMES[t % TEAM_NAMES.len()];
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(div().size_3().rounded_sm().bg(rgb(TEAM_COLORS[t % TEAM_COLORS.len()])))
                .child(div().flex_1().child(if t as u8 == MY_TEAM { format!("{name} (Ty)") } else { name.to_string() }))
                .child(div().text_color(muted).child(format!("{alive} w grze")))
                .child(div().w(px(56.)).flex().justify_end().child(self.team_score(t as u8).to_string()))
        });

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
            .child(column(76., "Statek".into()))
            .child(column(84., "Kadłub".into()))
            .child(column(76., "Ładunek".into()))
            .child(column(64., "Ulepsz.".into()))
            .child("Rozkaz");
        let fleet = self.my_ships().enumerate().map(|(n, i)| {
            let ship = &self.world.ships[i];
            let levels: u32 = ship.levels.iter().map(|l| *l as u32).sum();
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
                .child(column(76., format!("Statek {}", n + 1)));
            #[cfg(test)]
            let row = row.debug_selector(move || format!("ship-{n}"));
            if ship.alive {
                row.child(column(84., format!("{:.0}/{:.0}", ship.hull.max(0.0), cfg.hull_max(ship))))
                    .child(column(76., format!("{}/{}", ship.cargo, cfg.cargo_max(ship))))
                    .child(column(64., format!("{levels}/{}", 8 * cfg.upgrade_max as u32)))
                    .child(order_name(ship.order.kind))
            } else {
                row.child(div().text_color(muted).child("odradza się"))
            }
        });

        div()
            .w(px(400.))
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
            .child(div().flex().flex_col().gap_1().children(teams))
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
            .child(div().text_color(muted).child(if self.playback.is_some() { "Powtórka (rozkazy wyłączone)" } else { "Rozkaz" }))
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
    }
}

impl Render for Game {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.advance();
        if self.running() {
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
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match (arg.as_str(), args.next()) {
            ("--seed", Some(v)) => seed = v.parse().unwrap_or_else(|_| fail(&format!("--seed wants a number, got {v}"))),
            ("--replay", Some(path)) => {
                playback = Some(Replay::load(&path).unwrap_or_else(|e| fail(&format!("cannot read replay {path}: {e}"))))
            }
            _ => fail("usage: swarm-gui [--seed N] [--replay FILE]"),
        }
    }
    application().with_assets(assets::Assets).run(move |cx| {
        init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
        open_window(WindowOptions::default(), cx, move |_, cx| cx.new(|_| Game::new(seed, playback)))
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
            // The map is the 800 px square left of the 400 px panel.
            let target = g.world.ships[picked[0]].order.target;
            assert!(g.world.delta(target, Vec2::new(900.0, 1200.0)).len() < 2.0, "target {target:?}");
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
