//! Webhooks — outgoing HTTP callbacks on CRDT changes.

use std::collections::HashMap;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::network::relay::module_system::{capabilities, RelayContext, RelayModule};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebhookConfig {
    pub id: String,
    pub url: String,
    pub events: Vec<String>,
    pub secret: Option<String>,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebhookDelivery {
    pub webhook_id: String,
    pub event: String,
    pub timestamp: String,
    pub success: bool,
    pub status_code: Option<u16>,
    pub error: Option<String>,
}

pub trait WebhookHttpClient: Send + Sync {
    fn post(&self, url: &str, body: &str, headers: &[(&str, &str)]) -> Result<u16, String>;
}

pub struct NoopHttpClient;
impl WebhookHttpClient for NoopHttpClient {
    fn post(&self, _url: &str, _body: &str, _headers: &[(&str, &str)]) -> Result<u16, String> {
        Ok(200)
    }
}

pub struct WebhookEmitter {
    configs: RwLock<HashMap<String, WebhookConfig>>,
    deliveries: RwLock<HashMap<String, Vec<WebhookDelivery>>>,
    http_client: Box<dyn WebhookHttpClient>,
    next_id: RwLock<u64>,
}

impl WebhookEmitter {
    pub fn new(http_client: Box<dyn WebhookHttpClient>) -> Self {
        Self {
            configs: RwLock::new(HashMap::new()),
            deliveries: RwLock::new(HashMap::new()),
            http_client,
            next_id: RwLock::new(1),
        }
    }

    pub fn register(
        &self,
        url: &str,
        events: Vec<String>,
        secret: Option<String>,
    ) -> WebhookConfig {
        let mut id_gen = self.next_id.write().unwrap();
        let id = format!("wh-{}", *id_gen);
        *id_gen += 1;
        let config = WebhookConfig {
            id: id.clone(),
            url: url.to_string(),
            events,
            secret,
            active: true,
        };
        self.configs.write().unwrap().insert(id, config.clone());
        config
    }

    pub fn unregister(&self, webhook_id: &str) -> bool {
        self.configs.write().unwrap().remove(webhook_id).is_some()
    }

    pub fn list(&self) -> Vec<WebhookConfig> {
        self.configs.read().unwrap().values().cloned().collect()
    }

    pub fn get(&self, id: &str) -> Option<WebhookConfig> {
        self.configs.read().unwrap().get(id).cloned()
    }

    pub fn emit(&self, event: &str, data: &str, now_iso: &str) -> Vec<WebhookDelivery> {
        let configs: Vec<WebhookConfig> = self
            .configs
            .read()
            .unwrap()
            .values()
            .filter(|c| c.active && c.events.iter().any(|e| e == event || e == "*"))
            .cloned()
            .collect();

        let mut results = Vec::new();
        for config in &configs {
            let mut headers = vec![("Content-Type", "application/json")];
            let sig;
            if let Some(ref secret) = config.secret {
                sig = hmac_sign(secret, data);
                headers.push(("X-Prism-Signature", &sig));
            }
            let delivery = match self.http_client.post(&config.url, data, &headers) {
                Ok(status) => WebhookDelivery {
                    webhook_id: config.id.clone(),
                    event: event.to_string(),
                    timestamp: now_iso.to_string(),
                    success: (200..300).contains(&status),
                    status_code: Some(status),
                    error: None,
                },
                Err(e) => WebhookDelivery {
                    webhook_id: config.id.clone(),
                    event: event.to_string(),
                    timestamp: now_iso.to_string(),
                    success: false,
                    status_code: None,
                    error: Some(e),
                },
            };
            self.deliveries
                .write()
                .unwrap()
                .entry(config.id.clone())
                .or_default()
                .push(delivery.clone());
            results.push(delivery);
        }
        results
    }

    pub fn deliveries(&self, webhook_id: &str) -> Vec<WebhookDelivery> {
        self.deliveries
            .read()
            .unwrap()
            .get(webhook_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn restore(&self, configs: Vec<WebhookConfig>) {
        let mut store = self.configs.write().unwrap();
        for c in configs {
            store.insert(c.id.clone(), c);
        }
    }
}

fn hmac_sign(secret: &str, data: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(data.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

pub struct WebhookModule;

impl RelayModule for WebhookModule {
    fn name(&self) -> &str {
        "webhooks"
    }
    fn description(&self) -> &str {
        "Outgoing HTTP webhooks on CRDT changes"
    }
    fn install(&self, ctx: &RelayContext) {
        ctx.set_capability(
            capabilities::WEBHOOKS,
            WebhookEmitter::new(Box::new(NoopHttpClient)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_emit() {
        let emitter = WebhookEmitter::new(Box::new(NoopHttpClient));
        let wh = emitter.register(
            "https://example.com/hook",
            vec!["object.created".into()],
            None,
        );
        assert_eq!(emitter.list().len(), 1);

        let deliveries = emitter.emit("object.created", r#"{"id":"1"}"#, "2026-04-18T00:00:00Z");
        assert_eq!(deliveries.len(), 1);
        assert!(deliveries[0].success);

        let history = emitter.deliveries(&wh.id);
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn unregister_stops_delivery() {
        let emitter = WebhookEmitter::new(Box::new(NoopHttpClient));
        let wh = emitter.register("https://example.com/hook", vec!["*".into()], None);
        emitter.unregister(&wh.id);
        let deliveries = emitter.emit("object.created", "{}", "2026-04-18T00:00:00Z");
        assert!(deliveries.is_empty());
    }
}
