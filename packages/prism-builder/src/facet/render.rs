//! `FacetComponent` — facet block Slint emitter.

use prism_core::help::HelpEntry;

use crate::component::{Component, ComponentId};
use crate::registry::FieldSpec;
use crate::signal::{common_signals, SignalDef};

pub struct FacetComponent {
    pub id: ComponentId,
}

impl FacetComponent {
    pub fn new() -> Self {
        Self { id: "facet".into() }
    }
}

impl Default for FacetComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for FacetComponent {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn schema(&self) -> Vec<FieldSpec> {
        crate::schemas::facet()
    }

    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "builder.components.facet",
            "Facet",
            "Programmatic list: expands a prefab template once per item in a data source.",
        ))
    }

    fn signals(&self) -> Vec<SignalDef> {
        common_signals()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────
