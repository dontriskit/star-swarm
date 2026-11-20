//! Draws the whole world on a canvas and maps clicks back to world positions.

use std::collections::BTreeSet;

use gpui_kit::*;
use swarm_sim::{OrderKind, Vec2, World};

use crate::{MY_TEAM, TEAM_COLORS};

/// The square the world is drawn into, centred in the canvas.
pub struct View {
    left: f32,
    top: f32,
    scale: f32,
    world_size: f32,
}

impl View {
    pub fn new(area: Bounds<Pixels>, world_size: f32) -> Self {
        let (w, h) = (f32::from(area.size.width), f32::from(area.size.height));
        let side = w.min(h).max(1.0);
        View {
            left: f32::from(area.origin.x) + (w - side) / 2.0,
            top: f32::from(area.origin.y) + (h - side) / 2.0,
            scale: side / world_size,
            world_size,
        }
    }

    fn at(&self, p: Vec2) -> Point<Pixels> {
        point(px(self.left + p.x * self.scale), px(self.top + p.y * self.scale))
    }

    /// The world position under a window position, or None outside the map.
    pub fn to_world(&self, p: Point<Pixels>) -> Option<Vec2> {
        let at = Vec2::new((f32::from(p.x) - self.left) / self.scale, (f32::from(p.y) - self.top) / self.scale);
        let inside = |v: f32| (0.0..self.world_size).contains(&v);
        (inside(at.x) && inside(at.y)).then_some(at)
    }
}

fn polygon(center: Point<Pixels>, radius: f32, sides: usize, turn: f32) -> Vec<Point<Pixels>> {
    (0..sides)
        .map(|i| {
            let a = turn + std::f32::consts::TAU * i as f32 / sides as f32;
            point(center.x + px(radius * a.cos()), center.y + px(radius * a.sin()))
        })
        .collect()
}

fn fill_shape(window: &mut Window, points: &[Point<Pixels>], color: Rgba) {
    let mut path = PathBuilder::fill();
    path.add_polygon(points, true);
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
}

fn stroke_shape(window: &mut Window, points: &[Point<Pixels>], width: f32, color: Rgba) {
    let mut path = PathBuilder::stroke(px(width));
    path.add_polygon(points, true);
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
}

fn dot(window: &mut Window, center: Point<Pixels>, half: f32, color: Rgba) {
    let origin = point(center.x - px(half), center.y - px(half));
    window.paint_quad(fill(Bounds::new(origin, size(px(half * 2.0), px(half * 2.0))), color));
}

fn team_color(team: u8) -> Rgba {
    rgb(TEAM_COLORS[team as usize % TEAM_COLORS.len()])
}

pub fn paint(world: &World, selected: &BTreeSet<usize>, area: Bounds<Pixels>, window: &mut Window) {
    let view = View::new(area, world.cfg.world_size);
    let side = world.cfg.world_size * view.scale;
    window.paint_quad(fill(Bounds::new(point(px(view.left), px(view.top)), size(px(side), px(side))), rgb(0x0d111a)));

    for (team, base) in world.bases.iter().enumerate() {
        let center = view.at(base.pos);
        let radius = world.cfg.base_radius * view.scale;
        if !base.alive() {
            stroke_shape(window, &polygon(center, radius, 6, 0.0), 1.0, rgb(0x3a4050));
            continue;
        }
        let mut color = team_color(team as u8);
        color.a = 0.3;
        // The repair zone, then the base itself with its health bar underneath.
        stroke_shape(window, &polygon(center, world.cfg.base_heal_radius * view.scale, 40, 0.0), 1.0, color);
        color.a = 0.55;
        fill_shape(window, &polygon(center, radius, 6, 0.0), color);
        let (width, health) = (radius * 2.4, (base.hp / world.cfg.base_hp).clamp(0.0, 1.0));
        let bar = point(center.x - px(width / 2.0), center.y + px(radius + 5.0));
        window.paint_quad(fill(Bounds::new(bar, size(px(width), px(4.0))), rgb(0x2a3040)));
        window.paint_quad(fill(Bounds::new(bar, size(px(width * health), px(4.0))), team_color(team as u8)));
    }

    for (i, rock) in world.rocks.iter().enumerate() {
        let radius = world.cfg.rock_radius_of(rock.size) * view.scale;
        fill_shape(window, &polygon(view.at(rock.pos), radius.max(2.0), 7, i as f32 * 0.9), rgb(0x4a5263));
    }

    for crystal in &world.crystals {
        dot(window, view.at(crystal.pos), 1.5, rgb(0x8ef0c8));
    }

    for shot in &world.shots {
        dot(window, view.at(shot.pos), 1.5, team_color(shot.team));
    }

    // Where your ships have been sent.
    for ship in world.ships.iter().filter(|s| s.team == MY_TEAM && s.order.kind != OrderKind::Mine) {
        let mut color = team_color(MY_TEAM);
        color.a = 0.8;
        stroke_shape(window, &polygon(view.at(ship.order.target), 9.0, 4, 0.0), 1.5, color);
    }

    for (i, ship) in world.ships.iter().enumerate().filter(|(_, s)| s.alive) {
        let center = view.at(ship.pos);
        // Tier 2 ships are drawn larger.
        let radius = (world.cfg.ship_radius * view.scale * 1.8).max(9.0) * if ship.tier > 1 { 1.4 } else { 1.0 };
        let nose = Vec2::from_angle(ship.heading);
        let corner = |turn: f32, reach: f32| {
            let dir = Vec2::from_angle(ship.heading + turn);
            point(center.x + px(dir.x * radius * reach), center.y + px(dir.y * radius * reach))
        };
        let hull = [point(center.x + px(nose.x * radius), center.y + px(nose.y * radius)), corner(2.5, 0.9), corner(-2.5, 0.9)];
        fill_shape(window, &hull, team_color(ship.team));
        if selected.contains(&i) {
            stroke_shape(window, &polygon(center, radius + 5.0, 24, 0.0), 1.5, rgb(0xffffff));
        }
    }
}
