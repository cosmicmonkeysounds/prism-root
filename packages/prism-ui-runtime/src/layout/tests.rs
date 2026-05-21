use super::*;

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color { r, g, b, a: 255 }
}

fn five_element_scene() -> Node {
    Node::Container {
        id: "root".into(),
        props: ContainerProps {
            direction: Direction::Column,
            gap: 8.0,
            padding: Padding::all(16.0),
            width: Sizing::Grow,
            height: Sizing::Grow,
            background: Some(rgb(240, 240, 240)),
            ..Default::default()
        },
        children: vec![
            Node::Text {
                id: "title".into(),
                content: "Prism".into(),
                props: TextProps {
                    font_size: 24.0,
                    color: rgb(20, 20, 20),
                    ..Default::default()
                },
            },
            Node::Container {
                id: "row".into(),
                props: ContainerProps {
                    direction: Direction::Row,
                    gap: 8.0,
                    width: Sizing::Grow,
                    height: Sizing::Fixed(40.0),
                    background: Some(rgb(255, 255, 255)),
                    ..Default::default()
                },
                children: vec![
                    Node::Text {
                        id: "a".into(),
                        content: "A".into(),
                        props: TextProps::default(),
                    },
                    Node::Spacer {
                        id: "gap".into(),
                        width: 16.0,
                        height: 0.0,
                    },
                    Node::Text {
                        id: "b".into(),
                        content: "B".into(),
                        props: TextProps::default(),
                    },
                ],
            },
        ],
    }
}

#[test]
fn five_element_scene_lays_out() {
    let mut surface = Surface::new(
        five_element_scene(),
        Viewport {
            width: 800.0,
            height: 600.0,
        },
    );
    let commands = surface.commands();
    // Root background + row background + 3 text leaves = 5 commands.
    assert_eq!(commands.len(), 5);
}

#[test]
fn surface_is_retained_layout_only_runs_when_dirty() {
    let mut surface = Surface::new(
        five_element_scene(),
        Viewport {
            width: 800.0,
            height: 600.0,
        },
    );
    assert!(surface.is_dirty());
    let _ = surface.commands();
    assert!(!surface.is_dirty(), "after compute, surface is clean");
    let _ = surface.commands();
    assert!(!surface.is_dirty(), "second call is a no-op");

    surface.set_viewport(Viewport {
        width: 1024.0,
        height: 768.0,
    });
    assert!(surface.is_dirty(), "resize invalidates");
    let _ = surface.commands();

    surface.set_viewport(Viewport {
        width: 1024.0,
        height: 768.0,
    });
    assert!(
        !surface.is_dirty(),
        "setting the same viewport must not invalidate"
    );

    surface.invalidate();
    assert!(surface.is_dirty(), "explicit invalidate works");
}

#[test]
fn empty_container_emits_nothing() {
    let mut surface = Surface::new(
        Node::Container {
            id: "empty".into(),
            props: ContainerProps::default(),
            children: vec![],
        },
        Viewport {
            width: 100.0,
            height: 100.0,
        },
    );
    assert!(surface.commands().is_empty());
}

#[test]
fn row_layout_distributes_along_x() {
    let tree = Node::Container {
        id: "row".into(),
        props: ContainerProps {
            direction: Direction::Row,
            width: Sizing::Grow,
            height: Sizing::Grow,
            background: Some(rgb(0, 0, 0)),
            ..Default::default()
        },
        children: vec![
            Node::Container {
                id: "a".into(),
                props: ContainerProps {
                    width: Sizing::Fixed(50.0),
                    height: Sizing::Grow,
                    background: Some(rgb(255, 0, 0)),
                    ..Default::default()
                },
                children: vec![],
            },
            Node::Container {
                id: "b".into(),
                props: ContainerProps {
                    width: Sizing::Fixed(70.0),
                    height: Sizing::Grow,
                    background: Some(rgb(0, 0, 255)),
                    ..Default::default()
                },
                children: vec![],
            },
        ],
    };
    let mut surface = Surface::new(
        tree,
        Viewport {
            width: 200.0,
            height: 100.0,
        },
    );
    let commands = surface.commands().to_vec();
    let rects: Vec<_> = commands
        .iter()
        .filter_map(|c| match c {
            RenderCommand::Rectangle { bounds, .. } => Some(*bounds),
            _ => None,
        })
        .collect();
    assert_eq!(rects.len(), 3);
    assert_eq!(rects[1].x, 0.0);
    assert_eq!(rects[1].width, 50.0);
    assert_eq!(rects[2].x, 50.0);
    assert_eq!(rects[2].width, 70.0);
}

