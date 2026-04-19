//! Transform system for the builder's spatial model (ADR-003).
//!
//! Every node in a `BuilderDocument` carries a [`Transform2D`] that
//! describes its position, rotation, scale, and pivot relative to its
//! layout-computed origin. A propagation pass composes these into
//! [`ComputedTransform`]s that cache both local and global affine
//! matrices for O(1) coordinate conversion.

use glam::{Affine2, Vec2};
use serde::{Deserialize, Serialize};

use super::geometry::Point2;

/// Per-node transform — position, rotation, scale relative to the
/// layout-computed origin. Serialized as part of the document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transform2D {
    /// Offset from the layout-computed origin (or absolute position
    /// when the node is in `Free` layout mode).
    #[serde(default)]
    pub position: [f32; 2],

    /// Rotation in radians, applied around [`Self::pivot`].
    #[serde(default)]
    pub rotation: f32,

    /// Scale factor. `[1.0, 1.0]` = identity.
    #[serde(default = "Transform2D::default_scale")]
    pub scale: [f32; 2],

    /// The point around which rotation and scale are applied,
    /// expressed as a fraction of the node's own size.
    /// `[0.5, 0.5]` = center, `[0.0, 0.0]` = top-left.
    #[serde(default = "Transform2D::default_pivot")]
    pub pivot: [f32; 2],

    /// How this node attaches to its parent's rectangle.
    #[serde(default)]
    pub anchor: Anchor,

    /// Stacking order.
    #[serde(default)]
    pub z_index: ZIndex,

    /// Transform modifiers applied during interaction (drag, resize).
    #[serde(default)]
    pub modifiers: Vec<TransformModifier>,
}

impl Transform2D {
    fn default_scale() -> [f32; 2] {
        [1.0, 1.0]
    }
    fn default_pivot() -> [f32; 2] {
        [0.5, 0.5]
    }

    pub fn position_vec2(&self) -> Vec2 {
        Vec2::from(self.position)
    }

    pub fn scale_vec2(&self) -> Vec2 {
        Vec2::from(self.scale)
    }

    pub fn pivot_vec2(&self) -> Vec2 {
        Vec2::from(self.pivot)
    }

    /// Build the local affine matrix for a node of the given `size`.
    ///
    /// The matrix encodes: translate to pivot → scale → rotate →
    /// translate back from pivot → translate by position.
    pub fn to_local_affine(&self, size: Vec2) -> Affine2 {
        let pivot_px = size * self.pivot_vec2();
        let to_pivot = Affine2::from_translation(-pivot_px);
        let from_pivot = Affine2::from_translation(pivot_px);
        let scale = Affine2::from_scale(self.scale_vec2());
        let rotate = Affine2::from_angle(self.rotation);
        let translate = Affine2::from_translation(self.position_vec2());

        translate * from_pivot * rotate * scale * to_pivot
    }
}

impl Default for Transform2D {
    fn default() -> Self {
        Self {
            position: [0.0, 0.0],
            rotation: 0.0,
            scale: Self::default_scale(),
            pivot: Self::default_pivot(),
            anchor: Anchor::default(),
            z_index: ZIndex::default(),
            modifiers: Vec::new(),
        }
    }
}

impl PartialEq for Transform2D {
    fn eq(&self, other: &Self) -> bool {
        self.position == other.position
            && self.rotation == other.rotation
            && self.scale == other.scale
            && self.pivot == other.pivot
            && self.anchor == other.anchor
            && self.z_index == other.z_index
            && self.modifiers == other.modifiers
    }
}

/// How a node attaches to its parent's rectangle.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Anchor {
    #[default]
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    Center,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
    Stretch,
}

impl Anchor {
    /// Returns the anchor as a fractional offset within the parent rect.
    /// `Stretch` returns `(0.0, 0.0)` — stretching is handled by the
    /// layout system, not by the anchor offset.
    pub fn as_fraction(self) -> Vec2 {
        match self {
            Self::TopLeft => Vec2::new(0.0, 0.0),
            Self::TopCenter => Vec2::new(0.5, 0.0),
            Self::TopRight => Vec2::new(1.0, 0.0),
            Self::CenterLeft => Vec2::new(0.0, 0.5),
            Self::Center => Vec2::new(0.5, 0.5),
            Self::CenterRight => Vec2::new(1.0, 0.5),
            Self::BottomLeft => Vec2::new(0.0, 1.0),
            Self::BottomCenter => Vec2::new(0.5, 1.0),
            Self::BottomRight => Vec2::new(1.0, 1.0),
            Self::Stretch => Vec2::ZERO,
        }
    }
}

/// Stacking order within the parent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ZIndex {
    #[default]
    Auto,
    Explicit(i32),
}

/// Modifiers that constrain or snap transform values during
/// interaction. Evaluated by the drag/resize system, not by the
/// layout pass.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum TransformModifier {
    SnapToGrid { grid_size: f32 },
    ConstrainAspectRatio { ratio: f32 },
    ClampToBounds,
}

/// Cached result of the transform propagation pass. Holds both the
/// local (parent-relative) and global (page-relative) affine matrices.
#[derive(Debug, Clone, Copy)]
pub struct ComputedTransform {
    pub local: Affine2,
    pub global: Affine2,
}

impl ComputedTransform {
    pub const IDENTITY: Self = Self {
        local: Affine2::IDENTITY,
        global: Affine2::IDENTITY,
    };

    pub fn new(local: Affine2, parent_global: Affine2) -> Self {
        Self {
            local,
            global: parent_global * local,
        }
    }

