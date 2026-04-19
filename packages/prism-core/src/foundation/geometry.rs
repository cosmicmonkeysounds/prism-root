//! Pure 2D geometry primitives built on `glam`.
//!
//! These types are the spatial vocabulary shared across `prism-builder`
//! (layout computation), `prism-shell` (hit-testing, selection), and
//! `prism-relay` (absolute-positioned HTML output). See ADR-003.

use glam::Vec2;
use serde::{Deserialize, Serialize};

/// A 2D point in some coordinate space.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point2 {
    pub x: f32,
    pub y: f32,
}

impl Point2 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn to_vec2(self) -> Vec2 {
        Vec2::new(self.x, self.y)
    }

    pub fn distance(self, other: Self) -> f32 {
        self.to_vec2().distance(other.to_vec2())
    }
}

impl From<Vec2> for Point2 {
    fn from(v: Vec2) -> Self {
        Self { x: v.x, y: v.y }
    }
}

impl From<Point2> for Vec2 {
    fn from(p: Point2) -> Self {
        Self::new(p.x, p.y)
    }
}

/// A 2D size (width, height). Always non-negative after construction
/// via [`Size2::new`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Size2 {
    pub width: f32,
    pub height: f32,
}

impl Size2 {
    pub const ZERO: Self = Self {
        width: 0.0,
        height: 0.0,
    };

    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width: width.max(0.0),
            height: height.max(0.0),
        }
    }

    pub fn area(self) -> f32 {
        self.width * self.height
    }

    pub fn aspect_ratio(self) -> f32 {
        if self.height == 0.0 {
            0.0
        } else {
            self.width / self.height
        }
    }

    pub fn to_vec2(self) -> Vec2 {
        Vec2::new(self.width, self.height)
    }
}

impl Default for Size2 {
    fn default() -> Self {
        Self::ZERO
    }
}

/// An axis-aligned rectangle defined by origin + size.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub origin: Point2,
    pub size: Size2,
}

impl Rect {
    pub const ZERO: Self = Self {
        origin: Point2::ZERO,
        size: Size2::ZERO,
    };

    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            origin: Point2::new(x, y),
            size: Size2::new(width, height),
        }
    }

    pub fn from_origin_size(origin: Point2, size: Size2) -> Self {
        Self { origin, size }
    }

    pub fn x(self) -> f32 {
        self.origin.x
    }

    pub fn y(self) -> f32 {
        self.origin.y
    }

    pub fn width(self) -> f32 {
        self.size.width
    }

    pub fn height(self) -> f32 {
        self.size.height
    }

    pub fn right(self) -> f32 {
        self.origin.x + self.size.width
    }

    pub fn bottom(self) -> f32 {
        self.origin.y + self.size.height
    }

    pub fn center(self) -> Point2 {
        Point2::new(
            self.origin.x + self.size.width * 0.5,
            self.origin.y + self.size.height * 0.5,
        )
    }

    pub fn contains(self, point: Point2) -> bool {
        point.x >= self.origin.x
            && point.x <= self.right()
            && point.y >= self.origin.y
            && point.y <= self.bottom()
    }

    pub fn intersects(self, other: Rect) -> bool {
        self.origin.x < other.right()
            && self.right() > other.origin.x
            && self.origin.y < other.bottom()
            && self.bottom() > other.origin.y
    }

    pub fn union(self, other: Rect) -> Rect {
        let x = self.origin.x.min(other.origin.x);
        let y = self.origin.y.min(other.origin.y);
        let right = self.right().max(other.right());
        let bottom = self.bottom().max(other.bottom());
        Rect::new(x, y, right - x, bottom - y)
    }

    pub fn inset(self, edges: &Edges<f32>) -> Rect {
        Rect::new(
            self.origin.x + edges.left,
            self.origin.y + edges.top,
            (self.size.width - edges.left - edges.right).max(0.0),
            (self.size.height - edges.top - edges.bottom).max(0.0),
        )
    }

    pub fn outset(self, edges: &Edges<f32>) -> Rect {
        Rect::new(
            self.origin.x - edges.left,
            self.origin.y - edges.top,
            self.size.width + edges.left + edges.right,
            self.size.height + edges.top + edges.bottom,
        )
    }
}

impl Default for Rect {
    fn default() -> Self {
        Self::ZERO
    }
}

/// Four-sided edge values (top, right, bottom, left). Used for margins,
/// padding, and border widths.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Edges<T> {
    pub top: T,
    pub right: T,
    pub bottom: T,
    pub left: T,
}

