//! Sample Flux objects for dev/demo mode.
//!
//! Provides 50+ pre-built `GraphObject` seeds across all Flux entity
//! types. Used by the shell's demo mode and tests.

use indexmap::IndexMap;
use serde_json::{json, Value};

pub struct SeedObject {
    pub id: String,
    pub object_type: String,
    pub data: IndexMap<String, Value>,
}

impl SeedObject {
    fn new(id: &str, object_type: &str) -> Self {
        Self {
            id: id.into(),
            object_type: object_type.into(),
            data: IndexMap::new(),
        }
    }

    fn field(mut self, key: &str, value: Value) -> Self {
        self.data.insert(key.into(), value);
        self
    }
}

pub struct SeedEdge {
    pub id: String,
    pub edge_type: String,
    pub source_id: String,
    pub target_id: String,
}

pub struct SeedData {
    pub objects: Vec<SeedObject>,
    pub edges: Vec<SeedEdge>,
}

pub fn generate_seed_data() -> SeedData {
    let mut objects = Vec::new();
    let mut edges = Vec::new();

    // ── Projects ────────────────────────────────────────────────
    objects.push(
        SeedObject::new("proj-1", "flux:project")
            .field("name", json!("Website Redesign"))
            .field("status", json!("active"))
            .field(
                "description",
                json!("Complete overhaul of the marketing website"),
            )
            .field("startDate", json!("2026-04-01"))
            .field("endDate", json!("2026-06-30")),
    );
    objects.push(
        SeedObject::new("proj-2", "flux:project")
            .field("name", json!("Mobile App v2"))
            .field("status", json!("planning"))
            .field("description", json!("Next major release of the mobile app"))
            .field("startDate", json!("2026-05-15"))
            .field("endDate", json!("2026-09-01")),
    );
    objects.push(
        SeedObject::new("proj-3", "flux:project")
            .field("name", json!("Q3 Marketing Campaign"))
            .field("status", json!("planning"))
            .field("description", json!("Integrated marketing push for Q3")),
    );

    // ── Tasks ───────────────────────────────────────────────────
    let tasks = [
        (
            "task-1",
            "Design landing page",
            "in_progress",
            "proj-1",
            "2026-05-05",
        ),
        (
            "task-2",
            "Set up CI/CD pipeline",
            "done",
            "proj-1",
            "2026-04-15",
        ),
        (
            "task-3",
            "Write API documentation",
            "todo",
            "proj-1",
            "2026-05-10",
        ),
        (
            "task-4",
            "User testing round 1",
            "todo",
            "proj-1",
            "2026-05-20",
        ),
        (
            "task-5",
            "Fix navigation bug",
            "in_progress",
            "proj-1",
            "2026-05-03",
        ),
        (
            "task-6",
            "Create wireframes",
            "done",
            "proj-2",
            "2026-05-01",
        ),
        (
            "task-7",
            "Backend API design",
            "in_progress",
            "proj-2",
            "2026-05-10",
        ),
        (
            "task-8",
            "Set up test environment",
            "todo",
            "proj-2",
            "2026-05-15",
        ),
        (
            "task-9",
            "Design email templates",
            "todo",
            "proj-3",
            "2026-05-20",
        ),
        (
            "task-10",
            "Content calendar",
            "backlog",
            "proj-3",
            "2026-06-01",
        ),
        (
            "task-11",
            "Social media audit",
            "todo",
            "proj-3",
            "2026-05-25",
        ),
        (
            "task-12",
            "Performance optimization",
            "review",
            "proj-1",
            "2026-05-08",
        ),
        (
            "task-13",
            "Database migration plan",
            "backlog",
            "proj-2",
            "2026-06-01",
        ),
        (
            "task-14",
            "Analytics dashboard",
            "todo",
            "proj-1",
            "2026-05-15",
        ),
        ("task-15", "Security audit", "todo", "proj-2", "2026-06-10"),
    ];
    for (id, name, status, proj, due) in tasks {
        objects.push(
            SeedObject::new(id, "flux:task")
                .field("name", json!(name))
                .field("status", json!(status))
                .field("date", json!(due))
                .field(
                    "priority",
                    json!(if status == "in_progress" { 2 } else { 1 }),
                ),
        );
        edges.push(SeedEdge {
            id: format!("edge-{id}-{proj}"),
            edge_type: "flux:belongs-to".into(),
            source_id: id.into(),
            target_id: proj.into(),
        });
    }

    // Task dependencies
    edges.push(SeedEdge {
        id: "edge-dep-1".into(),
        edge_type: "flux:depends-on".into(),
        source_id: "task-4".into(),
        target_id: "task-1".into(),
    });
    edges.push(SeedEdge {
        id: "edge-dep-2".into(),
        edge_type: "flux:depends-on".into(),
        source_id: "task-3".into(),
        target_id: "task-2".into(),
    });

    // ── Goals ───────────────────────────────────────────────────
    objects.push(
        SeedObject::new("goal-1", "flux:goal")
            .field("name", json!("Launch new website"))
            .field("status", json!("active"))
            .field("targetDate", json!("2026-06-30"))
            .field("progress", json!(35)),
    );
    objects.push(
        SeedObject::new("goal-2", "flux:goal")
            .field("name", json!("Increase monthly active users by 50%"))
            .field("status", json!("active"))
            .field("targetDate", json!("2026-12-31"))
            .field("progress", json!(15)),
    );
    objects.push(
        SeedObject::new("goal-3", "flux:goal")
            .field("name", json!("Complete fitness challenge"))
            .field("status", json!("active"))
            .field("targetDate", json!("2026-07-31"))
            .field("progress", json!(60)),
    );

    // ── Contacts ────────────────────────────────────────────────
    let contacts = [
        (
            "contact-1",
            "Alice Chen",
            "alice@example.com",
            "Engineering",
        ),
        ("contact-2", "Bob Martinez", "bob@example.com", "Design"),
        (
            "contact-3",
            "Carol Williams",
            "carol@example.com",
            "Marketing",
        ),
        ("contact-4", "David Kim", "david@example.com", "Product"),
        ("contact-5", "Eva Thompson", "eva@example.com", "Sales"),
        ("contact-6", "Frank Patel", "frank@example.com", "Support"),
        ("contact-7", "Grace Liu", "grace@example.com", "Engineering"),
        ("contact-8", "Henry Johnson", "henry@example.com", "Design"),
    ];
    for (id, name, email, dept) in contacts {
        objects.push(
            SeedObject::new(id, "flux:contact")
                .field("name", json!(name))
                .field("email", json!(email))
                .field("department", json!(dept)),
        );
    }

    // Task assignments
    edges.push(SeedEdge {
        id: "edge-assign-1".into(),
        edge_type: "flux:assigned-to".into(),
        source_id: "task-1".into(),
        target_id: "contact-2".into(),
    });
    edges.push(SeedEdge {
        id: "edge-assign-2".into(),
        edge_type: "flux:assigned-to".into(),
        source_id: "task-5".into(),
        target_id: "contact-1".into(),
    });
    edges.push(SeedEdge {
        id: "edge-assign-3".into(),
        edge_type: "flux:assigned-to".into(),
        source_id: "task-7".into(),
        target_id: "contact-7".into(),
    });

    // ── Organizations ───────────────────────────────────────────
    objects.push(
        SeedObject::new("org-1", "flux:organization")
            .field("name", json!("Acme Corp"))
            .field("industry", json!("Technology"))
            .field("size", json!("50-200")),
    );
    objects.push(
        SeedObject::new("org-2", "flux:organization")
            .field("name", json!("Globex Industries"))
            .field("industry", json!("Manufacturing"))
            .field("size", json!("200-1000")),
    );

    // ── Invoices ────────────────────────────────────────────────
    objects.push(
        SeedObject::new("inv-1", "flux:invoice")
            .field("name", json!("INV-2026-001"))
            .field("status", json!("sent"))
            .field("amount", json!(450_000))
            .field("currency", json!("USD"))
            .field("date", json!("2026-04-15"))
            .field("dueDate", json!("2026-05-15")),
    );
    objects.push(
        SeedObject::new("inv-2", "flux:invoice")
            .field("name", json!("INV-2026-002"))
            .field("status", json!("paid"))
            .field("amount", json!(1_200_000))
            .field("currency", json!("USD"))
            .field("date", json!("2026-03-01"))
            .field("dueDate", json!("2026-04-01")),
    );
    objects.push(
        SeedObject::new("inv-3", "flux:invoice")
            .field("name", json!("INV-2026-003"))
            .field("status", json!("overdue"))
            .field("amount", json!(275_000))
            .field("currency", json!("USD"))
            .field("date", json!("2026-03-15"))
            .field("dueDate", json!("2026-04-15")),
    );

    // Invoice→org edges
    edges.push(SeedEdge {
        id: "edge-inv-1".into(),
        edge_type: "flux:invoiced-to".into(),
        source_id: "inv-1".into(),
        target_id: "org-1".into(),
    });
    edges.push(SeedEdge {
        id: "edge-inv-2".into(),
        edge_type: "flux:invoiced-to".into(),
        source_id: "inv-2".into(),
        target_id: "org-1".into(),
    });
    edges.push(SeedEdge {
        id: "edge-inv-3".into(),
        edge_type: "flux:invoiced-to".into(),
        source_id: "inv-3".into(),
        target_id: "org-2".into(),
    });

    // ── Transactions ────────────────────────────────────────────
    let transactions = [
        (
            "txn-1",
            "Client payment - Acme",
            "income",
            450_000,
            "2026-04-20",
        ),
        ("txn-2", "Office supplies", "expense", 35_000, "2026-04-18"),
        (
            "txn-3",
            "Software subscription",
            "expense",
            9_900,
            "2026-05-01",
        ),
        (
            "txn-4",
            "Freelancer payment",
            "expense",
            200_000,
            "2026-04-25",
        ),
        (
            "txn-5",
            "Client payment - Globex",
            "income",
            1_200_000,
            "2026-04-01",
        ),
        ("txn-6", "Cloud hosting", "expense", 45_000, "2026-05-01"),
        (
            "txn-7",
            "Conference registration",
            "expense",
            79_900,
            "2026-04-10",
        ),
        (
            "txn-8",
            "Consulting revenue",
            "income",
            650_000,
            "2026-04-28",
        ),
    ];
    for (id, name, txn_type, amount, date) in transactions {
        objects.push(
            SeedObject::new(id, "flux:transaction")
                .field("name", json!(name))
                .field("type", json!(txn_type))
                .field("amount", json!(amount))
                .field("currency", json!("USD"))
                .field("date", json!(date)),
        );
    }

    // ── Items (inventory) ───────────────────────────────────────
    objects.push(
        SeedObject::new("item-1", "flux:item")
            .field("name", json!("Laptop - MacBook Pro 16\""))
            .field("status", json!("in_stock"))
            .field("quantity", json!(5))
            .field("sku", json!("MBP-16-2026")),
    );
    objects.push(
        SeedObject::new("item-2", "flux:item")
            .field("name", json!("Monitor - 4K 27\""))
            .field("status", json!("in_stock"))
            .field("quantity", json!(12))
            .field("sku", json!("MON-4K-27")),
    );
    objects.push(
        SeedObject::new("item-3", "flux:item")
            .field("name", json!("Standing Desk"))
            .field("status", json!("low_stock"))
            .field("quantity", json!(2))
            .field("sku", json!("DESK-STAND")),
    );

    // ── Locations ───────────────────────────────────────────────
    objects.push(
        SeedObject::new("loc-1", "flux:location")
            .field("name", json!("Main Office"))
            .field("address", json!("123 Tech Blvd, San Francisco, CA")),
    );
    objects.push(
        SeedObject::new("loc-2", "flux:location")
            .field("name", json!("Warehouse"))
            .field("address", json!("456 Storage Lane, Oakland, CA")),
    );

    // Item→location edges
    edges.push(SeedEdge {
        id: "edge-stored-1".into(),
        edge_type: "flux:stored-at".into(),
        source_id: "item-1".into(),
        target_id: "loc-1".into(),
    });
    edges.push(SeedEdge {
        id: "edge-stored-2".into(),
        edge_type: "flux:stored-at".into(),
        source_id: "item-2".into(),
        target_id: "loc-1".into(),
    });
    edges.push(SeedEdge {
        id: "edge-stored-3".into(),
        edge_type: "flux:stored-at".into(),
        source_id: "item-3".into(),
        target_id: "loc-2".into(),
    });

    // ── Accounts ────────────────────────────────────────────────
    objects.push(
        SeedObject::new("acct-1", "flux:account")
            .field("name", json!("Operating Account"))
            .field("balance", json!(4_500_000))
            .field("currency", json!("USD")),
    );
    objects.push(
        SeedObject::new("acct-2", "flux:account")
            .field("name", json!("Savings Account"))
            .field("balance", json!(12_000_000))
            .field("currency", json!("USD")),
    );

    // ── Milestones ──────────────────────────────────────────────
    objects.push(
        SeedObject::new("ms-1", "flux:milestone")
            .field("name", json!("Design complete"))
            .field("date", json!("2026-05-15"))
            .field("completed", json!(false)),
    );
    objects.push(
        SeedObject::new("ms-2", "flux:milestone")
            .field("name", json!("Beta launch"))
            .field("date", json!("2026-06-15"))
            .field("completed", json!(false)),
    );
    objects.push(
        SeedObject::new("ms-3", "flux:milestone")
            .field("name", json!("Public launch"))
            .field("date", json!("2026-06-30"))
            .field("completed", json!(false)),
    );

    // Milestone→project edges
    edges.push(SeedEdge {
        id: "edge-ms-1".into(),
        edge_type: "flux:belongs-to".into(),
        source_id: "ms-1".into(),
        target_id: "proj-1".into(),
    });
    edges.push(SeedEdge {
        id: "edge-ms-2".into(),
        edge_type: "flux:belongs-to".into(),
        source_id: "ms-2".into(),
        target_id: "proj-1".into(),
    });
    edges.push(SeedEdge {
        id: "edge-ms-3".into(),
        edge_type: "flux:belongs-to".into(),
        source_id: "ms-3".into(),
        target_id: "proj-1".into(),
    });

    SeedData { objects, edges }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_data_has_objects() {
        let data = generate_seed_data();
        assert!(data.objects.len() >= 50);
    }

    #[test]
    fn seed_data_has_edges() {
        let data = generate_seed_data();
        assert!(data.edges.len() >= 15);
    }

    #[test]
    fn all_objects_have_name() {
        let data = generate_seed_data();
        for obj in &data.objects {
            assert!(
                obj.data.contains_key("name"),
                "object {} ({}) missing name",
                obj.id,
                obj.object_type
            );
        }
    }

    #[test]
    fn all_objects_have_valid_type() {
        let data = generate_seed_data();
        for obj in &data.objects {
            assert!(
                obj.object_type.starts_with("flux:"),
                "object {} has non-flux type: {}",
                obj.id,
                obj.object_type
            );
        }
    }

    #[test]
    fn unique_ids() {
        let data = generate_seed_data();
        let mut ids: Vec<&str> = data.objects.iter().map(|o| o.id.as_str()).collect();
        let original_len = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), original_len, "duplicate object IDs found");
    }

    #[test]
    fn edge_references_exist() {
        let data = generate_seed_data();
        let ids: Vec<&str> = data.objects.iter().map(|o| o.id.as_str()).collect();
        for edge in &data.edges {
            assert!(
                ids.contains(&edge.source_id.as_str()),
                "edge {} references missing source {}",
                edge.id,
                edge.source_id
            );
            assert!(
                ids.contains(&edge.target_id.as_str()),
                "edge {} references missing target {}",
                edge.id,
                edge.target_id
            );
        }
    }

    #[test]
    fn has_all_entity_types() {
        let data = generate_seed_data();
        let types: Vec<&str> = data
            .objects
            .iter()
            .map(|o| o.object_type.as_str())
            .collect();
        assert!(types.contains(&"flux:project"));
        assert!(types.contains(&"flux:task"));
        assert!(types.contains(&"flux:goal"));
        assert!(types.contains(&"flux:contact"));
        assert!(types.contains(&"flux:organization"));
        assert!(types.contains(&"flux:invoice"));
        assert!(types.contains(&"flux:transaction"));
        assert!(types.contains(&"flux:item"));
        assert!(types.contains(&"flux:location"));
        assert!(types.contains(&"flux:account"));
        assert!(types.contains(&"flux:milestone"));
    }
}
