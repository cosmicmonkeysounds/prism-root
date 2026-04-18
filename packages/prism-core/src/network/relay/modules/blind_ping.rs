//! Blind ping — content-free push notifications.

use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::network::relay::module_system::{capabilities, RelayContext, RelayModule};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlindPing {
    pub to: String,
    pub sent_at: String,
    pub badge_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceRegistration {
    pub did: String,
    pub platform: PingPlatform,
    pub token: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PingPlatform {
    Apns,
    Fcm,
}

pub trait PingTransport: Send + Sync {
    fn send(&self, ping: &BlindPing, registration: &DeviceRegistration) -> bool;
}

pub struct MemoryPingTransport {
    sent: RwLock<Vec<BlindPing>>,
}

impl MemoryPingTransport {
    pub fn new() -> Self {
        Self {
            sent: RwLock::new(Vec::new()),
        }
    }
    pub fn sent(&self) -> Vec<BlindPing> {
        self.sent.read().unwrap().clone()
    }
}

impl Default for MemoryPingTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl PingTransport for MemoryPingTransport {
    fn send(&self, ping: &BlindPing, _reg: &DeviceRegistration) -> bool {
        self.sent.write().unwrap().push(ping.clone());
        true
    }
}

pub struct BlindPinger {
    registrations: RwLock<Vec<DeviceRegistration>>,
    transport: RwLock<Option<Box<dyn PingTransport>>>,
}

impl BlindPinger {
    pub fn new() -> Self {
        Self {
            registrations: RwLock::new(Vec::new()),
            transport: RwLock::new(None),
        }
    }

    pub fn set_transport(&self, transport: Box<dyn PingTransport>) {
        *self.transport.write().unwrap() = Some(transport);
    }

    pub fn register(&self, reg: DeviceRegistration) {
        self.registrations.write().unwrap().push(reg);
    }

    pub fn unregister(&self, did: &str) {
        self.registrations.write().unwrap().retain(|r| r.did != did);
    }

    pub fn devices(&self) -> Vec<DeviceRegistration> {
        self.registrations.read().unwrap().clone()
    }

    pub fn ping(&self, recipient_did: &str, badge_count: Option<u32>, now_iso: &str) -> bool {
        let ping = BlindPing {
            to: recipient_did.to_string(),
            sent_at: now_iso.to_string(),
            badge_count,
        };
        let regs = self.registrations.read().unwrap();
        let targets: Vec<_> = regs
            .iter()
            .filter(|r| r.did == recipient_did)
            .cloned()
            .collect();
        drop(regs);

        let transport = self.transport.read().unwrap();
        if let Some(ref t) = *transport {
            targets.iter().all(|r| t.send(&ping, r))
        } else {
            false
        }
    }

    pub fn wake(&self, did: &str, badge_count: Option<u32>, now_iso: &str) -> bool {
        self.ping(did, badge_count, now_iso)
    }
}

impl Default for BlindPinger {
    fn default() -> Self {
        Self::new()
    }
}

pub struct BlindPingModule;

impl RelayModule for BlindPingModule {
    fn name(&self) -> &str {
        "blind-ping"
    }
    fn description(&self) -> &str {
        "Content-free push notifications"
    }
    fn install(&self, ctx: &RelayContext) {
        ctx.set_capability(capabilities::PINGER, BlindPinger::new());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn register_and_ping() {
        let pinger = BlindPinger::new();
        let _transport = Arc::new(MemoryPingTransport::new());
        pinger.set_transport(Box::new(MemoryPingTransport::new()));
        pinger.register(DeviceRegistration {
            did: "alice".into(),
            platform: PingPlatform::Apns,
            token: "tok-1".into(),
        });
        assert_eq!(pinger.devices().len(), 1);

        pinger.unregister("alice");
        assert!(pinger.devices().is_empty());
    }
}