fn hover_button_tree() -> Node {
    Node::Container {
        id: "btn".into(),
        props: ContainerProps {
            width: Sizing::Fixed(28.0),
            height: Sizing::Fixed(28.0),
            background: Some(rgb(255, 255, 255)),
            hover: Some(StateOverrides {
                background: Some(rgb(0, 100, 200)),
                ..Default::default()
            }),
            ..Default::default()
        },
        children: vec![],
    }
}

fn rectangle_color(commands: &[RenderCommand]) -> Color {
    commands
        .iter()
        .find_map(|c| match c {
            RenderCommand::Rectangle { color, .. } => Some(*color),
            _ => None,
        })
        .expect("at least one rectangle")
}

/// Wave 14.6 — container `opacity` cascades into every emitted
/// colour. A 0.5 opacity on the parent halves both its own
/// background alpha and every text/border/rectangle alpha in
/// its subtree.
#[test]
fn opacity_cascades_into_child_colours() {
    let tree = Node::Container {
        id: "root".into(),
        props: ContainerProps {
            direction: Direction::Column,
            width: Sizing::Grow,
            height: Sizing::Grow,
            background: Some(rgb(255, 255, 255)),
            opacity: Some(0.5),
            ..Default::default()
        },
        children: vec![Node::Text {
            id: "label".into(),
            content: "x".into(),
            props: TextProps {
                font_size: 16.0,
                color: rgb(20, 20, 20),
                ..Default::default()
            },
        }],
    };
    let cmds = compute(
        &tree,
        Viewport {
            width: 100.0,
            height: 100.0,
        },
    );
    // Background rectangle: alpha was 0xff, opacity 0.5 → ≈ 0x7f.
    let rect_alpha = cmds
        .iter()
        .find_map(|c| match c {
            RenderCommand::Rectangle { color, .. } => Some(color.a),
            _ => None,
        })
        .expect("rectangle emitted");
    assert!(
        (rect_alpha as i32 - 0x80).abs() <= 1,
        "expected ~0x80, got {rect_alpha:#x}"
    );
    // Text colour inherits the cascaded opacity too.
    let text_alpha = cmds
        .iter()
        .find_map(|c| match c {
            RenderCommand::Text { color, .. } => Some(color.a),
            _ => None,
        })
        .expect("text emitted");
    assert!(
        (text_alpha as i32 - 0x80).abs() <= 1,
        "text expected ~0x80, got {text_alpha:#x}"
    );
}

/// Wave 14.6 — nested containers compose their opacity
/// multiplicatively (0.5 × 0.5 = 0.25), matching CSS.
#[test]
fn nested_opacity_multiplies_through_subtree() {
    let tree = Node::Container {
        id: "outer".into(),
        props: ContainerProps {
            direction: Direction::Column,
            width: Sizing::Grow,
            height: Sizing::Grow,
            opacity: Some(0.5),
            ..Default::default()
        },
        children: vec![Node::Container {
            id: "inner".into(),
            props: ContainerProps {
                width: Sizing::Grow,
                height: Sizing::Grow,
                background: Some(rgb(255, 0, 0)),
                opacity: Some(0.5),
                ..Default::default()
            },
            children: vec![],
        }],
    };
    let cmds = compute(
        &tree,
        Viewport {
            width: 100.0,
            height: 100.0,
        },
    );
    let inner_alpha = cmds
        .iter()
        .find_map(|c| match c {
            RenderCommand::Rectangle { color, .. } => Some(color.a),
            _ => None,
        })
        .expect("inner rect");
    // 0xff * 0.5 * 0.5 = ~0x40
    assert!(
        (inner_alpha as i32 - 0x40).abs() <= 2,
        "expected ~0x40, got {inner_alpha:#x}"
    );
}

