//! `intelligence::registry` — [`AiProviderRegistry`].

use std::collections::HashMap;
use std::sync::Arc;

use super::types::{
    AiCompletion, AiCompletionRequest, AiError, AiProvider, InlineCompletion,
    InlineCompletionRequest,
};

/// Registry of AI providers with a single active default. Mirrors the
/// legacy TS `AiProviderRegistry`.
pub struct AiProviderRegistry {
    providers: HashMap<String, Arc<dyn AiProvider>>,
    order: Vec<String>,
    active: Option<String>,
}

impl AiProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
            order: Vec::new(),
            active: None,
        }
    }

    /// Register `provider`. The first registered provider becomes the
    /// default active one.
    pub fn register(&mut self, provider: Arc<dyn AiProvider>) {
        let name = provider.name().to_string();
        if !self.providers.contains_key(&name) {
            self.order.push(name.clone());
        }
        self.providers.insert(name.clone(), provider);
        if self.active.is_none() {
            self.active = Some(name);
        }
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn AiProvider>> {
        self.providers.get(name).cloned()
    }

    pub fn list(&self) -> Vec<String> {
        self.order.clone()
    }

    /// Currently active provider, if any.
    pub fn active(&self) -> Option<Arc<dyn AiProvider>> {
        self.active.as_ref().and_then(|name| self.get(name))
    }

    pub fn set_active(&mut self, name: &str) -> Result<(), AiError> {
        if !self.providers.contains_key(name) {
            return Err(AiError::NotRegistered(name.into()));
        }
        self.active = Some(name.to_string());
        Ok(())
    }

    /// Complete using the active provider.
    pub fn complete(&self, request: &AiCompletionRequest) -> Result<AiCompletion, AiError> {
        let provider = self.active().ok_or(AiError::NoActiveProvider)?;
        provider.complete(request)
    }

    /// Inline-complete using the active provider.
    pub fn complete_inline(
        &self,
        request: &InlineCompletionRequest,
    ) -> Result<InlineCompletion, AiError> {
        let provider = self.active().ok_or(AiError::NoActiveProvider)?;
        provider.complete_inline(request)
    }
}

impl Default for AiProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}