    pub fn local_to_global(&self, point: Point2) -> Point2 {
        self.global.transform_point2(point.to_vec2()).into()
    }

    pub fn global_to_local(&self, point: Point2) -> Point2 {
        self.global
            .inverse()
            .transform_point2(point.to_vec2())
            .into()
    }
}

impl Default for ComputedTransform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// Snap a scalar value to a grid.
pub fn snap_to_grid(value: f32, grid_size: f32) -> f32 {
    if grid_size <= 0.0 {
        return value;
    }
    (value / grid_size).round() * grid_size
}

/// Snap a 2D position to a grid.
pub fn snap_point_to_grid(point: Vec2, grid_size: f32) -> Vec2 {
    Vec2::new(
        snap_to_grid(point.x, grid_size),
        snap_to_grid(point.y, grid_size),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    #[test]
    fn identity_transform() {
        let t = Transform2D::default();
        let affine = t.to_local_affine(Vec2::new(100.0, 100.0));
        let p = affine.transform_point2(Vec2::new(10.0, 20.0));
        assert!((p.x - 10.0).abs() < 1e-5);
        assert!((p.y - 20.0).abs() < 1e-5);
    }

    #[test]
    fn translation_only() {
        let t = Transform2D {
            position: [50.0, 30.0],
            ..Default::default()
        };
        let affine = t.to_local_affine(Vec2::new(100.0, 100.0));
        let p = affine.transform_point2(Vec2::ZERO);
        assert!((p.x - 50.0).abs() < 1e-5);
        assert!((p.y - 30.0).abs() < 1e-5);
    }

    #[test]
    fn rotation_around_center() {
        let t = Transform2D {
            rotation: FRAC_PI_2,
            pivot: [0.5, 0.5],
            ..Default::default()
        };
        let size = Vec2::new(100.0, 100.0);
        let affine = t.to_local_affine(size);
        // top-right corner (100, 0) rotated 90 deg around center (50,50)
        let p = affine.transform_point2(Vec2::new(100.0, 0.0));
        assert!((p.x - 100.0).abs() < 1e-3);
        assert!((p.y - 100.0).abs() < 1e-3);
    }

    #[test]
    fn scale_around_top_left() {
        let t = Transform2D {
            scale: [2.0, 2.0],
            pivot: [0.0, 0.0],
            ..Default::default()
        };
        let affine = t.to_local_affine(Vec2::new(50.0, 50.0));
        let p = affine.transform_point2(Vec2::new(10.0, 10.0));
        assert!((p.x - 20.0).abs() < 1e-5);
        assert!((p.y - 20.0).abs() < 1e-5);
    }

    #[test]
    fn computed_transform_roundtrip() {
        let parent_global = Affine2::from_translation(Vec2::new(100.0, 200.0));
        let local = Affine2::from_translation(Vec2::new(10.0, 20.0));
        let ct = ComputedTransform::new(local, parent_global);

        let p = Point2::new(5.0, 5.0);
        let global = ct.local_to_global(p);
        assert!((global.x - 115.0).abs() < 1e-5);
        assert!((global.y - 225.0).abs() < 1e-5);

        let back = ct.global_to_local(global);
        assert!((back.x - p.x).abs() < 1e-4);
        assert!((back.y - p.y).abs() < 1e-4);
    }

    #[test]
    fn snap_to_grid_works() {
        assert!((snap_to_grid(17.0, 10.0) - 20.0).abs() < 1e-5);
        assert!((snap_to_grid(14.0, 10.0) - 10.0).abs() < 1e-5);
        assert!((snap_to_grid(15.0, 10.0) - 20.0).abs() < 1e-5);
    }

    #[test]
    fn snap_to_grid_zero_size() {
        assert!((snap_to_grid(17.0, 0.0) - 17.0).abs() < 1e-5);
    }

    #[test]
    fn anchor_fractions() {
        assert_eq!(Anchor::TopLeft.as_fraction(), Vec2::ZERO);
        assert_eq!(Anchor::Center.as_fraction(), Vec2::new(0.5, 0.5));
        assert_eq!(Anchor::BottomRight.as_fraction(), Vec2::new(1.0, 1.0));
    }

    #[test]
    fn z_index_default_is_auto() {
        assert_eq!(ZIndex::default(), ZIndex::Auto);
    }

    #[test]
    fn serde_roundtrip() {
        let t = Transform2D {
            position: [10.0, 20.0],
            rotation: 1.57,
            scale: [2.0, 0.5],
            pivot: [0.5, 0.5],
            anchor: Anchor::Center,
            z_index: ZIndex::Explicit(5),
            modifiers: vec![
                TransformModifier::SnapToGrid { grid_size: 8.0 },
                TransformModifier::ConstrainAspectRatio { ratio: 1.5 },
            ],
        };
        let json = serde_json::to_string(&t).unwrap();
        let t2: Transform2D = serde_json::from_str(&json).unwrap();
        assert_eq!(t, t2);
    }

    #[test]
    fn default_transform_is_identity_equivalent() {
        let t = Transform2D::default();
        assert_eq!(t.position, [0.0, 0.0]);
        assert_eq!(t.rotation, 0.0);
        assert_eq!(t.scale, [1.0, 1.0]);
        assert_eq!(t.pivot, [0.5, 0.5]);
        assert_eq!(t.anchor, Anchor::TopLeft);
        assert_eq!(t.z_index, ZIndex::Auto);
        assert!(t.modifiers.is_empty());
    }
}
