//! Signals and connections — user-authorable event wiring between nodes.
//!
//! Components declare the signals they can emit via
//! [`Component::signals()`]. Document authors wire connections between
//! nodes: source signal -> target action. The render walker emits the
//! appropriate event handlers for each backend.
//!
//! # Naming overlap with `prism_core::reactive`
//!
//! The word "signal" is now taken twice in the workspace:
//!
//! * `prism_core::reactive::Signal<T>` — a `Copy + 'static` reactive
//!   cell used by the Dioxus-inspired runtime substrate
//!   (`docs/dev/dioxus-inspiration.md`). Reads inside a reactive
//!   context auto-subscribe; writes invalidate every subscriber.
//! * `prism_builder::signal::SignalDef` / [`Connection`] —
//!   user-authorable *event channels*. A `SignalDef` is the schema for
//!   a fireable event (`emit save`); a `Connection` is the authored
//!   wiring from a source `(node, signal)` to a target `(node,
//!   action)`. There is no reactive subscription here — it's a
//!   pure-data switchboard the [`dispatch_signal`] function walks.
//!
//! The two are deliberately coexisting (see
//! `docs/dev/dioxus-inspiration.md` §5 "Naming hygiene"). The
//! end-state plan is that the authored grammar compiles each
//! `SignalDef` into a hidden `reactive::Signal<()>` and each
//! `Connection` into a hidden `Effect`, at which point both names
//! mean the same thing underneath. Until then: when we mean the
//! reactive cell, spell it `reactive::Signal<T>`; when we mean the
//! authored event channel, say "connection signal" or `SignalDef`.

use prism_core::language::codegen::symbol_def::{SymbolDef, SymbolKind, SymbolParam};
use prism_core::language::codegen::symbol_emitter::SymbolEmmyDocEmitter;
use prism_core::language::codegen::types::{CodegenMeta, Emitter};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::document::NodeId;
use crate::registry::{FieldKind, FieldSpec};

pub type ConnectionId = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalDef {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub payload: Vec<FieldSpec>,
}

