//! Dev-only diagnostic: prints the production-render tree shape so
//! we can spot structural bugs (missing panels, wrong directions,
//! off-screen siblings) without launching a window. Run with
//! `cargo test -p prism-shell --test dump_tree -- --nocapture`.
//!
//! Asserts nothing — the printout itself is the artefact. Lives in
//! `tests/` so it doesn't appear in the regular `cargo test` output.

use prism_shell::Shell;
use prism_ui_runtime::layout::Node as UiNode;

#[test]
#[ignore]
fn dump_production_tree() {
    let shell = Shell::new().expect("boot");
    let nodes = shell.render();
    for (i, n) in nodes.iter().enumerate() {
        println!("=== top-level [{i}] ===");
        dump(n, 0);
    }
}

fn dump(n: &UiNode, depth: usize) {
    let pad = "  ".repeat(depth);
    match n {
        UiNode::Container {
            id,
            props,
            children,
        } => {
            let role = props
                .semantic
                .attrs
                .iter()
                .find(|(k, _)| k == "data-role")
                .map(|(_, v)| v.as_str())
                .unwrap_or("");
            let panel = props
                .semantic
                .attrs
                .iter()
                .find(|(k, _)| k == "data-panel")
                .map(|(_, v)| v.as_str())
                .unwrap_or("");
            println!(
                "{pad}C id={id:?} role={role:?} panel={panel:?} w={:?} h={:?} dir={:?} kids={}",
                props.width,
                props.height,
                props.direction,
                children.len()
            );
            for c in children {
                dump(c, depth + 1);
            }
        }
        UiNode::Text { id, content, props } => {
            let trunc: String = content.chars().take(40).collect();
            println!("{pad}T id={id:?} fs={} \"{trunc}\"", props.font_size);
        }
        UiNode::Image { id, source, .. } => {
            println!("{pad}I id={id:?} src={source:?}")
        }
        UiNode::Spacer { id, width, height } => {
            println!("{pad}S id={id:?} {width}x{height}")
        }
        UiNode::TextInput { id, value, .. } => {
            println!("{pad}TI id={id:?} value={value:?}")
        }
    }
}