#[test]
fn hover_overrides_swap_in_when_id_matches() {
    let resting = compute_with_hover(
        &hover_button_tree(),
        Viewport {
            width: 100.0,
            height: 100.0,
        },
        None,
    );
    assert_eq!(rectangle_color(&resting), rgb(255, 255, 255));

    let hovered = compute_with_hover(
        &hover_button_tree(),
        Viewport {
            width: 100.0,
            height: 100.0,
        },
        Some("btn"),
    );
    assert_eq!(rectangle_color(&hovered), rgb(0, 100, 200));
}

#[test]
fn hover_overrides_ignored_when_id_does_not_match() {
    let cmds = compute_with_hover(
        &hover_button_tree(),
        Viewport {
            width: 100.0,
            height: 100.0,
        },
        Some("some-other-node"),
    );
    assert_eq!(rectangle_color(&cmds), rgb(255, 255, 255));
}

fn state_button_tree() -> Node {
    // Same shape as `hover_button_tree` but with all four state buckets
    // declared so the precedence cascade is observable. Resting paint
    // is white; each state swaps to a distinct colour so a paint test
    // can read the active state directly.
    Node::Container {
        id: "btn".into(),
        props: ContainerProps {
            width: Sizing::Fixed(28.0),
            height: Sizing::Fixed(28.0),
            background: Some(rgb(255, 255, 255)),
            hover: Some(StateOverrides {
                background: Some(rgb(10, 10, 10)),
                ..Default::default()
            }),
            focused: Some(StateOverrides {
                background: Some(rgb(20, 20, 20)),
                ..Default::default()
            }),
            pressed: Some(StateOverrides {
                background: Some(rgb(30, 30, 30)),
                ..Default::default()
            }),
            disabled: Some(StateOverrides {
                background: Some(rgb(40, 40, 40)),
                ..Default::default()
            }),
            ..Default::default()
        },
        children: vec![],
    }
}

/// §7.7 Phase 1 — the precedence cascade. Multiple states active
/// together resolve in low → high order: `hover < focused < pressed
/// < disabled`. A button under the cursor with focus AND a press
/// MUST paint pressed, not hovered.
#[test]
fn state_overrides_precedence_pressed_beats_focused_beats_hovered() {
    let viewport = Viewport {
        width: 100.0,
        height: 100.0,
    };
    let tree = state_button_tree();
    // Hover only → 10,10,10
    let cmds = compute_full(
        &tree,
        &[],
        viewport,
        StateIds {
            hover: Some("btn"),
            ..Default::default()
        },
    );
    assert_eq!(rectangle_color(&cmds), rgb(10, 10, 10), "hover only");

    // Hover + focused → focused wins (20,20,20)
    let cmds = compute_full(
        &tree,
        &[],
        viewport,
        StateIds {
            hover: Some("btn"),
            focused: Some("btn"),
            ..Default::default()
        },
    );
    assert_eq!(
        rectangle_color(&cmds),
        rgb(20, 20, 20),
        "focused beats hover"
    );

    // Hover + focused + pressed → pressed wins (30,30,30)
    let cmds = compute_full(
        &tree,
        &[],
        viewport,
        StateIds {
            hover: Some("btn"),
            focused: Some("btn"),
            pressed: Some("btn"),
        },
    );
    assert_eq!(
        rectangle_color(&cmds),
        rgb(30, 30, 30),
        "pressed beats focused"
    );
}