impl SignalDef {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            payload: Vec::new(),
        }
    }

    pub fn with_payload(mut self, fields: Vec<FieldSpec>) -> Self {
        self.payload = fields;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub id: ConnectionId,
    pub source_node: NodeId,
    pub signal: String,
    pub target_node: NodeId,
    pub action: ActionKind,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ActionKind {
    SetProperty {
        key: String,
        value: Value,
    },
    ToggleVisibility,
    NavigateTo {
        target: String,
    },
    PlayAnimation {
        animation: String,
    },
    EmitSignal {
        signal: String,
    },
    Custom {
        handler: String,
    },
    /// Phase 4 of `docs/dev/dioxus-inspiration.md`: declarative
    /// one-way reactive binding. Compiles to an `Effect` on document
    /// load — `target_node.target_key` mirrors the value at `source`
    /// (a path expression like `$selection.name`). The host's
    /// signals service is responsible for resolving the source path
    /// against the current reactive context and registering the
    /// effect; the dispatch executor's contribution is to surface
    /// the parsed action verbatim. Distinct from `SetProperty` —
    /// `SetProperty` writes once on a fired signal; `Bind` declares
    /// an ongoing reactive relationship.
    Bind {
        target_key: String,
        source: String,
    },
}

/// §43 A1: parsed shape of a `.prui` `on:*` attribute value.
///
/// Authoring grammar (plan §4.4):
///
/// ```text
/// on:click="emit save"           // fire the `save` signal on this node
/// on:click="cmd file.save"       // invoke the shell command by id
/// on:click="navigate /dashboard" // (parsed, runtime handler TBD)
/// on:click="luau { … }"          // custom Luau snippet (TBD)
/// ```
///
/// Only `emit` and `cmd` carry runtime handlers in this first wave —
/// the rest parse cleanly but the executor's match arm is a no-op,
/// surfaced via [`ParsedAction::Unsupported`]. This keeps every
/// shipped grammar token round-trippable through the parser without
/// gating on the full executor matrix.
#[derive(Debug, Clone, PartialEq)]
pub enum ParsedAction {
    /// `emit <signal>` — cascade through `SignalsService::fire_signal`
    /// against the clicked node's id.
    Emit { signal: String },
    /// `cmd <command-id>` — dispatch a shell command by id (the same
    /// ids the command palette exposes).
    Command { id: String },
    /// `navigate <target>` — page / workspace switch. Parsed but the
    /// host-side handler is a follow-up (§43 D3 — navigation slot).
    Navigate { target: String },
    /// `set <node>.<key> = <value>` — direct prop write. Parsed but
    /// the SignalsService path already handles this via canvas-doc
    /// connections, so the inline-action executor is a follow-up.
    SetProperty {
        node: String,
        key: String,
        value: Value,
    },
    /// `bind <node>.<key> = <source>` — reactive one-way binding,
    /// the Phase 4 grammar of `docs/dev/dioxus-inspiration.md`. The
    /// `source` is a path expression (e.g. `$selection.name`) the
    /// host resolves at install time and reads inside an `Effect`
    /// that writes back into `node.key`.
    Bind {
        node: String,
        key: String,
        source: String,
    },
    /// `toggle <node>` — visibility toggle. Same follow-up note as
    /// `SetProperty`.
    Toggle { node: String },
    /// `play <animation>` — animation trigger. Parsed; executor TBD.
    Play { animation: String },
    /// `luau { ... }` — Luau snippet. Parsed; executor lands once a
    /// real `LuauHost` plugs in (followup D2).
    Luau { source: String },
    /// Verb didn't parse — preserved as the raw string for diagnostics
    /// without poisoning the dispatch chain.
    Unsupported { raw: String },
}

/// Parse the right-hand side of a `.prui` `on:*` attribute.
///
/// The grammar splits the trimmed string into `verb rest` on the first
/// whitespace run. Empty input or a verb with no body falls through to
/// `None` so the lowering pass can skip the attribute entirely; valid
/// verbs whose runtime handler isn't wired yet still parse — they land
/// as [`ParsedAction::Unsupported`] (or the variant-specific carrier),
/// so the executor can decide "no-op for now" without losing the
/// authored text.
pub fn parse_action(raw: &str) -> Option<ParsedAction> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(body) = trimmed.strip_prefix("luau") {
        // `luau { ... }` — capture the brace-delimited body if present;
        // otherwise treat the rest as a Luau expression.
        let body = body.trim_start();
        let snippet = body
            .strip_prefix('{')
            .and_then(|b| b.strip_suffix('}'))
            .map(str::trim)
            .unwrap_or(body)
            .to_string();
        if snippet.is_empty() {
            return None;
        }
        return Some(ParsedAction::Luau { source: snippet });
    }
    let (verb, rest) = trimmed
        .split_once(char::is_whitespace)
        .map(|(v, r)| (v, r.trim()))
        .unwrap_or((trimmed, ""));
    if rest.is_empty() {
        // `cmd` with no body is meaningless — same for `emit`. Return
        // `None` so the lower pass treats it as "no handler authored".
        return None;
    }
    Some(match verb {
        "emit" => ParsedAction::Emit {
            signal: rest.to_string(),
        },
        "cmd" => ParsedAction::Command {
            id: rest.to_string(),
        },
        "navigate" => ParsedAction::Navigate {
            target: rest.to_string(),
        },
        "toggle" => ParsedAction::Toggle {
            node: rest.to_string(),
        },
        "play" => ParsedAction::Play {
            animation: rest.to_string(),
        },
        "set" => {
            parse_set_body(rest).unwrap_or_else(|| ParsedAction::Unsupported { raw: raw.into() })
        }
        "bind" => {
            parse_bind_body(rest).unwrap_or_else(|| ParsedAction::Unsupported { raw: raw.into() })
        }
        _ => ParsedAction::Unsupported { raw: raw.into() },
    })
}

/// Parse the body of a `bind` action. Shape:
/// `<node>.<key> = <source-path>`. Source paths are kept verbatim
/// at parse time — `$selection.name`, `$user.email`, etc. The host
/// signals service resolves them at install time and registers the
/// resulting `Effect`. Empty / malformed bodies return `None`.
fn parse_bind_body(rest: &str) -> Option<ParsedAction> {
    let (lhs, source_raw) = rest.split_once('=')?;
    let lhs = lhs.trim();
    let source = source_raw.trim();
    let (node, key) = lhs.split_once('.')?;
    let node = node.trim();
    let key = key.trim();
    if node.is_empty() || key.is_empty() || source.is_empty() {
        return None;
    }
    Some(ParsedAction::Bind {
        node: node.to_string(),
        key: key.to_string(),
        source: source.to_string(),
    })
}

