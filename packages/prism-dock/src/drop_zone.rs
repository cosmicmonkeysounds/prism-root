use crate::layout::{LayoutNodeKind, LayoutRect, Rect};
use crate::node::{Axis, MoveTarget, NodeAddress, SplitPosition};

#[derive(Debug, Clone, PartialEq)]
pub enum DropZone {
    TabInsert { addr: NodeAddress, rect: Rect },
    Left { addr: NodeAddress, rect: Rect },
    Right { addr: NodeAddress, rect: Rect },
    Top { addr: NodeAddress, rect: Rect },
    Bottom { addr: NodeAddress, rect: Rect },
}

impl DropZone {
    pub fn addr(&self) -> &NodeAddress {
        match self {
            Self::TabInsert { addr, .. }
            | Self::Left { addr, .. }
            | Self::Right { addr, .. }
            | Self::Top { addr, .. }
            | Self::Bottom { addr, .. } => addr,
        }
    }

    pub fn rect(&self) -> &Rect {
        match self {
            Self::TabInsert { rect, .. }
            | Self::Left { rect, .. }
            | Self::Right { rect, .. }
            | Self::Top { rect, .. }
            | Self::Bottom { rect, .. } => rect,
        }
    }

    pub fn to_move_target(&self) -> MoveTarget {
        match self {
            Self::TabInsert { addr, .. } => MoveTarget::TabGroup(addr.clone()),
            Self::Left { addr, .. } => MoveTarget::SplitEdge {
                addr: addr.clone(),
                axis: Axis::Horizontal,
                position: SplitPosition::Before,
            },
            Self::Right { addr, .. } => MoveTarget::SplitEdge {
                addr: addr.clone(),
                axis: Axis::Horizontal,
                position: SplitPosition::After,
            },
            Self::Top { addr, .. } => MoveTarget::SplitEdge {
                addr: addr.clone(),
                axis: Axis::Vertical,
                position: SplitPosition::Before,
            },
            Self::Bottom { addr, .. } => MoveTarget::SplitEdge {
                addr: addr.clone(),
                axis: Axis::Vertical,
                position: SplitPosition::After,
            },
        }
    }
}

const EDGE_FRACTION: f32 = 0.25;

pub fn compute_drop_zones(layout: &[LayoutRect]) -> Vec<DropZone> {
    let mut zones = Vec::new();
    for lr in layout {
        if let LayoutNodeKind::TabGroup { .. } = &lr.kind {
            let r = &lr.rect;
            let ew = r.width * EDGE_FRACTION;
            let eh = r.height * EDGE_FRACTION;

            zones.push(DropZone::Left {
                addr: lr.addr.clone(),
                rect: Rect::new(r.x, r.y, ew, r.height),
            });
            zones.push(DropZone::Right {
                addr: lr.addr.clone(),
                rect: Rect::new(r.x + r.width - ew, r.y, ew, r.height),
            });
            zones.push(DropZone::Top {
                addr: lr.addr.clone(),
                rect: Rect::new(r.x + ew, r.y, r.width - 2.0 * ew, eh),
            });
            zones.push(DropZone::Bottom {
                addr: lr.addr.clone(),
                rect: Rect::new(r.x + ew, r.y + r.height - eh, r.width - 2.0 * ew, eh),
            });
            zones.push(DropZone::TabInsert {
                addr: lr.addr.clone(),
                rect: Rect::new(r.x + ew, r.y + eh, r.width - 2.0 * ew, r.height - 2.0 * eh),
            });
        }
    }
    zones
}

