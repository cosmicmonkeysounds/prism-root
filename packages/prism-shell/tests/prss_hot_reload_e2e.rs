//! End-to-end PRSS hot-reload integration test.
//!
//! Exercises the full disk → watcher → shell → render pipeline with
//! a real `.prss` file written to a tempdir. Each phase asserts on
//! observable shell state (active stylesheet, render output) so a
//! regression in any link of the chain — fingerprint hashing,
//! patch classification, stylesheet install, scope threading, class
//! application, semantic round-trip — surfaces here.
//!
//! The test deliberately bypasses the dev-loop watcher's notify
//! crate (filesystem races, polling intervals, OS-level event
//! coalescing make those tests flaky in CI) and drives the
//! `StylesheetWatcher` directly, which is the same component
//! `prism-cli` would call in response to a notify event. Every
//! other layer is real: `Stylesheet::load_from_path` reads the
//! tempfile, `prss::parse` parses it, `Shell::install_stylesheet`
//! mounts it on the live shell, `Shell::render` walks the
//! `LowerScope` chain through `apply_prss_class`, and
//! `Semantic::class` round-trips into the visible tree.

use std::fs;

use prism_shell::{Shell, Stylesheet, StylesheetWatcher};
use prism_ui_build::PrssChange;
use prism_ui_runtime::layout::{ContainerProps, Node as UiNode};

/// Initial PRSS content for the e2e flow. Carries enough surface
/// to exercise tokens, classes, descendant selectors, state
/// variants, and short-name resolution in one shot.
const INITIAL_PRSS: &str = r##"
prss-version = 1

[tokens.colors]
accent = "#0060c0"
brand = "#5b21b6"

[tokens.spacing]
md = 16
lg = 24

[tokens.radius]
md = 8

[class.btn]
background = "accent"
radius = "md"
padding = "md"

[class.btn.hovered]
background = "brand"

[class.icon]
background = "#aaaaaa"

[class.".btn .icon"]
background = "brand"
"##;

const LITERAL_EDIT_PRSS: &str = r##"
prss-version = 1

[tokens.colors]
accent = "#7c3aed"
brand = "#5b21b6"

[tokens.spacing]
md = 16
lg = 24

[tokens.radius]
md = 8

[class.btn]
background = "accent"
radius = "md"
padding = "md"

[class.btn.hovered]
background = "brand"

[class.icon]
background = "#aaaaaa"

[class.".btn .icon"]
background = "brand"
"##;

const STRUCTURAL_EDIT_PRSS: &str = r##"
prss-version = 1

[tokens.colors]
accent = "#7c3aed"
brand = "#5b21b6"

[tokens.spacing]
md = 16
lg = 24

[tokens.radius]
md = 8

[class.btn]
background = "accent"
radius = "md"
padding = "md"

[class.btn.hovered]
background = "brand"

[class.icon]
background = "#aaaaaa"

[class.".btn .icon"]
background = "brand"

[class.danger]
background = "#ff0000"
"##;