fn parse_set_body(rest: &str) -> Option<ParsedAction> {
    // Shape: `<node>.<key> = <value>`. The value is JSON-parsed when
    // it looks like a literal (`true` / `42` / `"text"` / `[…]`),
    // otherwise it lands as a string — same coercion the resolver
    // uses for bare attributes.
    let (lhs, value_raw) = rest.split_once('=')?;
    let lhs = lhs.trim();
    let value_raw = value_raw.trim();
    let (node, key) = lhs.split_once('.')?;
    let node = node.trim();
    let key = key.trim();
    if node.is_empty() || key.is_empty() || value_raw.is_empty() {
        return None;
    }
    let value = serde_json::from_str::<Value>(value_raw)
        .unwrap_or_else(|_| Value::String(value_raw.to_string()));
    Some(ParsedAction::SetProperty {
        node: node.to_string(),
        key: key.to_string(),
        value,
    })
}

/// Payload carried by a fired signal — the source node, signal name,
/// and a JSON map of the payload fields (matching the `SignalDef::payload`
/// spec).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalEvent {
    pub source_node: NodeId,
    pub signal: String,
    #[serde(default)]
    pub payload: serde_json::Map<String, Value>,
}

/// Result of evaluating one connection's action.
#[derive(Debug, Clone)]
pub enum DispatchResult {
    SetProperty {
        target_node: NodeId,
        key: String,
        value: Value,
    },
    ToggleVisibility {
        target_node: NodeId,
    },
    NavigateTo {
        target: String,
    },
    PlayAnimation {
        target_node: NodeId,
        animation: String,
    },
    EmitSignal {
        target_node: NodeId,
        signal: String,
    },
    Custom {
        handler: String,
        payload: serde_json::Map<String, Value>,
    },
    /// Phase 4 of `docs/dev/dioxus-inspiration.md`. The host signals
    /// service receives this and is expected to register an `Effect`
    /// reading from `source` and writing into
    /// `target_node.target_key` whenever the source updates.
    Bind {
        target_node: NodeId,
        target_key: String,
        source: String,
    },
}

/// Evaluate a signal event against a document's connection list.
/// Returns the list of actions to execute — the caller (shell) is
/// responsible for actually applying them (mutating nodes, navigating,
/// invoking Luau handlers, etc.).
pub fn dispatch_signal(event: &SignalEvent, connections: &[Connection]) -> Vec<DispatchResult> {
    connections
        .iter()
        .filter(|c| c.source_node == event.source_node && c.signal == event.signal)
        .map(|c| match &c.action {
            ActionKind::SetProperty { key, value } => DispatchResult::SetProperty {
                target_node: c.target_node.clone(),
                key: key.clone(),
                value: value.clone(),
            },
            ActionKind::ToggleVisibility => DispatchResult::ToggleVisibility {
                target_node: c.target_node.clone(),
            },
            ActionKind::NavigateTo { target } => DispatchResult::NavigateTo {
                target: target.clone(),
            },
            ActionKind::PlayAnimation { animation } => DispatchResult::PlayAnimation {
                target_node: c.target_node.clone(),
                animation: animation.clone(),
            },
            ActionKind::EmitSignal { signal } => DispatchResult::EmitSignal {
                target_node: c.target_node.clone(),
                signal: signal.clone(),
            },
            ActionKind::Custom { handler } => DispatchResult::Custom {
                handler: handler.clone(),
                payload: event.payload.clone(),
            },
            ActionKind::Bind { target_key, source } => DispatchResult::Bind {
                target_node: c.target_node.clone(),
                target_key: target_key.clone(),
                source: source.clone(),
            },
        })
        .collect()
}

