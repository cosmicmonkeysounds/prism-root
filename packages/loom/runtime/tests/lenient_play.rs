//! Graceful-degradation contract for the in-browser play loop.
//!
//! A directive with no Rust handler and no Luau backing must NOT abort
//! a play session when the registry is lenient — that's the mode the
//! wasm editor bundle runs in (the Luau VM can't compile to
//! `wasm32-unknown-unknown`). The directive surfaces as a logged
//! `Event::Directive` envelope and the narrative keeps going. Native
//! builds with the `luau` feature stay strict so typos / missing
//! extensions still surface loudly.

use std::sync::Arc;

use loom_runtime::directives::Registry;
use loom_runtime::{Bundle, Event, Mesh, Playhead, Step};

const SRC: &str = "\
# Lenient
entry: a

== a
A thing occurs.

<totally_unknown_directive: foo, n: 3>

More happens.

-> END
";

#[test]
fn lenient_registry_plays_through_unknown_directive() {
    let mut reg = Registry::with_builtins();
    reg.lenient = true;
    let bundle = Arc::new(Bundle::from_sources(vec![(
        "main.loom".to_string(),
        SRC.to_string(),
    )]));
    let head = Playhead::with_registry(Arc::clone(&bundle), Arc::new(reg)).expect("playhead");
    let mut mesh = Mesh::from_playhead(head);

    let mut saw_directive = false;
    for _ in 0..1000 {
        let (_, step) = mesh
            .step()
            .expect("a lenient registry must not error on an unknown directive");
        match step {
            Step::Ended => {
                assert!(
                    saw_directive,
                    "the unknown directive should have surfaced as a logged envelope"
                );
                return;
            }
            Step::Event(Event::Directive { kind, .. })
                if kind == "totally_unknown_directive" =>
            {
                saw_directive = true;
            }
            _ => {}
        }
    }
    panic!("playhead did not reach the end");
}

#[cfg(feature = "luau")]
#[test]
fn strict_registry_errors_on_unknown_directive() {
    // The default native registry is strict, so the same source should
    // abort rather than silently swallow the directive.
    let bundle = Arc::new(Bundle::from_sources(vec![(
        "main.loom".to_string(),
        SRC.to_string(),
    )]));
    let mut head = Playhead::new(Arc::clone(&bundle)).expect("playhead");
    for _ in 0..1000 {
        match head.step() {
            Ok(Step::Ended) => panic!("strict registry should have rejected the directive"),
            Ok(_) => {}
            Err(_) => return,
        }
    }
    panic!("playhead neither errored nor ended");
}
