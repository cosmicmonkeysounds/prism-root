//! Phase-5 end-to-end coverage:
//!   * inline `<…>` directives mid-action / mid-dialogue text
//!   * `-> (target) ->` tunnel calls + `<-` returns
//!   * `with k: v` beat parameters available inside the callee
//!   * `<for: x in [list]>` iteration that unrolls the body once
//!     per element with `x` bound in the world scope

use std::sync::Arc;

use loom_runtime::{Bundle, Event, Playhead, Step};

#[test]
fn inline_directive_fires_and_strips_from_text() {
    const SRC: &str = "\
entry: scene

== scene

WREN
  (startled)
  Lightning?<sfx: thunder, fade: 200>
";
    let bundle = Arc::new(Bundle::from_sources([("main.loom", SRC)]));
    let mut p = Playhead::new(bundle).unwrap();

    // The dialogue event surfaces with the inline chunk stripped.
    match p.step().unwrap() {
        Step::Event(Event::Dialogue { text, .. }) => {
            assert_eq!(text, "Lightning?", "inline directive must be removed");
        }
        other => panic!("expected dialogue, got {other:?}"),
    }
    // The ledger records the `sfx` directive envelope as a
    // side-effect alongside the dialogue.
    let events = p.ledger().events();
    let sfx = events
        .iter()
        .find_map(|e| match e {
            Event::Directive {
                kind,
                positional,
                named,
            } if kind == "sfx" => Some((positional.clone(), named.clone())),
            _ => None,
        })
        .expect("sfx directive must hit the ledger");
    assert_eq!(sfx.0, vec!["thunder".to_string()]);
    assert_eq!(sfx.1, vec![("fade".to_string(), "200".to_string())]);
}

#[test]
fn tunnel_call_returns_to_caller_mid_beat() {
    const SRC: &str = "\
entry: opening

== opening

Start.
-> (interlude) ->
End.

== interlude

Interlude line.
<-
";
    let bundle = Arc::new(Bundle::from_sources([("main.loom", SRC)]));
    let mut p = Playhead::new(bundle).unwrap();

    let mut actions = Vec::new();
    loop {
        match p.step().unwrap() {
            Step::Event(Event::Action { text }) => actions.push(text),
            Step::Ended => break,
            _ => {}
        }
    }
    assert_eq!(
        actions,
        vec![
            "Start.".to_string(),
            "Interlude line.".to_string(),
            "End.".to_string()
        ],
        "tunnel must run callee then resume caller"
    );
}

#[test]
fn beat_param_binds_into_world() {
    const SRC: &str = "\
entry: opening

== opening

-> talk with topic: bell

== talk

Topic was {topic}.
";
    let bundle = Arc::new(Bundle::from_sources([("main.loom", SRC)]));
    let mut p = Playhead::new(bundle).unwrap();

    let mut last_action = None;
    loop {
        match p.step().unwrap() {
            Step::Event(Event::Action { text }) => last_action = Some(text),
            Step::Ended => break,
            _ => {}
        }
    }
    assert_eq!(last_action.as_deref(), Some("Topic was bell."));
}

#[test]
fn for_loop_unrolls_body_per_element() {
    const SRC: &str = "\
entry: roll

== roll

<for: name in ['Wren', 'Pell', 'Mara']>
  Hello, {name}.
";
    let bundle = Arc::new(Bundle::from_sources([("main.loom", SRC)]));
    let mut p = Playhead::new(bundle).unwrap();

    let mut greetings = Vec::new();
    loop {
        match p.step().unwrap() {
            Step::Event(Event::Action { text }) => greetings.push(text),
            Step::Ended => break,
            _ => {}
        }
    }
    assert_eq!(
        greetings,
        vec![
            "Hello, Wren.".to_string(),
            "Hello, Pell.".to_string(),
            "Hello, Mara.".to_string(),
        ]
    );
}