/// Standard interaction signals that every component receives automatically.
/// Component-specific signals (e.g. `submitted`, `tab-changed`) are appended
/// by the component's own `signals()` impl via [`with_common_signals`].
pub fn common_signals() -> Vec<SignalDef> {
    vec![
        SignalDef::new("clicked", "Fires when the component is clicked").with_payload(vec![
            FieldSpec::number("x", "Mouse X position", Default::default()),
            FieldSpec::number("y", "Mouse Y position", Default::default()),
        ]),
        SignalDef::new("double-clicked", "Fires on double-click").with_payload(vec![
            FieldSpec::number("x", "Mouse X position", Default::default()),
            FieldSpec::number("y", "Mouse Y position", Default::default()),
        ]),
        SignalDef::new("hovered", "Fires when the pointer enters the component").with_payload(
            vec![
                FieldSpec::number("x", "Mouse X position", Default::default()),
                FieldSpec::number("y", "Mouse Y position", Default::default()),
            ],
        ),
        SignalDef::new("hover-ended", "Fires when the pointer leaves the component"),
        SignalDef::new("drag-started", "Fires when a drag begins on the component").with_payload(
            vec![
                FieldSpec::number("x", "Start X position", Default::default()),
                FieldSpec::number("y", "Start Y position", Default::default()),
            ],
        ),
        SignalDef::new("drag-moved", "Fires while dragging over the component").with_payload(vec![
            FieldSpec::number("x", "Current X position", Default::default()),
            FieldSpec::number("y", "Current Y position", Default::default()),
            FieldSpec::number("dx", "Delta X from start", Default::default()),
            FieldSpec::number("dy", "Delta Y from start", Default::default()),
        ]),
        SignalDef::new("drag-ended", "Fires when a drag ends (drop or cancel)").with_payload(vec![
            FieldSpec::number("x", "End X position", Default::default()),
            FieldSpec::number("y", "End Y position", Default::default()),
        ]),
        SignalDef::new("changed", "Fires when a property value changes").with_payload(vec![
            FieldSpec::text("key", "Property key that changed"),
            FieldSpec::text("value", "New value (serialized)"),
            FieldSpec::text("old_value", "Previous value (serialized)"),
        ]),
        SignalDef::new("focused", "Fires when the component gains focus"),
        SignalDef::new("blurred", "Fires when the component loses focus"),
        SignalDef::new(
            "deleted",
            "Fires when the component is removed from the document",
        ),
        SignalDef::new(
            "mounted",
            "Fires when the component is first added to the document",
        ),
    ]
}

/// Prepend the common interaction signals, then append component-specific extras.
/// Deduplicates by name — if a component declares a signal with the same name
/// as a common one, the component-specific definition wins (it replaces the
/// common version so payloads can be specialised).
pub fn with_common_signals(component_signals: Vec<SignalDef>) -> Vec<SignalDef> {
    let mut result = common_signals();
    for sig in component_signals {
        if let Some(pos) = result.iter().position(|s| s.name == sig.name) {
            result[pos] = sig;
        } else {
            result.push(sig);
        }
    }
    result
}

fn field_kind_to_luau_type(kind: &FieldKind) -> &'static str {
    match kind {
        FieldKind::Text | FieldKind::TextArea | FieldKind::Color | FieldKind::Select { .. } => {
            "string"
        }
        FieldKind::Number { .. }
        | FieldKind::Integer { .. }
        | FieldKind::Duration
        | FieldKind::Currency { .. } => "number",
        FieldKind::Boolean => "boolean",
        FieldKind::File { .. } => "string",
        FieldKind::Date | FieldKind::DateTime => "string",
        FieldKind::Calculation { .. } => "any",
        FieldKind::Custom { .. } => "any",
    }
}