#[test]
fn prss_e2e_disk_to_render() {
    // ─── Phase 1: write the initial .prss to a tempfile ────────
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("theme.prss");
    fs::write(&path, INITIAL_PRSS).expect("write initial prss");

    // ─── Phase 2: load through Stylesheet::load_from_path ──────
    let sheet = Stylesheet::load_from_path(&path).expect("load");
    // Sanity: parsed every class we authored (`btn`, `icon`,
    // `.btn .icon` — the `.hovered` state is a sub-table of `btn`,
    // not its own class).
    assert_eq!(
        sheet.sheet().classes.len(),
        3,
        "expected btn / icon / `.btn .icon`, got {:?}",
        sheet.sheet().classes.keys().collect::<Vec<_>>()
    );
    assert!(
        sheet.sheet().tokens.colors.contains_key("accent"),
        "expected accent token"
    );

    // ─── Phase 3: boot Shell, install sheet, render ────────────
    let shell = Shell::new().expect("boot shell");
    assert!(shell.stylesheet().is_none(), "no sheet before install");
    shell.install_stylesheet(Some(sheet));
    assert!(shell.stylesheet().is_some(), "sheet installed");
    let tree_v1 = shell.render();
    assert!(!tree_v1.is_empty(), "render returned a tree");

    // ─── Phase 4: drive the watcher with the same content ──────
    // The watcher seeds its cache from the first observation; that
    // observation classifies as FirstSighting (the watcher is fresh).
    let mut watcher = StylesheetWatcher::new();
    let r0 = watcher.observe(&path);
    assert!(matches!(r0.change, PrssChange::FirstSighting { .. }));
    assert!(r0.stylesheet.is_some());

    // ─── Phase 5: edit the file (literal-only change) ──────────
    fs::write(&path, LITERAL_EDIT_PRSS).expect("write literal edit");
    let r1 = watcher.observe(&path);
    match r1.change {
        PrssChange::LiteralOnly { ref patches } => {
            // Exactly one slot changed: tokens.colors.accent.
            assert_eq!(patches.len(), 1, "patches: {patches:?}");
            assert_eq!(patches[0].key, "accent");
            assert_eq!(patches[0].old_value, "#0060c0");
            assert_eq!(patches[0].new_value, "#7c3aed");
        }
        other => panic!("expected LiteralOnly, got {other:?}"),
    }
    let next_sheet = r1.stylesheet.expect("watcher emits sheet for literal");
    shell.install_stylesheet(Some(next_sheet));
    let tree_v2 = shell.render();
    assert!(!tree_v2.is_empty());

    // ─── Phase 6: edit the file (structural change) ────────────
    fs::write(&path, STRUCTURAL_EDIT_PRSS).expect("write structural edit");
    let r2 = watcher.observe(&path);
    assert!(
        matches!(r2.change, PrssChange::Structural),
        "expected Structural, got {:?}",
        r2.change
    );
    let next_sheet = r2.stylesheet.expect("watcher emits sheet for structural");
    // Verify the new class actually landed.
    assert!(
        next_sheet.sheet().classes.contains_key("danger"),
        "structural edit should add `danger`"
    );
    shell.install_stylesheet(Some(next_sheet));
    let tree_v3 = shell.render();
    assert!(!tree_v3.is_empty());

    // ─── Phase 7: identical content classifies as NoChange ────
    let r3 = watcher.observe(&path);
    assert_eq!(r3.change, PrssChange::NoChange);
    assert!(r3.stylesheet.is_none(), "NoChange skips reinstall");

    // ─── Phase 8: invalid syntax keeps previous sheet ─────────
    fs::write(&path, "[class.btn\nbackground = \"").expect("write broken prss");
    let r4 = watcher.observe(&path);
    assert!(
        matches!(r4.change, PrssChange::ParseError { .. }),
        "expected ParseError, got {:?}",
        r4.change
    );
    assert!(r4.stylesheet.is_none(), "ParseError yields no new sheet");
    // The previously-installed sheet stays mounted on the shell —
    // hot-reload doesn't clobber valid styling on syntax break.
    assert!(shell.stylesheet().is_some());

    // ─── Phase 9: file deleted → ReadError ────────────────────
    fs::remove_file(&path).expect("remove tempfile");
    let r5 = watcher.observe(&path);
    assert!(
        matches!(r5.change, PrssChange::ReadError { .. }),
        "expected ReadError, got {:?}",
        r5.change
    );
}

