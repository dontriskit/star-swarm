//! Draws the planet as a globe seen from space and maps clicks back to positions on it.
//!
//! The view is orthographic: the half of the planet facing the camera is visible, the
//! other half is hidden until the globe is turned.

use std::collections::BTreeSet;

use gpui_kit::*;
use swarm_sim::{OrderKind, Vec3, World, geo};

use crate::{MY_TEAM, TEAM_COLORS};

/// Where the globe is looked at from: the position at the centre of the view, and which way is up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub center: Vec3,
    pub up: Vec3,
}

impl Camera {
    /// Centred on a position, with north up where that means something.
    pub fn looking_at(center: Vec3) -> Self {
        let north = geo::tangent(center, Vec3::new(0.0, 0.0, 1.0));
        let up = if north.len() > 1e-3 { north.unit() } else { geo::basis(center).0 };
        Camera { center, up }
    }

    fn right(&self) -> Vec3 {
        self.up.cross(self.center)
    }

    /// Turns the view: `yaw` brings what is to the right into the centre, `pitch` what is above.
    pub fn turn(&mut self, yaw: f32, pitch: f32) {
        let right = self.right();
        self.center = self.center.scale(yaw.cos()).add(right.scale(yaw.sin())).unit();
        let (center, up) = (self.center, self.up);
        self.center = center.scale(pitch.cos()).add(up.scale(pitch.sin())).unit();
        self.up = geo::tangent(self.center, up.scale(pitch.cos()).sub(center.scale(pitch.sin()))).unit();
    }
}

/// The globe as it sits in the canvas.
pub struct View {
    cx: f32,
    cy: f32,
    /// Radius of the globe on screen, pixels.
    pub radius: f32,
    camera: Camera,
    right: Vec3,
}

impl View {
    pub fn new(area: Bounds<Pixels>, camera: Camera) -> Self {
        let (w, h) = (f32::from(area.size.width), f32::from(area.size.height));
        View {
            cx: f32::from(area.origin.x) + w / 2.0,
            cy: f32::from(area.origin.y) + h / 2.0,
            radius: (w.min(h) / 2.0 * 0.94).max(1.0),
            camera,
            right: camera.right(),
        }
    }

    /// Where a position on the planet is drawn, or None when it is on the far side.
    pub fn at(&self, p: Vec3) -> Option<Point<Pixels>> {
        (p.dot(self.camera.center) > 0.03)
            .then(|| point(px(self.cx + p.dot(self.right) * self.radius), px(self.cy - p.dot(self.camera.up) * self.radius)))
    }

    /// The point on the edge of the globe nearest to where a far-side position would be drawn.
    fn on_edge(&self, p: Vec3) -> Point<Pixels> {
        let (x, y) = (p.dot(self.right), p.dot(self.camera.up));
        let len = (x * x + y * y).sqrt().max(1e-6);
        point(px(self.cx + x / len * self.radius), px(self.cy - y / len * self.radius))
    }

    /// A direction along the surface at `p` as a unit direction on screen (x right, y down).
    fn heading(&self, dir: Vec3) -> (f32, f32) {
        let (x, y) = (dir.dot(self.right), -dir.dot(self.camera.up));
        let len = (x * x + y * y).sqrt().max(1e-6);
        (x / len, y / len)
    }