/// §7.7 Phase 1 — `disabled` is declarative (`props.disabled_flag`),
/// not event-driven, and outranks every event-driven state in the
/// cascade. A button declared disabled MUST paint disabled even
/// under hover/pressed/focused.
#[test]
fn state_overrides_disabled_outranks_event_driven_states() {
    let viewport = Viewport {
        width: 100.0,
        height: 100.0,
    };
    let mut tree = state_button_tree();
    if let Node::Container { props, .. } = &mut tree {
        props.disabled_flag = true;
    }

    // Even with pressed + focused + hover all active, disabled wins.
    let cmds = compute_full(
        &tree,
        &[],
        viewport,
        StateIds {
            hover: Some("btn"),
            focused: Some("btn"),
            pressed: Some("btn"),
        },
    );
    assert_eq!(rectangle_color(&cmds), rgb(40, 40, 40), "disabled wins");
}

/// §7.7 Phase 1 — state buckets compose sparsely. A `pressed` bucket
/// that only sets `opacity` (leaving `background` `None`) does NOT
/// erase the hovered background; the field-by-field overrides
/// stack. This is the canonical "pressed dims a hovered button"
/// shape.
#[test]
fn state_overrides_sparse_fields_compose_across_buckets() {
    let viewport = Viewport {
        width: 100.0,
        height: 100.0,
    };
    let tree = Node::Container {
        id: "btn".into(),
        props: ContainerProps {
            width: Sizing::Fixed(20.0),
            height: Sizing::Fixed(20.0),
            background: Some(rgb(255, 255, 255)),
            hover: Some(StateOverrides {
                background: Some(rgb(50, 60, 70)),
                ..Default::default()
            }),
            pressed: Some(StateOverrides {
                opacity: Some(0.5),
                ..Default::default()
            }),
            ..Default::default()
        },
        children: vec![],
    };

    let cmds = compute_full(
        &tree,
        &[],
        viewport,
        StateIds {
            hover: Some("btn"),
            pressed: Some("btn"),
            ..Default::default()
        },
    );
    // Background stays the hovered colour (only pressed.opacity was
    // overridden); paint applies the pressed opacity multiplier so
    // we see a 50%-alpha-scaled (50, 60, 70).
    let color = rectangle_color(&cmds);
    // Opacity 0.5 scales alpha to ~128. Hue stays the hovered RGB.
    assert_eq!((color.r, color.g, color.b), (50, 60, 70));
    assert!(
        color.a >= 120 && color.a <= 135,
        "expected pressed opacity 0.5 to halve alpha, got {}",
        color.a
    );
}

#[test]
fn surface_set_hovered_dirty_only_when_paint_actually_changes() {
    let mut surface = Surface::new(
        hover_button_tree(),
        Viewport {
            width: 100.0,
            height: 100.0,
        },
    );
    let _ = surface.commands();
    assert!(!surface.is_dirty());

    // Entering a hover-affecting node → dirty (need to paint hover state).
    surface.set_hovered(Some("btn".into()));
    assert!(surface.is_dirty());
    let _ = surface.commands();

    // Leaving a hover-affecting node → dirty (need to paint resting state).
    surface.set_hovered(Some("nonexistent".into()));
    assert!(surface.is_dirty());
    let _ = surface.commands();

    // Drift between two non-affecting nodes → no recompute.
    surface.set_hovered(Some("also-nonexistent".into()));
    assert!(
        !surface.is_dirty(),
        "moves between non-affecting nodes shouldn't recompute"
    );

    // Idempotent: same id doesn't dirty.
    surface.set_hovered(Some("also-nonexistent".into()));
    assert!(!surface.is_dirty());
}

#[test]
fn surface_hover_swap_round_trip() {
    let mut surface = Surface::new(
        hover_button_tree(),
        Viewport {
            width: 100.0,
            height: 100.0,
        },
    );
    assert_eq!(rectangle_color(surface.commands()), rgb(255, 255, 255));
    surface.set_hovered(Some("btn".into()));
    assert_eq!(rectangle_color(surface.commands()), rgb(0, 100, 200));
    surface.set_hovered(None);
    assert_eq!(rectangle_color(surface.commands()), rgb(255, 255, 255));
}