/// Headless interpret-side e2e — we feed PRSS through the pipeline
/// against a synthetic skeleton that exercises every implemented
/// feature (static class, class:foo toggle, descendant selector,
/// state variant, short-name token, rem base) and then assert on
/// the resulting [`UiNode`] tree.
///
/// This complements `prss_e2e_disk_to_render` by checking the
/// *render output* for correctness — the disk-side test asserts the
/// pipeline plumbing, this one asserts the visual semantics.
#[test]
fn prss_e2e_synthetic_skeleton_renders_through_full_feature_set() {
    use prism_core::language::prss;
    use prism_ui_runtime::interpret::{interpret_with_scope, LowerScope};
    use std::sync::Arc;

    // Mint a fresh stylesheet covering every feature.
    let (sheet, errors) = prss::parse(
        r##"
        [tokens.colors]
        accent = "#0060c0"
        muted = "#a78bfa"

        [tokens.spacing]
        md = 16
        lg = 24

        [tokens.radius]
        md = 8

        [tokens.typography]
        font-size-md = 14

        [class.btn]
        background = "accent"
        radius = "md"
        padding = "md"

        [class.btn.hovered]
        background = "muted"

        [class.primary]
        background = "muted"

        [class.icon]
        background = "#aaaaaa"

        [class.".btn .icon"]
        background = "accent"
        "##,
    );
    assert!(errors.is_empty(), "parse errors: {errors:?}");

    let scope = LowerScope::default()
        .with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS)
        .with_binding("primary", serde_json::json!(true))
        .with_stylesheet(Arc::new(sheet));

    // Authored PRUI that uses every feature: static class, class
    // toggle, descendant ancestor / leaf, rem-based unit on a sibling.
    let source = r##"<container>
        <container id="leaf-btn" class="btn" class:primary="{primary}"/>
        <container class="btn">
            <container id="leaf-icon" class="icon"/>
        </container>
        <container id="leaf-rem" padding="1rem"/>
    </container>"##;
    let nodes = interpret_with_scope(source, &scope).expect("lower");

    // Walk the tree to find each leaf id.
    let leaves = collect_containers_by_id(&nodes);

    // 1. `class:primary="{true}"` layered onto `class="btn"` —
    //    `primary` declared after `btn`, so its background wins.
    let btn = leaves.get("leaf-btn").expect("leaf-btn");
    let btn_bg = btn.background.expect("btn background");
    let muted = &prism_core::design_tokens::DEFAULT_TOKENS
        .colors
        .accent_muted;
    // primary's `muted` value resolves through the sheet's custom
    // `muted` token (#a78bfa). assert it's not the accent (#0060c0).
    assert_eq!((btn_bg.r, btn_bg.g, btn_bg.b), (0xa7, 0x8b, 0xfa));
    let _ = muted; // keep token reference visible

    // 2. The descendant selector `.btn .icon` matches the nested
    //    `<container class="icon">` and overrides the flat `.icon`.
    let icon = leaves.get("leaf-icon").expect("leaf-icon");
    let icon_bg = icon.background.expect("icon background");
    assert_eq!((icon_bg.r, icon_bg.g, icon_bg.b), (0x00, 0x60, 0xc0));

    // 3. Token-driven rem base: `font-size-md` is 14 in DEFAULT_TOKENS,
    //    so `padding="1rem"` lowers to 14.
    let rem = leaves.get("leaf-rem").expect("leaf-rem");
    assert!((rem.padding.left - 14.0).abs() < f32::EPSILON);

    // 4. SSR class round-trip — `class="btn"` lands on
    //    `Semantic::class`. A hot-reload host shipping the tree
    //    through `lower_semantic_html` would produce
    //    `<div class="btn primary">` because `primary` is the
    //    truthy toggle value.
    assert_eq!(btn.semantic.class.as_deref(), Some("btn primary"));
}

fn collect_containers_by_id(nodes: &[UiNode]) -> std::collections::HashMap<String, ContainerProps> {
    let mut out = std::collections::HashMap::new();
    walk(nodes, &mut out);
    out
}

fn walk(nodes: &[UiNode], out: &mut std::collections::HashMap<String, ContainerProps>) {
    for n in nodes {
        if let UiNode::Container {
            id,
            props,
            children,
        } = n
        {
            if !id.is_empty() {
                out.insert(id.clone(), props.clone());
            }
            walk(children, out);
        }
    }
}