    /// The position on the planet under a window position, or None outside the globe.
    pub fn to_world(&self, p: Point<Pixels>) -> Option<Vec3> {
        let (x, y) = ((f32::from(p.x) - self.cx) / self.radius, (self.cy - f32::from(p.y)) / self.radius);
        let flat = x * x + y * y;
        (flat < 1.0).then(|| self.right.scale(x).add(self.camera.up.scale(y)).add(self.camera.center.scale((1.0 - flat).sqrt())).unit())
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

/// A circle on the planet's surface as screen points. Where it runs behind the planet it is
/// drawn along the edge of the globe, so the visible part keeps its shape; a circle mostly on
/// the far side gives nothing.
fn ring(world: &World, view: &View, around: Vec3, radius: f32, sides: usize) -> Vec<Point<Pixels>> {
    let points: Vec<Vec3> = (0..sides).map(|i| world.offset(around, std::f32::consts::TAU * i as f32 / sides as f32, radius)).collect();
    // Mostly behind the planet: what little shows would be a sliver along the edge, so leave it out.
    if points.iter().filter(|p| view.at(**p).is_some()).count() * 3 < points.len() {
        return Vec::new();
    }
    points.into_iter().map(|p| view.at(p).unwrap_or_else(|| view.on_edge(p))).collect()
}

/// With `fog`, only what the player's team can see is drawn: enemy ships, shots and crystals
/// show within view range of one of its living ships, and enemy bases show the health the
/// team last saw.
pub fn paint(world: &World, selected: &BTreeSet<usize>, camera: Camera, fog: bool, area: Bounds<Pixels>, window: &mut Window) {
    let view = View::new(area, camera);
    let eyes: Vec<Vec3> = world.ships.iter().filter(|s| s.team == MY_TEAM && s.alive).map(|s| s.pos).collect();
    let in_sight = |p: Vec3| !fog || eyes.iter().any(|e| geo::chord(world.cfg.radius, *e, p) < world.cfg.view_range);
    // Pixels per world unit at the centre of the view.
    let scale = view.radius / world.cfg.radius;
    let middle = point(px(view.cx), px(view.cy));
    fill_shape(window, &polygon(middle, view.radius, 96, 0.0), rgb(0x0d111a));
    stroke_shape(window, &polygon(middle, view.radius, 96, 0.0), 1.0, rgb(0x263046));

    // A faint grid of meridians and parallels, so turning the globe reads as turning.
    for step in 0..12 {
        let lon = std::f32::consts::TAU * step as f32 / 12.0;
        for k in -14..=14 {
            if let Some(p) = view.at(Vec3::from_lon_lat(lon, k as f32 * 0.1)) {
                dot(window, p, 0.6, rgb(0x1c2536));
            }
        }
    }
    for lat in [-1.0f32, -0.5, 0.0, 0.5, 1.0] {
        for k in 0..90 {
            if let Some(p) = view.at(Vec3::from_lon_lat(std::f32::consts::TAU * k as f32 / 90.0, lat)) {
                dot(window, p, 0.6, rgb(0x1c2536));
            }
        }
    }

    for (team, base) in world.bases.iter().enumerate() {
        let Some(center) = view.at(base.pos) else { continue };
        let body = ring(world, &view, base.pos, world.cfg.base_radius, 6);
        if !base.alive() {
            if body.len() >= 3 {
                stroke_shape(window, &body, 1.0, rgb(0x3a4050));
            }
            continue;
        }
        let mut color = team_color(team as u8);
        color.a = 0.3;
        // The turret's reach, then the base itself with its health bar underneath.
        let reach = ring(world, &view, base.pos, world.cfg.turret_range, 48);
        if reach.len() >= 3 {
            stroke_shape(window, &reach, 1.0, color);
        }
        color.a = 0.6;
        if body.len() >= 3 {
            fill_shape(window, &body, color);
        }
        let known = if fog { world.seen_base_hp[MY_TEAM as usize][team] } else { base.hp };
        let (width, health) = (36.0, (known / world.cfg.base_hp).clamp(0.0, 1.0));
        let bar = point(center.x - px(width / 2.0), center.y + px(world.cfg.base_radius * scale + 6.0));
        window.paint_quad(fill(Bounds::new(bar, size(px(width), px(4.0))), rgb(0x2a3040)));
        window.paint_quad(fill(Bounds::new(bar, size(px(width * health), px(4.0))), team_color(team as u8)));
    }

    for (i, rock) in world.rocks.iter().enumerate() {
        if let Some(p) = view.at(rock.pos) {
            let radius = (world.cfg.rock_radius_of(rock.size) * scale).max(1.5);
            fill_shape(window, &polygon(p, radius, 7, i as f32 * 0.9), rgb(0x4a5263));
        }
    }

    // Around each ship of the blue team: the inner circle is how far it sees clearly, the faint
    // outer one how far its long-range radar reaches.
    for eye in &eyes {
        let far = ring(world, &view, *eye, world.cfg.radar_range, 64);
        if far.len() >= 3 {
            stroke_shape(window, &far, 1.0, rgba(0x4c9aff16));
        }
        let reach = ring(world, &view, *eye, world.cfg.view_range, 48);
        if reach.len() >= 3 {
            fill_shape(window, &reach, rgba(0x4c9aff0a));
            stroke_shape(window, &reach, 1.0, rgba(0x4c9aff38));
        }
    }
    // Seen clearly: within sight of one of the blue team's living ships. On the radar: within
    // the long range of one of them, which gives a position but no detail.
    let on_radar = |p: Vec3| eyes.iter().any(|e| geo::chord(world.cfg.radius, *e, p) < world.cfg.view_range);
    let blip = |p: Vec3| eyes.iter().any(|e| geo::chord(world.cfg.radius, *e, p) < world.cfg.radar_range);

    for crystal in world.crystals.iter().filter(|c| in_sight(c.pos)) {
        if let Some(p) = view.at(crystal.pos) {
            // Boost cells are larger and orange.
            // Bigger packs are drawn bigger.
            let (half, color) = if crystal.boost_cell { (2.0, rgb(0xff9f43)) } else { (1.2 + (crystal.value as f32).sqrt() * 0.22, rgb(0x8ef0c8)) };
            dot(window, p, half, color);
        }
    }

    // Drones: a small diamond in the owner's colour, grey when nobody owns it.
    for drone in world.drones.iter().filter(|d| in_sight(d.pos) || d.owner.is_some_and(|o| world.ships[o as usize].team == MY_TEAM)) {
        if let Some(p) = view.at(drone.pos) {
            let color = drone.owner.map_or(rgb(0x8a93a6), |o| team_color(world.ships[o as usize].team));
            fill_shape(window, &polygon(p, 3.5, 4, 0.0), color);
        }
    }

    for shot in world.shots.iter().filter(|s| s.team == MY_TEAM || in_sight(s.pos)) {
        if let Some(p) = view.at(shot.pos) {
            dot(window, p, 1.2, team_color(shot.team));
        }
    }

    // Where your ships have been sent.
    for ship in world.ships.iter().filter(|s| s.team == MY_TEAM && s.order.kind != OrderKind::Mine) {
        if let Some(p) = view.at(ship.order.target) {
            let mut color = team_color(MY_TEAM);
            color.a = 0.8;
            stroke_shape(window, &polygon(p, 9.0, 4, 0.0), 1.5, color);
        }
    }

    for (i, ship) in world.ships.iter().enumerate().filter(|(_, s)| s.alive) {
        let Some(center) = view.at(ship.pos) else { continue };
        if ship.team != MY_TEAM && !in_sight(ship.pos) {
            // Hidden by the fog of war; the long-range radar still shows it as a blip.
            if blip(ship.pos) {
                stroke_shape(window, &polygon(center, 4.0, 4, 0.0), 1.0, team_color(ship.team));
            }
            continue;
        }
        // Ships are drawn larger than life so they stay readable; higher tiers are bigger.
        let radius = (world.cfg.radius_of(ship) * scale * 1.8).max(7.0 * world.cfg.radius_of(ship) / world.cfg.ship_radius);
        let (fx, fy) = view.heading(ship.fwd);
        let corner = |turn: f32, reach: f32| {
            let (s, c) = turn.sin_cos();
            point(center.x + px((fx * c - fy * s) * radius * reach), center.y + px((fx * s + fy * c) * radius * reach))
        };
        // An enemy the blue team can see is a radar contact and gets a marker; one it cannot see
        // (only drawn when watching without fog) is dimmed.
        let spotted = ship.team != MY_TEAM && on_radar(ship.pos);
        let mut color = team_color(ship.team);
        if ship.team != MY_TEAM && !spotted {
            // Not seen clearly by the blue team: dimmer on its long-range radar, dimmest off it.
            color.a = if blip(ship.pos) { 0.6 } else { 0.3 };
        }
        fill_shape(window, &[corner(0.0, 1.0), corner(2.5, 0.9), corner(-2.5, 0.9)], color);
        if spotted {
            stroke_shape(window, &polygon(center, radius + 4.0, 4, std::f32::consts::FRAC_PI_4), 1.0, rgba(0xffffffb0));
        }
        if selected.contains(&i) {
            stroke_shape(window, &polygon(center, radius + 5.0, 24, 0.0), 1.5, rgb(0xffffff));
        }
    }

    minimap(world, camera, &in_sight, area, window);
}

/// The whole planet unrolled in a corner of the view: longitude across, latitude up. Bases are
/// squares, ships dots, and the cross marks where the globe view is centred.
fn minimap(world: &World, camera: Camera, in_sight: &dyn Fn(Vec3) -> bool, area: Bounds<Pixels>, window: &mut Window) {
    let (width, height, margin) = (280.0f32, 140.0f32, 12.0f32);
    let left = f32::from(area.origin.x) + margin;
    let top = f32::from(area.origin.y) + f32::from(area.size.height) - height - margin;
    let place = |p: Vec3| {
        let (lon, lat) = (p.y.atan2(p.x), p.z.clamp(-1.0, 1.0).asin());
        point(
            px(left + (lon + std::f32::consts::PI) / std::f32::consts::TAU * width),
            px(top + (std::f32::consts::FRAC_PI_2 - lat) / std::f32::consts::PI * height),
        )
    };
    let frame = Bounds::new(point(px(left), px(top)), size(px(width), px(height)));
    window.paint_quad(fill(frame, rgba(0x0b0f17e6)));
    window.paint_quad(outline(frame, rgb(0x263046), BorderStyle::Solid));
    // The equator, where the bases sit.
    window.paint_quad(fill(Bounds::new(point(px(left), px(top + height / 2.0)), size(px(width), px(1.0))), rgb(0x1c2536)));

    for rock in &world.rocks {
        dot(window, place(rock.pos), 0.5, rgb(0x3a4252));
    }
    for (team, base) in world.bases.iter().enumerate() {
        let color = if base.alive() { team_color(team as u8) } else { rgb(0x3a4050) };
        dot(window, place(base.pos), 3.5, color);
    }
    for ship in world.ships.iter().filter(|s| s.alive && (s.team == MY_TEAM || in_sight(s.pos))) {
        dot(window, place(ship.pos), if ship.team == MY_TEAM { 2.0 } else { 1.5 }, team_color(ship.team));
    }
    // Where the globe view is looking.
    let eye = place(camera.center);
    window.paint_quad(fill(Bounds::new(point(eye.x - px(5.0), eye.y), size(px(11.0), px(1.0))), rgb(0xffffff)));
    window.paint_quad(fill(Bounds::new(point(eye.x, eye.y - px(5.0)), size(px(1.0), px(11.0))), rgb(0xffffff)));
}