#[test]
fn semantic_button_constructor_includes_type_attr() {
    let s = Semantic::button();
    assert_eq!(s.tag.as_deref(), Some("button"));
    assert!(s.attrs.iter().any(|(k, v)| k == "type" && v == "button"));
}

#[test]
fn semantic_with_attr_if_branches_on_cond() {
    let on = Semantic::button().with_attr_if(true, "aria-pressed", "true");
    assert!(on
        .attrs
        .iter()
        .any(|(k, v)| k == "aria-pressed" && v == "true"));

    let off = Semantic::button().with_attr_if(false, "aria-pressed", "true");
    assert!(off.attrs.iter().all(|(k, _)| k != "aria-pressed"));
}

fn fixed_box(id: &str, w: f32, h: f32, color: Color) -> Node {
    Node::Container {
        id: id.into(),
        props: ContainerProps {
            width: Sizing::Fixed(w),
            height: Sizing::Fixed(h),
            background: Some(color),
            ..Default::default()
        },
        children: vec![],
    }
}

fn rectangle_at(commands: &[RenderCommand], color: Color) -> Rect {
    commands
        .iter()
        .find_map(|c| match c {
            RenderCommand::Rectangle {
                bounds, color: c2, ..
            } if *c2 == color => Some(*bounds),
            _ => None,
        })
        .expect("expected rectangle of given color")
}

#[test]
fn overlay_paints_after_main_tree_at_resolved_corner() {
    let main = fixed_box("main", 100.0, 100.0, rgb(10, 10, 10));
    let toast = fixed_box("toast", 200.0, 60.0, rgb(20, 20, 20));
    let overlay = Overlay::new(
        "toast",
        OverlayAnchor::Corner {
            corner: Corner::BottomRight,
            inset: Inset::all(16.0),
        },
        toast,
    );
    let cmds = compute_full(
        &main,
        std::slice::from_ref(&overlay),
        Viewport {
            width: 800.0,
            height: 600.0,
        },
        StateIds::default(),
    );
    // Main tree paints first, overlay second — z-order via order.
    let main_idx = cmds
        .iter()
        .position(
            |c| matches!(c, RenderCommand::Rectangle { color, .. } if *color == rgb(10,10,10)),
        )
        .unwrap();
    let toast_idx = cmds
        .iter()
        .position(
            |c| matches!(c, RenderCommand::Rectangle { color, .. } if *color == rgb(20,20,20)),
        )
        .unwrap();
    assert!(toast_idx > main_idx, "overlay must paint after main");

    let bounds = rectangle_at(&cmds, rgb(20, 20, 20));
    // BottomRight at (800-200-16, 600-60-16) = (584, 524)
    assert_eq!(bounds.x, 584.0);
    assert_eq!(bounds.y, 524.0);
}

#[test]
fn overlay_anchor_center_resolves_to_viewport_centre_with_offset() {
    let palette = fixed_box("p", 400.0, 100.0, rgb(50, 60, 70));
    let overlay = Overlay::new(
        "palette",
        OverlayAnchor::Center { offset_y: -100.0 },
        palette,
    );
    let cmds = compute_full(
        &fixed_box("root", 1.0, 1.0, rgb(0, 0, 0)),
        std::slice::from_ref(&overlay),
        Viewport {
            width: 800.0,
            height: 600.0,
        },
        StateIds::default(),
    );
    let bounds = rectangle_at(&cmds, rgb(50, 60, 70));
    // x = (800-400)/2 = 200, y = (600-100)/2 - 100 = 250 - 100 = 150
    assert_eq!(bounds.x, 200.0);
    assert_eq!(bounds.y, 150.0);
}

#[test]
fn overlay_anchor_point_translates_verbatim() {
    let tip = fixed_box("tip", 80.0, 24.0, rgb(99, 99, 99));
    let overlay = Overlay::new("tip", OverlayAnchor::Point { x: 312.5, y: 48.0 }, tip);
    let cmds = compute_full(
        &fixed_box("root", 1.0, 1.0, rgb(0, 0, 0)),
        std::slice::from_ref(&overlay),
        Viewport {
            width: 1000.0,
            height: 600.0,
        },
        StateIds::default(),
    );
    let bounds = rectangle_at(&cmds, rgb(99, 99, 99));
    assert_eq!(bounds.x, 312.5);
    assert_eq!(bounds.y, 48.0);
}

