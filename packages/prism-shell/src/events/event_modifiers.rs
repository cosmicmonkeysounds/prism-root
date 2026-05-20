//! Parsed `data-on-<event>-<mods>` suffix decoder.
//!
//! Authors write `on:click.once.stop="cmd save"`; the PRUI lowering
//! pass joins the dotted suffix with dashes (`data-on-click-once-stop`)
//! and this module splits the trailing chunk back into a typed
//! [`EventModifiers`] struct. Unknown segments are dropped.

/// Parsed event-modifier suffix attached to a `data-on-<event>` attr
/// key. Authors write `on:click.once.stop="cmd save"` — the lowering
/// pass joins the dotted suffix with dashes (`data-on-click-once-stop`)
/// so this struct just splits the suffix on `-` and matches each
/// segment against the supported set. Unknown segments are dropped.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct EventModifiers {
    pub(super) once: bool,
    pub(super) stop: bool,
    pub(super) prevent: bool,
}

impl EventModifiers {
    pub(super) fn parse(suffix: &str) -> Self {
        let mut m = Self::default();
        if suffix.is_empty() {
            return m;
        }
        for part in suffix.split('-') {
            match part {
                "once" => m.once = true,
                "stop" => m.stop = true,
                "prevent" => m.prevent = true,
                _ => {}
            }
        }
        m
    }
}
