//! Geometry of the playing field: the surface of a sphere.
//!
//! A position is a unit vector from the planet's centre. Velocities and
//! headings are vectors tangent to the surface at that position, in world
//! units. Distances are measured along the surface.

use crate::sim::Vec2;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Vec3 { x, y, z }
    }
    pub fn add(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
    pub fn sub(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
    pub fn scale(self, k: f32) -> Vec3 {
        Vec3::new(self.x * k, self.y * k, self.z * k)
    }
    pub fn dot(self, o: Vec3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    pub fn cross(self, o: Vec3) -> Vec3 {
        Vec3::new(self.y * o.z - self.z * o.y, self.z * o.x - self.x * o.z, self.x * o.y - self.y * o.x)
    }
    pub fn len(self) -> f32 {
        self.dot(self).sqrt()
    }
    /// The same direction with length 1; the zero vector stays zero.
    pub fn unit(self) -> Vec3 {
        let len = self.len();
        if len > 1e-9 { self.scale(1.0 / len) } else { Vec3::default() }
    }
    /// The position at a longitude and latitude, in radians.
    pub fn from_lon_lat(lon: f32, lat: f32) -> Vec3 {
        Vec3::new(lat.cos() * lon.cos(), lat.cos() * lon.sin(), lat.sin())
    }
}

/// The part of `v` that lies along the surface at `pos`.
pub fn tangent(pos: Vec3, v: Vec3) -> Vec3 {
    v.sub(pos.scale(v.dot(pos)))
}

/// Some direction along the surface at `pos`, and the one a quarter turn to its left.
/// Any pair will do where only an angle relative to it matters.
pub fn basis(pos: Vec3) -> (Vec3, Vec3) {
    let pole = if pos.z.abs() < 0.9 { Vec3::new(0.0, 0.0, 1.0) } else { Vec3::new(1.0, 0.0, 0.0) };
    let east = pole.cross(pos).unit();
    (east, pos.cross(east))
}

/// The direction along the surface at `pos` at `angle` in its [`basis`].
pub fn bearing(pos: Vec3, angle: f32) -> Vec3 {
    let (e, n) = basis(pos);
    e.scale(angle.cos()).add(n.scale(angle.sin()))
}

/// Turns a tangent vector at `pos` to the left by `angle`.
pub fn turn(pos: Vec3, v: Vec3, angle: f32) -> Vec3 {
    v.scale(angle.cos()).add(pos.cross(v).scale(angle.sin()))
}

/// The point reached by travelling `dist` from `pos` in the unit tangent direction `dir`.
pub fn travel(radius: f32, pos: Vec3, dir: Vec3, dist: f32) -> Vec3 {
    let a = dist / radius;
    pos.scale(a.cos()).add(dir.scale(a.sin())).unit()
}

/// The point a short tangent offset away from `pos`.
pub fn nudge(radius: f32, pos: Vec3, offset: Vec3) -> Vec3 {
    pos.add(offset.scale(1.0 / radius)).unit()
}

/// Moves a body for `dt` and returns its new position and its velocity carried along with it.
pub fn advance(radius: f32, pos: Vec3, vel: Vec3, dt: f32) -> (Vec3, Vec3) {
    let next = nudge(radius, pos, vel.scale(dt));
    (next, tangent(next, vel).unit().scale(vel.len()))
}

/// Straight-line distance between two positions, in world units. For nearby points it is
/// the distance along the surface, and it is cheap, so collisions use it.
pub fn chord(radius: f32, a: Vec3, b: Vec3) -> f32 {
    a.sub(b).len() * radius
}

/// Distance along the surface.
pub fn dist(radius: f32, a: Vec3, b: Vec3) -> f32 {
    radius * a.dot(b).clamp(-1.0, 1.0).acos()
}

/// The tangent vector at `from` that points along the shortest way to `to` and is as long
/// as that way. Zero when `to` is not a position (the zero vector) or is `from` itself.
pub fn toward(radius: f32, from: Vec3, to: Vec3) -> Vec3 {
    tangent(from, to).unit().scale(dist(radius, from, to) * (to.len() > 0.5) as u8 as f32)
}

/// A tangent vector at `pos` in the frame of a ship there facing `fwd`: x forward, y to the left.
pub fn frame(pos: Vec3, fwd: Vec3, v: Vec3) -> Vec2 {
    Vec2::new(v.dot(fwd), v.dot(pos.cross(fwd)))
}