impl<T: Copy> Edges<T> {
    pub const fn new(top: T, right: T, bottom: T, left: T) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    pub const fn all(value: T) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    pub const fn symmetric(vertical: T, horizontal: T) -> Self {
        Self {
            top: vertical,
            right: horizontal,
            bottom: vertical,
            left: horizontal,
        }
    }
}

impl<T: Copy + Default> Default for Edges<T> {
    fn default() -> Self {
        Self::all(T::default())
    }
}

impl Edges<f32> {
    pub const ZERO: Self = Self::all(0.0);

    pub fn horizontal(self) -> f32 {
        self.left + self.right
    }

    pub fn vertical(self) -> f32 {
        self.top + self.bottom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_distance() {
        let a = Point2::new(0.0, 0.0);
        let b = Point2::new(3.0, 4.0);
        assert!((a.distance(b) - 5.0).abs() < f32::EPSILON);
    }

    #[test]
    fn size_clamps_negative() {
        let s = Size2::new(-10.0, -5.0);
        assert_eq!(s.width, 0.0);
        assert_eq!(s.height, 0.0);
    }

    #[test]
    fn size_aspect_ratio() {
        let s = Size2::new(1920.0, 1080.0);
        assert!((s.aspect_ratio() - 16.0 / 9.0).abs() < 0.01);
    }

    #[test]
    fn rect_contains() {
        let r = Rect::new(10.0, 10.0, 100.0, 50.0);
        assert!(r.contains(Point2::new(50.0, 30.0)));
        assert!(!r.contains(Point2::new(5.0, 30.0)));
        assert!(r.contains(Point2::new(10.0, 10.0)));
        assert!(r.contains(Point2::new(110.0, 60.0)));
    }

    #[test]
    fn rect_intersects() {
        let a = Rect::new(0.0, 0.0, 100.0, 100.0);
        let b = Rect::new(50.0, 50.0, 100.0, 100.0);
        let c = Rect::new(200.0, 200.0, 10.0, 10.0);
        assert!(a.intersects(b));
        assert!(!a.intersects(c));
    }

    #[test]
    fn rect_union() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(5.0, 5.0, 20.0, 20.0);
        let u = a.union(b);
        assert_eq!(u.origin, Point2::ZERO);
        assert_eq!(u.size, Size2::new(25.0, 25.0));
    }

    #[test]
    fn rect_inset() {
        let r = Rect::new(0.0, 0.0, 100.0, 100.0);
        let edges = Edges::all(10.0);
        let inset = r.inset(&edges);
        assert_eq!(inset.origin, Point2::new(10.0, 10.0));
        assert_eq!(inset.size, Size2::new(80.0, 80.0));
    }

    #[test]
    fn rect_outset() {
        let r = Rect::new(10.0, 10.0, 80.0, 80.0);
        let edges = Edges::all(10.0);
        let outset = r.outset(&edges);
        assert_eq!(outset.origin, Point2::ZERO);
        assert_eq!(outset.size, Size2::new(100.0, 100.0));
    }

    #[test]
    fn rect_center() {
        let r = Rect::new(0.0, 0.0, 100.0, 50.0);
        assert_eq!(r.center(), Point2::new(50.0, 25.0));
    }

    #[test]
    fn edges_symmetric() {
        let e = Edges::symmetric(10.0_f32, 20.0);
        assert_eq!(e.top, 10.0);
        assert_eq!(e.right, 20.0);
        assert_eq!(e.bottom, 10.0);
        assert_eq!(e.left, 20.0);
    }

    #[test]
    fn edges_horizontal_vertical() {
        let e = Edges::new(10.0, 20.0, 30.0, 40.0);
        assert_eq!(e.horizontal(), 60.0);
        assert_eq!(e.vertical(), 40.0);
    }

    #[test]
    fn point_vec2_roundtrip() {
        let p = Point2::new(3.5, 2.5);
        let v: Vec2 = p.into();
        let p2: Point2 = v.into();
        assert_eq!(p, p2);
    }

    #[test]
    fn rect_inset_clamps_to_zero() {
        let r = Rect::new(0.0, 0.0, 10.0, 10.0);
        let edges = Edges::all(100.0);
        let inset = r.inset(&edges);
        assert_eq!(inset.size, Size2::ZERO);
    }

    #[test]
    fn serde_roundtrip() {
        let r = Rect::new(1.0, 2.0, 3.0, 4.0);
        let json = serde_json::to_string(&r).unwrap();
        let r2: Rect = serde_json::from_str(&json).unwrap();
        assert_eq!(r, r2);
    }
}