/// Generate a `SymbolDef` class for the signals a component exposes.
/// The class name is `{ComponentId}Signals` (PascalCase); each signal
/// becomes a method whose parameters mirror the signal's payload fields.
///
/// Feed the result into `SymbolEmmyDocEmitter` to produce a `.d.luau`
/// type stub that LuaLS picks up for autocomplete.
pub fn signal_symbols(component_id: &str, signals: &[SignalDef]) -> SymbolDef {
    let class_name = format!(
        "{}Signals",
        component_id
            .split('-')
            .map(|w| {
                let mut c = w.chars();
                match c.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
                    None => String::new(),
                }
            })
            .collect::<String>()
    );

    let children = signals
        .iter()
        .map(|sig| {
            let handler_params: Vec<SymbolParam> = sig
                .payload
                .iter()
                .map(|f| SymbolParam::new(&f.key, field_kind_to_luau_type(&f.kind)))
                .collect();

            let handler_type = if handler_params.is_empty() {
                "fun()".to_string()
            } else {
                let params = handler_params
                    .iter()
                    .map(|p| format!("{}: {}", p.name, p.r#type))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("fun({params})")
            };

            SymbolDef {
                name: sig.name.replace('-', "_"),
                kind: SymbolKind::Function,
                description: Some(sig.description.clone()),
                params: vec![SymbolParam::new("handler", &handler_type)],
                returns: Some("()".into()),
                ..SymbolDef::default()
            }
        })
        .collect();

    SymbolDef {
        name: class_name,
        kind: SymbolKind::Class,
        description: Some(format!(
            "Signal handlers for the `{component_id}` component"
        )),
        children,
        ..SymbolDef::default()
    }
}

/// Generate `.d.luau` type stubs for every component's signals.
///
/// Iterates the registry, calls [`signal_symbols`] for each component,
/// and feeds the combined `Vec<SymbolDef>` into [`SymbolEmmyDocEmitter`]
/// to produce a single `signals.d.luau` file. Returns the file content
/// or an empty string if the registry has no components.
pub fn generate_signal_type_stubs(
    registry: &crate::registry::ComponentRegistry,
    project_name: &str,
) -> String {
    let symbols: Vec<SymbolDef> = registry
        .iter()
        .map(|(id, comp)| signal_symbols(id, &comp.signals()))
        .collect();
    if symbols.is_empty() {
        return String::new();
    }
    let emitter = SymbolEmmyDocEmitter::new()
        .with_global_name("Signals")
        .with_return_table("Signals");
    let meta = CodegenMeta::new(project_name);
    let result = emitter.emit(&symbols, &meta);
    result
        .files
        .into_iter()
        .next()
        .map(|f| f.content)
        .unwrap_or_default()
}

/// Bridge from builder [`SignalDef`]s to [`SignalContext`]s for the Luau
/// syntax provider. Converts payload `FieldSpec`s into `SignalPayloadField`
/// entries with Luau type strings.
pub fn signal_contexts(signals: &[SignalDef]) -> Vec<prism_core::language::syntax::SignalContext> {
    signals
        .iter()
        .map(|sig| prism_core::language::syntax::SignalContext {
            name: sig.name.clone(),
            description: sig.description.clone(),
            payload: sig
                .payload
                .iter()
                .map(|f| prism_core::language::syntax::SignalPayloadField {
                    name: f.key.clone(),
                    luau_type: field_kind_to_luau_type(&f.kind).to_string(),
                })
                .collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_action_emit_picks_the_signal_name() {
        let a = parse_action("emit save").expect("parses");
        match a {
            ParsedAction::Emit { signal } => assert_eq!(signal, "save"),
            other => panic!("expected Emit, got {other:?}"),
        }
    }

    #[test]
    fn parse_action_cmd_picks_the_command_id() {
        match parse_action("cmd file.save").expect("parses") {
            ParsedAction::Command { id } => assert_eq!(id, "file.save"),
            other => panic!("expected Command, got {other:?}"),
        }
    }

    #[test]
    fn parse_action_set_round_trips_node_key_and_json_value() {
        match parse_action("set modal.open = true").expect("parses") {
            ParsedAction::SetProperty { node, key, value } => {
                assert_eq!(node, "modal");
                assert_eq!(key, "open");
                assert_eq!(value, json!(true));
            }
            other => panic!("expected SetProperty, got {other:?}"),
        }
    }

    #[test]
    fn parse_action_bind_round_trips_node_key_and_source_path() {
        // Phase 4 of `docs/dev/dioxus-inspiration.md`. Source paths
        // are kept verbatim — the host resolves them at install
        // time, this layer just parses the surface grammar.
        match parse_action("bind modal.title = $selection.name").expect("parses") {
            ParsedAction::Bind { node, key, source } => {
                assert_eq!(node, "modal");
                assert_eq!(key, "title");
                assert_eq!(source, "$selection.name");
            }
            other => panic!("expected Bind, got {other:?}"),
        }
    }

    #[test]
    fn parse_action_bind_rejects_missing_lhs_or_rhs() {
        // Missing equals → unsupported.
        match parse_action("bind modal.title").expect("parses") {
            ParsedAction::Unsupported { .. } => {}
            other => panic!("expected Unsupported, got {other:?}"),
        }
        // Empty source path → unsupported.
        match parse_action("bind modal.title =").expect("parses") {
            ParsedAction::Unsupported { .. } => {}
            other => panic!("expected Unsupported, got {other:?}"),
        }
        // Missing node.key dot → unsupported.
        match parse_action("bind title = $selection.name").expect("parses") {
            ParsedAction::Unsupported { .. } => {}
            other => panic!("expected Unsupported, got {other:?}"),
        }
    }

    #[test]
    fn parse_action_luau_unwraps_brace_block() {
        match parse_action("luau { print('hi') }").expect("parses") {
            ParsedAction::Luau { source } => assert_eq!(source, "print('hi')"),
            other => panic!("expected Luau, got {other:?}"),
        }
    }

    #[test]
    fn parse_action_returns_none_for_empty_or_body_only_input() {
        assert!(parse_action("").is_none());
        assert!(parse_action("   ").is_none());
        // Verb with no body — meaningless, treated as no handler.
        assert!(parse_action("emit").is_none());
        assert!(parse_action("cmd  ").is_none());
    }

    #[test]
    fn parse_action_unknown_verb_lands_as_unsupported_for_diagnostics() {
        match parse_action("yodel high-and-low").expect("parses") {
            ParsedAction::Unsupported { raw } => assert_eq!(raw, "yodel high-and-low"),
            other => panic!("expected Unsupported, got {other:?}"),
        }
    }

    #[test]
    fn signal_def_builder() {
        let sig = SignalDef::new("clicked", "Fires when the button is pressed")
            .with_payload(vec![FieldSpec::text("target", "Event target")]);
        assert_eq!(sig.name, "clicked");
        assert_eq!(sig.payload.len(), 1);
    }

    #[test]
    fn connection_round_trips() {
        let conn = Connection {
            id: "c1".into(),
            source_node: "btn-1".into(),
            signal: "clicked".into(),
            target_node: "modal-1".into(),
            action: ActionKind::ToggleVisibility,
            params: json!({}),
        };
        let json = serde_json::to_string(&conn).unwrap();
        let back: Connection = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "c1");
        assert_eq!(back.signal, "clicked");
        assert!(matches!(back.action, ActionKind::ToggleVisibility));
    }

    #[test]
    fn set_property_action_serializes() {
        let action = ActionKind::SetProperty {
            key: "visible".into(),
            value: json!(true),
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("set-property"));
        let back: ActionKind = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, ActionKind::SetProperty { .. }));
    }

    #[test]
    fn navigate_action_serializes() {
        let action = ActionKind::NavigateTo {
            target: "/dashboard".into(),
        };
        let json = serde_json::to_string(&action).unwrap();
        let back: ActionKind = serde_json::from_str(&json).unwrap();
        match back {
            ActionKind::NavigateTo { target } => assert_eq!(target, "/dashboard"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn custom_action_serializes() {
        let action = ActionKind::Custom {
            handler: "onSubmit".into(),
        };
        let json = serde_json::to_string(&action).unwrap();
        let back: ActionKind = serde_json::from_str(&json).unwrap();
        match back {
            ActionKind::Custom { handler } => assert_eq!(handler, "onSubmit"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn common_signals_covers_all_interaction_types() {
        let sigs = common_signals();
        let names: Vec<&str> = sigs.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"clicked"));
        assert!(names.contains(&"double-clicked"));
        assert!(names.contains(&"hovered"));
        assert!(names.contains(&"hover-ended"));
        assert!(names.contains(&"drag-started"));
        assert!(names.contains(&"drag-moved"));
        assert!(names.contains(&"drag-ended"));
        assert!(names.contains(&"changed"));
        assert!(names.contains(&"focused"));
        assert!(names.contains(&"blurred"));
        assert!(names.contains(&"deleted"));
        assert!(names.contains(&"mounted"));
        assert_eq!(sigs.len(), 12);
    }

    #[test]
    fn common_signals_have_payloads_where_expected() {
        let sigs = common_signals();
        let clicked = sigs.iter().find(|s| s.name == "clicked").unwrap();
        assert_eq!(clicked.payload.len(), 2);
        let drag_moved = sigs.iter().find(|s| s.name == "drag-moved").unwrap();
        assert_eq!(drag_moved.payload.len(), 4);
        let changed = sigs.iter().find(|s| s.name == "changed").unwrap();
        assert_eq!(changed.payload.len(), 3);
        let blurred = sigs.iter().find(|s| s.name == "blurred").unwrap();
        assert!(blurred.payload.is_empty());
    }

    #[test]
    fn with_common_signals_deduplicates_by_name() {
        let custom_clicked = SignalDef::new("clicked", "Custom click handler")
            .with_payload(vec![FieldSpec::text("target", "Click target")]);
        let extra = SignalDef::new("submitted", "Form submitted");

        let result = with_common_signals(vec![custom_clicked, extra]);

        let clicked = result.iter().find(|s| s.name == "clicked").unwrap();
        assert_eq!(clicked.description, "Custom click handler");
        assert_eq!(clicked.payload.len(), 1);

        assert!(result.iter().any(|s| s.name == "submitted"));
        assert!(result.iter().any(|s| s.name == "hovered"));

        let click_count = result.iter().filter(|s| s.name == "clicked").count();
        assert_eq!(click_count, 1);
    }

    #[test]
    fn with_common_signals_preserves_all_common_when_no_overlap() {
        let extra = SignalDef::new("custom-event", "A custom event");
        let result = with_common_signals(vec![extra]);
        assert_eq!(result.len(), 13); // 12 common + 1 extra
    }

    #[test]
    fn signal_symbols_generates_class_with_methods() {
        let signals = vec![
            SignalDef::new("clicked", "Click").with_payload(vec![
                FieldSpec::number("x", "X", Default::default()),
                FieldSpec::number("y", "Y", Default::default()),
            ]),
            SignalDef::new("hover-ended", "Hover left"),
        ];
        let sym = signal_symbols("button", &signals);
        assert_eq!(sym.name, "ButtonSignals");
        assert_eq!(sym.kind, SymbolKind::Class);
        assert_eq!(sym.children.len(), 2);

        let clicked = &sym.children[0];
        assert_eq!(clicked.name, "clicked");
        assert_eq!(clicked.params.len(), 1);
        assert_eq!(clicked.params[0].r#type, "fun(x: number, y: number)");

        let hover = &sym.children[1];
        assert_eq!(hover.name, "hover_ended");
        assert_eq!(hover.params[0].r#type, "fun()");
    }

    #[test]
    fn signal_symbols_pascal_cases_kebab_id() {
        let sym = signal_symbols("my-component", &[]);
        assert_eq!(sym.name, "MyComponentSignals");
    }

    #[test]
    fn dispatch_signal_matches_source_and_signal_name() {
        let connections = vec![
            Connection {
                id: "c1".into(),
                source_node: "btn".into(),
                signal: "clicked".into(),
                target_node: "modal".into(),
                action: ActionKind::ToggleVisibility,
                params: json!({}),
            },
            Connection {
                id: "c2".into(),
                source_node: "btn".into(),
                signal: "hovered".into(),
                target_node: "tooltip".into(),
                action: ActionKind::ToggleVisibility,
                params: json!({}),
            },
            Connection {
                id: "c3".into(),
                source_node: "other".into(),
                signal: "clicked".into(),
                target_node: "panel".into(),
                action: ActionKind::ToggleVisibility,
                params: json!({}),
            },
        ];
        let event = SignalEvent {
            source_node: "btn".into(),
            signal: "clicked".into(),
            payload: serde_json::Map::new(),
        };
        let results = dispatch_signal(&event, &connections);
        assert_eq!(results.len(), 1);
        assert!(matches!(
            &results[0],
            DispatchResult::ToggleVisibility { target_node } if target_node == "modal"
        ));
    }

    #[test]
    fn dispatch_signal_multiple_actions_same_signal() {
        let connections = vec![
            Connection {
                id: "c1".into(),
                source_node: "btn".into(),
                signal: "clicked".into(),
                target_node: "a".into(),
                action: ActionKind::ToggleVisibility,
                params: json!({}),
            },
            Connection {
                id: "c2".into(),
                source_node: "btn".into(),
                signal: "clicked".into(),
                target_node: "b".into(),
                action: ActionKind::NavigateTo {
                    target: "/home".into(),
                },
                params: json!({}),
            },
        ];
        let event = SignalEvent {
            source_node: "btn".into(),
            signal: "clicked".into(),
            payload: serde_json::Map::new(),
        };
        let results = dispatch_signal(&event, &connections);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn dispatch_signal_custom_carries_payload() {
        let connections = vec![Connection {
            id: "c1".into(),
            source_node: "input".into(),
            signal: "changed".into(),
            target_node: "self".into(),
            action: ActionKind::Custom {
                handler: "validate".into(),
            },
            params: json!({}),
        }];
        let mut payload = serde_json::Map::new();
        payload.insert("key".into(), json!("email"));
        payload.insert("value".into(), json!("test@example.com"));
        let event = SignalEvent {
            source_node: "input".into(),
            signal: "changed".into(),
            payload,
        };
        let results = dispatch_signal(&event, &connections);
        assert_eq!(results.len(), 1);
        match &results[0] {
            DispatchResult::Custom { handler, payload } => {
                assert_eq!(handler, "validate");
                assert_eq!(payload.get("key").unwrap(), "email");
            }
            _ => panic!("expected Custom"),
        }
    }

    #[test]
    fn dispatch_signal_bind_carries_target_key_and_source() {
        // Phase 4 of `docs/dev/dioxus-inspiration.md`. The dispatch
        // executor surfaces `Bind` verbatim; the host's signals
        // service receives this and registers an `Effect` that
        // reads from `source` and writes into `target_node.target_key`.
        let connections = vec![Connection {
            id: "c1".into(),
            source_node: "btn".into(),
            signal: "loaded".into(),
            target_node: "modal".into(),
            action: ActionKind::Bind {
                target_key: "title".into(),
                source: "$selection.name".into(),
            },
            params: Value::Null,
        }];
        let event = SignalEvent {
            source_node: "btn".into(),
            signal: "loaded".into(),
            payload: Default::default(),
        };
        let results = dispatch_signal(&event, &connections);
        assert_eq!(results.len(), 1);
        match &results[0] {
            DispatchResult::Bind {
                target_node,
                target_key,
                source,
            } => {
                assert_eq!(target_node, "modal");
                assert_eq!(target_key, "title");
                assert_eq!(source, "$selection.name");
            }
            other => panic!("expected Bind, got {other:?}"),
        }
    }

    #[test]
    fn action_kind_bind_round_trips_through_serde() {
        let action = ActionKind::Bind {
            target_key: "title".into(),
            source: "$selection.name".into(),
        };
        let json = serde_json::to_string(&action).unwrap();
        let back: ActionKind = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, ActionKind::Bind { .. }));
        // Verify the tagged discriminator is "bind" (kebab-case).
        assert!(json.contains("\"bind\""));
    }

    #[test]
    fn signal_event_round_trips_through_serde() {
        let mut payload = serde_json::Map::new();
        payload.insert("x".into(), json!(42.0));
        let event = SignalEvent {
            source_node: "btn".into(),
            signal: "clicked".into(),
            payload,
        };
        let json = serde_json::to_string(&event).unwrap();
        let back: SignalEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back.source_node, "btn");
        assert_eq!(back.signal, "clicked");
        assert_eq!(back.payload.get("x").unwrap(), &json!(42.0));
    }

    #[test]
    fn generate_signal_type_stubs_produces_luau_output() {
        let mut registry = crate::registry::ComponentRegistry::new();
        crate::starter::register_builtins(&mut registry).unwrap();
        let output = generate_signal_type_stubs(&registry, "prism-test");
        assert!(!output.is_empty());
        assert!(output.contains("-- Generated by Prism Codegen"));
        assert!(output.contains("ButtonSignals"));
        assert!(output.contains("TextSignals"));
        assert!(output.contains("clicked"));
        assert!(output.contains("hovered"));
        assert!(output.contains("return Signals"));
    }

    #[test]
    fn generate_signal_type_stubs_empty_registry() {
        let registry = crate::registry::ComponentRegistry::new();
        let output = generate_signal_type_stubs(&registry, "empty");
        assert!(output.is_empty());
    }

    #[test]
    fn signal_contexts_bridges_to_syntax_types() {
        let signals = vec![
            SignalDef::new("clicked", "Fires on click").with_payload(vec![
                FieldSpec::number("x", "Mouse X", Default::default()),
                FieldSpec::number("y", "Mouse Y", Default::default()),
            ]),
            SignalDef::new("deleted", "Fires on remove"),
        ];
        let contexts = signal_contexts(&signals);
        assert_eq!(contexts.len(), 2);
        assert_eq!(contexts[0].name, "clicked");
        assert_eq!(contexts[0].payload.len(), 2);
        assert_eq!(contexts[0].payload[0].name, "x");
        assert_eq!(contexts[0].payload[0].luau_type, "number");
        assert_eq!(contexts[1].name, "deleted");
        assert!(contexts[1].payload.is_empty());
    }

    #[test]
    fn codegen_round_trip_all_builtin_components() {
        let mut registry = crate::registry::ComponentRegistry::new();
        crate::starter::register_builtins(&mut registry).unwrap();
        for (id, comp) in registry.iter() {
            let signals = comp.signals();
            assert!(
                !signals.is_empty(),
                "component {id} should have at least common signals"
            );
            let sym = signal_symbols(id, &signals);
            assert_eq!(sym.kind, SymbolKind::Class);
            assert!(!sym.children.is_empty());

            let contexts = signal_contexts(&signals);
            assert_eq!(contexts.len(), signals.len());
        }
    }
}