#[test]
fn surface_overlay_lifecycle_marks_dirty_only_when_stack_changes() {
    let mut surface = Surface::new(
        fixed_box("root", 1.0, 1.0, rgb(0, 0, 0)),
        Viewport {
            width: 400.0,
            height: 300.0,
        },
    );
    let _ = surface.commands();
    assert!(!surface.is_dirty());

    // Push → dirty.
    surface.push_overlay(Overlay::new(
        "t1",
        OverlayAnchor::Corner {
            corner: Corner::BottomRight,
            inset: Inset::all(8.0),
        },
        fixed_box("t1", 100.0, 40.0, rgb(11, 22, 33)),
    ));
    assert!(surface.is_dirty());
    let cmds = surface.commands();
    assert!(cmds.iter().any(|c| matches!(
        c,
        RenderCommand::Rectangle { color, .. } if *color == rgb(11, 22, 33)
    )));
    assert!(!surface.is_dirty());

    // Push same id → replaces, still dirty.
    surface.push_overlay(Overlay::new(
        "t1",
        OverlayAnchor::Corner {
            corner: Corner::BottomRight,
            inset: Inset::all(8.0),
        },
        fixed_box("t1", 100.0, 40.0, rgb(44, 55, 66)),
    ));
    assert!(surface.is_dirty());
    assert_eq!(surface.overlays().len(), 1, "same id replaces in place");
    let cmds = surface.commands();
    assert!(cmds.iter().any(|c| matches!(
        c,
        RenderCommand::Rectangle { color, .. } if *color == rgb(44, 55, 66)
    )));

    // Remove unknown → no-op.
    let removed = surface.remove_overlay("does-not-exist");
    assert!(!removed);
    assert!(!surface.is_dirty());

    // Remove existing → dirty.
    let removed = surface.remove_overlay("t1");
    assert!(removed);
    assert!(surface.is_dirty());

    // Clear empty → no-op.
    let _ = surface.commands();
    assert!(!surface.is_dirty());
    surface.clear_overlays();
    assert!(!surface.is_dirty(), "clearing empty stack is a no-op");
}

#[test]
fn surface_overlay_hover_swap_dirties_through_overlay_subtree() {
    // Overlay subtree contains a hover-affecting node.
    let inner = Node::Container {
        id: "btn".into(),
        props: ContainerProps {
            width: Sizing::Fixed(40.0),
            height: Sizing::Fixed(20.0),
            background: Some(rgb(255, 255, 255)),
            hover: Some(StateOverrides {
                background: Some(rgb(1, 2, 3)),
                ..Default::default()
            }),
            ..Default::default()
        },
        children: vec![],
    };
    let mut surface = Surface::new(
        fixed_box("root", 1.0, 1.0, rgb(0, 0, 0)),
        Viewport {
            width: 400.0,
            height: 300.0,
        },
    );
    surface.push_overlay(Overlay::new(
        "popup",
        OverlayAnchor::Center { offset_y: 0.0 },
        inner,
    ));
    let _ = surface.commands();
    assert!(!surface.is_dirty());

    // Hovering the overlay's interior must dirty — the lookup
    // walks both main + overlay subtrees.
    surface.set_hovered(Some("btn".into()));
    assert!(surface.is_dirty());
    let cmds = surface.commands().to_vec();
    assert!(cmds.iter().any(|c| matches!(
        c,
        RenderCommand::Rectangle { color, .. } if *color == rgb(1, 2, 3)
    )));
}