pub fn hit_test_drop_zone(zones: &[DropZone], px: f32, py: f32) -> Option<&DropZone> {
    zones.iter().find(|z| z.rect().contains(px, py))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::compute_layout;
    use crate::node::DockNode;

    #[test]
    fn single_tab_has_five_zones() {
        let node = DockNode::tab("builder".into());
        let layout = compute_layout(&node, Rect::new(0.0, 0.0, 1000.0, 600.0));
        let zones = compute_drop_zones(&layout);
        assert_eq!(zones.len(), 5);
    }

    #[test]
    fn hsplit_has_ten_zones() {
        let node = DockNode::hsplit(0.3, DockNode::tab("a".into()), DockNode::tab("b".into()));
        let layout = compute_layout(&node, Rect::new(0.0, 0.0, 1000.0, 600.0));
        let zones = compute_drop_zones(&layout);
        assert_eq!(zones.len(), 10);
    }

    #[test]
    fn center_hit_is_tab_insert() {
        let node = DockNode::tab("builder".into());
        let layout = compute_layout(&node, Rect::new(0.0, 0.0, 1000.0, 600.0));
        let zones = compute_drop_zones(&layout);
        let hit = hit_test_drop_zone(&zones, 500.0, 300.0).unwrap();
        assert!(matches!(hit, DropZone::TabInsert { .. }));
    }

    #[test]
    fn left_edge_hit() {
        let node = DockNode::tab("builder".into());
        let layout = compute_layout(&node, Rect::new(0.0, 0.0, 1000.0, 600.0));
        let zones = compute_drop_zones(&layout);
        let hit = hit_test_drop_zone(&zones, 10.0, 300.0).unwrap();
        assert!(matches!(hit, DropZone::Left { .. }));
    }

    #[test]
    fn right_edge_hit() {
        let node = DockNode::tab("builder".into());
        let layout = compute_layout(&node, Rect::new(0.0, 0.0, 1000.0, 600.0));
        let zones = compute_drop_zones(&layout);
        let hit = hit_test_drop_zone(&zones, 990.0, 300.0).unwrap();
        assert!(matches!(hit, DropZone::Right { .. }));
    }

    #[test]
    fn top_edge_hit() {
        let node = DockNode::tab("builder".into());
        let layout = compute_layout(&node, Rect::new(0.0, 0.0, 1000.0, 600.0));
        let zones = compute_drop_zones(&layout);
        let hit = hit_test_drop_zone(&zones, 500.0, 10.0).unwrap();
        assert!(matches!(hit, DropZone::Top { .. }));
    }

    #[test]
    fn bottom_edge_hit() {
        let node = DockNode::tab("builder".into());
        let layout = compute_layout(&node, Rect::new(0.0, 0.0, 1000.0, 600.0));
        let zones = compute_drop_zones(&layout);
        let hit = hit_test_drop_zone(&zones, 500.0, 590.0).unwrap();
        assert!(matches!(hit, DropZone::Bottom { .. }));
    }

    #[test]
    fn drop_zone_to_move_target() {
        let zone = DropZone::Left {
            addr: NodeAddress::root(),
            rect: Rect::new(0.0, 0.0, 100.0, 600.0),
        };
        let target = zone.to_move_target();
        assert_eq!(
            target,
            MoveTarget::SplitEdge {
                addr: NodeAddress::root(),
                axis: Axis::Horizontal,
                position: SplitPosition::Before,
            }
        );
    }

    #[test]
    fn tab_insert_to_move_target() {
        let zone = DropZone::TabInsert {
            addr: NodeAddress::root().first(),
            rect: Rect::new(100.0, 100.0, 400.0, 400.0),
        };
        let target = zone.to_move_target();
        assert_eq!(target, MoveTarget::TabGroup(NodeAddress::root().first()));
    }

    #[test]
    fn no_zones_outside_bounds() {
        let node = DockNode::tab("builder".into());
        let layout = compute_layout(&node, Rect::new(0.0, 0.0, 1000.0, 600.0));
        let zones = compute_drop_zones(&layout);
        assert!(hit_test_drop_zone(&zones, -10.0, 300.0).is_none());
        assert!(hit_test_drop_zone(&zones, 1010.0, 300.0).is_none());
    }
}
