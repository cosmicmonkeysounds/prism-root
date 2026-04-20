use crate::node::{Axis, DockNode, NodeAddress};
use crate::panel::PanelId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.width && py >= self.y && py < self.y + self.height
    }

    pub fn center(&self) -> (f32, f32) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutRect {
    pub addr: NodeAddress,
    pub rect: Rect,
    pub kind: LayoutNodeKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LayoutNodeKind {
    TabGroup { tabs: Vec<PanelId>, active: usize },
    SplitDivider { axis: Axis, ratio: f32 },
}

pub const DIVIDER_THICKNESS: f32 = 4.0;

pub fn compute_layout(node: &DockNode, bounds: Rect) -> Vec<LayoutRect> {
    let mut out = Vec::new();
    compute_inner(node, bounds, &NodeAddress::root(), &mut out);
    out
}

fn compute_inner(node: &DockNode, bounds: Rect, addr: &NodeAddress, out: &mut Vec<LayoutRect>) {
    match node {
        DockNode::TabGroup { tabs, active } => {
            out.push(LayoutRect {
                addr: addr.clone(),
                rect: bounds,
                kind: LayoutNodeKind::TabGroup {
                    tabs: tabs.clone(),
                    active: *active,
                },
            });
        }
        DockNode::Split {
            axis,
            ratio,
            first,
            second,
        } => {
            let half_div = DIVIDER_THICKNESS / 2.0;
            let (first_bounds, divider_bounds, second_bounds) = match axis {
                Axis::Horizontal => {
                    let split_x = bounds.x + ratio * bounds.width;
                    let fb = Rect::new(
                        bounds.x,
                        bounds.y,
                        (split_x - half_div - bounds.x).max(0.0),
                        bounds.height,
                    );
                    let db = Rect::new(
                        split_x - half_div,
                        bounds.y,
                        DIVIDER_THICKNESS,
                        bounds.height,
                    );
                    let sb = Rect::new(
                        split_x + half_div,
                        bounds.y,
                        (bounds.x + bounds.width - split_x - half_div).max(0.0),
                        bounds.height,
                    );
                    (fb, db, sb)
                }
                Axis::Vertical => {
                    let split_y = bounds.y + ratio * bounds.height;
                    let fb = Rect::new(
                        bounds.x,
                        bounds.y,
                        bounds.width,
                        (split_y - half_div - bounds.y).max(0.0),
                    );
                    let db = Rect::new(
                        bounds.x,
                        split_y - half_div,
                        bounds.width,
                        DIVIDER_THICKNESS,
                    );
                    let sb = Rect::new(
                        bounds.x,
                        split_y + half_div,
                        bounds.width,
                        (bounds.y + bounds.height - split_y - half_div).max(0.0),
                    );
                    (fb, db, sb)
                }
            };

            out.push(LayoutRect {
                addr: addr.clone(),
                rect: divider_bounds,
                kind: LayoutNodeKind::SplitDivider {
                    axis: *axis,
                    ratio: *ratio,
                },
            });

            compute_inner(first, first_bounds, &addr.first(), out);
            compute_inner(second, second_bounds, &addr.second(), out);
        }
    }
}

pub fn constrain_ratio(
    node: &DockNode,
    addr: &NodeAddress,
    available_width: f32,
    available_height: f32,
    min_sizes: &dyn Fn(&str) -> (f32, f32),
) -> Option<(f32, f32)> {
    let split = node.node_at(addr)?;
    let (axis, first, second) = match split {
        DockNode::Split {
            axis,
            first,
            second,
            ..
        } => (axis, first, second),
        _ => return None,
    };

    let (first_min, second_min) = match axis {
        Axis::Horizontal => {
            let f = min_width_of(first, min_sizes);
            let s = min_width_of(second, min_sizes);
            (f, s)
        }
        Axis::Vertical => {
            let f = min_height_of(first, min_sizes);
            let s = min_height_of(second, min_sizes);
            (f, s)
        }
    };

    let total = match axis {
        Axis::Horizontal => available_width - DIVIDER_THICKNESS,
        Axis::Vertical => available_height - DIVIDER_THICKNESS,
    };

    if total <= 0.0 {
        return Some((0.05, 0.95));
    }

    let min_ratio = (first_min / total).clamp(0.05, 0.95);
    let max_ratio = (1.0 - second_min / total).clamp(0.05, 0.95);
    if min_ratio > max_ratio {
        let mid = (min_ratio + max_ratio) / 2.0;
        return Some((mid, mid));
    }
    Some((min_ratio, max_ratio))
}

fn min_width_of(node: &DockNode, min_sizes: &dyn Fn(&str) -> (f32, f32)) -> f32 {
    match node {
        DockNode::TabGroup { tabs, .. } => {
            tabs.iter().map(|t| min_sizes(t).0).fold(0.0_f32, f32::max)
        }
        DockNode::Split {
            axis,
            first,
            second,
            ..
        } => match axis {
            Axis::Horizontal => {
                min_width_of(first, min_sizes) + DIVIDER_THICKNESS + min_width_of(second, min_sizes)
            }
            Axis::Vertical => min_width_of(first, min_sizes).max(min_width_of(second, min_sizes)),
        },
    }
}

fn min_height_of(node: &DockNode, min_sizes: &dyn Fn(&str) -> (f32, f32)) -> f32 {
    match node {
        DockNode::TabGroup { tabs, .. } => {
            tabs.iter().map(|t| min_sizes(t).1).fold(0.0_f32, f32::max)
        }
        DockNode::Split {
            axis,
            first,
            second,
            ..
        } => match axis {
            Axis::Horizontal => {
                min_height_of(first, min_sizes).max(min_height_of(second, min_sizes))
            }
            Axis::Vertical => {
                min_height_of(first, min_sizes)
                    + DIVIDER_THICKNESS
                    + min_height_of(second, min_sizes)
            }
        },
    }
}

pub fn find_tab_group_at(layout: &[LayoutRect], px: f32, py: f32) -> Option<&LayoutRect> {
    layout
        .iter()
        .find(|lr| matches!(lr.kind, LayoutNodeKind::TabGroup { .. }) && lr.rect.contains(px, py))
}

pub fn find_divider_at(layout: &[LayoutRect], px: f32, py: f32) -> Option<&LayoutRect> {
    layout.iter().find(|lr| {
        matches!(lr.kind, LayoutNodeKind::SplitDivider { .. }) && lr.rect.contains(px, py)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simple_hsplit() -> DockNode {
        DockNode::hsplit(
            0.3,
            DockNode::tab("explorer".into()),
            DockNode::tab("builder".into()),
        )
    }

    #[test]
    fn single_tab_fills_bounds() {
        let node = DockNode::tab("builder".into());
        let rects = compute_layout(&node, Rect::new(0.0, 0.0, 1000.0, 600.0));
        assert_eq!(rects.len(), 1);
        assert_eq!(rects[0].rect, Rect::new(0.0, 0.0, 1000.0, 600.0));
        assert!(matches!(
            &rects[0].kind,
            LayoutNodeKind::TabGroup { tabs, active: 0 } if tabs.len() == 1
        ));
    }

    #[test]
    fn hsplit_produces_three_rects() {
        let node = simple_hsplit();
        let rects = compute_layout(&node, Rect::new(0.0, 0.0, 1000.0, 600.0));
        // 1 divider + 2 tab groups
        assert_eq!(rects.len(), 3);
        let dividers: Vec<_> = rects
            .iter()
            .filter(|r| matches!(r.kind, LayoutNodeKind::SplitDivider { .. }))
            .collect();
        assert_eq!(dividers.len(), 1);
        let tabs: Vec<_> = rects
            .iter()
            .filter(|r| matches!(r.kind, LayoutNodeKind::TabGroup { .. }))
            .collect();
        assert_eq!(tabs.len(), 2);
    }

    #[test]
    fn hsplit_widths_sum_to_total() {
        let node = simple_hsplit();
        let rects = compute_layout(&node, Rect::new(0.0, 0.0, 1000.0, 600.0));
        let total: f32 = rects.iter().map(|r| r.rect.width).sum();
        assert!((total - 1000.0).abs() < 1.0);
    }

    #[test]
    fn vsplit_heights_sum_to_total() {
        let node = DockNode::vsplit(
            0.6,
            DockNode::tab("top".into()),
            DockNode::tab("bottom".into()),
        );
        let rects = compute_layout(&node, Rect::new(0.0, 0.0, 800.0, 600.0));
        let total: f32 = rects.iter().map(|r| r.rect.height).sum();
        assert!((total - 600.0).abs() < 1.0);
    }

    #[test]
    fn nested_split_layout() {
        // [Explorer | [Builder | Inspector]]
        let node = DockNode::hsplit(
            0.25,
            DockNode::tab("explorer".into()),
            DockNode::hsplit(
                0.7,
                DockNode::tab("builder".into()),
                DockNode::tab("inspector".into()),
            ),
        );
        let rects = compute_layout(&node, Rect::new(0.0, 0.0, 1200.0, 800.0));
        // 2 dividers + 3 tab groups
        assert_eq!(rects.len(), 5);
        let tab_groups: Vec<_> = rects
            .iter()
            .filter(|r| matches!(r.kind, LayoutNodeKind::TabGroup { .. }))
            .collect();
        assert_eq!(tab_groups.len(), 3);
    }

    #[test]
    fn find_tab_group_hit() {
        let node = simple_hsplit();
        let rects = compute_layout(&node, Rect::new(0.0, 0.0, 1000.0, 600.0));
        // Left side (explorer) is at x=0, width ~= 0.3 * 1000 - 2 = 298
        let hit = find_tab_group_at(&rects, 100.0, 300.0);
        assert!(hit.is_some());
        if let Some(lr) = hit {
            assert_eq!(lr.addr, NodeAddress::root().first());
        }
    }

    #[test]
    fn find_tab_group_miss_on_divider() {
        let node = simple_hsplit();
        let rects = compute_layout(&node, Rect::new(0.0, 0.0, 1000.0, 600.0));
        // Divider is around x=300
        let divider = find_divider_at(&rects, 300.0, 300.0);
        assert!(divider.is_some());
    }

    #[test]
    fn rect_contains() {
        let r = Rect::new(10.0, 20.0, 100.0, 50.0);
        assert!(r.contains(10.0, 20.0));
        assert!(r.contains(50.0, 40.0));
        assert!(!r.contains(9.0, 20.0));
        assert!(!r.contains(110.0, 20.0));
        assert!(!r.contains(50.0, 70.0));
    }

    #[test]
    fn rect_center() {
        let r = Rect::new(0.0, 0.0, 100.0, 60.0);
        assert_eq!(r.center(), (50.0, 30.0));
    }

    #[test]
    fn constrain_ratio_basic() {
        let node = DockNode::hsplit(
            0.3,
            DockNode::tab("explorer".into()),
            DockNode::tab("builder".into()),
        );
        let min_sizes = |id: &str| -> (f32, f32) {
            match id {
                "explorer" => (160.0, 100.0),
                "builder" => (200.0, 100.0),
                _ => (0.0, 0.0),
            }
        };
        let (min_r, max_r) =
            constrain_ratio(&node, &NodeAddress::root(), 1000.0, 600.0, &min_sizes).unwrap();
        // explorer needs 160 of (1000-4)=996, so min ratio ~= 0.1606
        assert!(min_r > 0.15 && min_r < 0.20);
        // builder needs 200 of 996, so max ratio ~= 1 - 0.2008 = 0.7992
        assert!(max_r > 0.75 && max_r < 0.85);
    }

    #[test]
    fn constrain_ratio_on_tab_group_returns_none() {
        let node = DockNode::tab("builder".into());
        let min_sizes = |_: &str| -> (f32, f32) { (100.0, 100.0) };
        assert!(constrain_ratio(&node, &NodeAddress::root(), 1000.0, 600.0, &min_sizes).is_none());
    }

    #[test]
    fn constrain_ratio_tight_space() {
        let node = DockNode::hsplit(0.5, DockNode::tab("a".into()), DockNode::tab("b".into()));
        let min_sizes = |_: &str| -> (f32, f32) { (200.0, 100.0) };
        // Available 300px - 4 = 296px, each needs 200 → can't fit.
        // min_ratio = 200/296 ≈ 0.676, max_ratio = 1 - 200/296 ≈ 0.324
        // min > max, so returns midpoint
        let (min_r, max_r) =
            constrain_ratio(&node, &NodeAddress::root(), 300.0, 600.0, &min_sizes).unwrap();
        assert!((min_r - max_r).abs() < f32::EPSILON);
    }

    #[test]
    fn workflow_preset_layout() {
        use crate::page::WorkflowPage;
        let page = WorkflowPage::fusion();
        let rects = compute_layout(&page.dock.root, Rect::new(0.0, 0.0, 1920.0, 1080.0));
        let tab_groups: Vec<_> = rects
            .iter()
            .filter(|r| matches!(r.kind, LayoutNodeKind::TabGroup { .. }))
            .collect();
        assert_eq!(tab_groups.len(), 5);
        for tg in &tab_groups {
            assert!(tg.rect.width > 0.0);
            assert!(tg.rect.height > 0.0);
        }
    }
}