#[test]
fn semantic_with_aria_label_opt_handles_some_and_none() {
    let labelled = Semantic::button().with_aria_label_opt(Some("Close"));
    assert_eq!(labelled.aria_label.as_deref(), Some("Close"));

    let unlabelled: Semantic = Semantic::button().with_aria_label_opt(None::<&str>);
    assert!(unlabelled.aria_label.is_none());
}

// ── §43 hit-test surface ──────────────────────────────────────────

fn id_container(id: &str, w: f32, h: f32, attrs: Vec<(&str, &str)>, children: Vec<Node>) -> Node {
    let mut semantic = Semantic::tag("div");
    for (k, v) in attrs {
        semantic = semantic.with_attr(k, v);
    }
    Node::Container {
        id: id.into(),
        props: ContainerProps {
            width: Sizing::Fixed(w),
            height: Sizing::Fixed(h),
            semantic,
            ..Default::default()
        },
        children,
    }
}

#[test]
fn hit_test_returns_topmost_container_for_point_inside() {
    // Row of two 100x40 boxes, each with a distinct id. Hits at (10,10)
    // land in the first; hits at (160,20) land in the second.
    let tree = Node::Container {
        id: "root".into(),
        props: ContainerProps {
            direction: Direction::Row,
            gap: 0.0,
            width: Sizing::Fixed(300.0),
            height: Sizing::Fixed(40.0),
            semantic: Semantic::tag("div").with_attr("data-role", "row"),
            ..Default::default()
        },
        children: vec![
            id_container("a", 100.0, 40.0, vec![("data-role", "alpha")], vec![]),
            id_container("b", 100.0, 40.0, vec![("data-role", "beta")], vec![]),
        ],
    };
    let mut surface = Surface::new(
        tree,
        Viewport {
            width: 400.0,
            height: 100.0,
        },
    );
    let hit = surface.hit_test_at(10.0, 10.0).expect("hit");
    assert_eq!(hit.id, "a");
    assert!(hit
        .attrs
        .iter()
        .any(|(k, v)| k == "data-role" && v == "alpha"));
    let hit = surface.hit_test_at(160.0, 20.0).expect("hit");
    assert_eq!(hit.id, "b");
    assert!(hit
        .attrs
        .iter()
        .any(|(k, v)| k == "data-role" && v == "beta"));
}

#[test]
fn hit_test_picks_deepest_container() {
    // Outer 100x100 holding an inner 40x40 — a hit inside the inner
    // returns the inner (deepest = topmost), not the outer.
    let tree = id_container(
        "outer",
        100.0,
        100.0,
        vec![("data-role", "outer")],
        vec![id_container(
            "inner",
            40.0,
            40.0,
            vec![("data-role", "inner")],
            vec![],
        )],
    );
    let mut surface = Surface::new(
        tree,
        Viewport {
            width: 200.0,
            height: 200.0,
        },
    );
    let hit = surface.hit_test_at(10.0, 10.0).expect("hit inside inner");
    assert_eq!(hit.id, "inner");
}

#[test]
fn hit_test_returns_none_outside_tree() {
    let tree = id_container("a", 50.0, 50.0, vec![], vec![]);
    let mut surface = Surface::new(
        tree,
        Viewport {
            width: 200.0,
            height: 200.0,
        },
    );
    assert!(surface.hit_test_at(80.0, 80.0).is_none());
}

#[test]
fn hit_test_skips_anonymous_wrapper_containers() {
    // The outer wrapper has no id (it's anonymous like a synthesised
    // surface wrap); only the id'd child shows up in the hit cache.
    let tree = Node::Container {
        id: String::new(),
        props: ContainerProps {
            width: Sizing::Fixed(100.0),
            height: Sizing::Fixed(100.0),
            ..Default::default()
        },
        children: vec![id_container("named", 100.0, 100.0, vec![], vec![])],
    };
    let mut surface = Surface::new(
        tree,
        Viewport {
            width: 200.0,
            height: 200.0,
        },
    );
    let hit = surface.hit_test_at(10.0, 10.0).expect("hit");
    assert_eq!(hit.id, "named");
    assert_eq!(surface.hit_rects().len(), 1);
}
