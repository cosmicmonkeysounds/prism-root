//! Component type registry. One-way DI surface — panels register
//! their component types at boot, the document tree looks them up by
//! id when rendering. Phase 3 bolts the field-factory layer on top.

use indexmap::IndexMap;
use std::sync::Arc;
use thiserror::Error;

use crate::component::{Component, ComponentId};

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("component already registered: {0}")]
    AlreadyRegistered(ComponentId),
    #[error("component not found: {0}")]
    NotFound(ComponentId),
}

#[derive(Default)]
pub struct ComponentRegistry {
    components: IndexMap<ComponentId, Arc<dyn Component>>,
}

impl ComponentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, component: Arc<dyn Component>) -> Result<(), RegistryError> {
        let id = component.id().clone();
        if self.components.contains_key(&id) {
            return Err(RegistryError::AlreadyRegistered(id));
        }
        self.components.insert(id, component);
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn Component>> {
        self.components.get(id).cloned()
    }

    pub fn len(&self) -> usize {
        self.components.len()
    }

    pub fn is_empty(&self) -> bool {
        self.components.is_empty()
    }

    pub fn ids(&self) -> impl Iterator<Item = &ComponentId> {
        self.components.keys()
    }
}
