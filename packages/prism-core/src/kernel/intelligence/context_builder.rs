//! `intelligence::context_builder` — shape graph data into an
//! [`ObjectContext`] for AI prompts.

use serde_json::Value as JsonValue;

use super::types::{
    AiMessage, ContextBuilderOptions, ContextCollection, ContextEdge, ContextEntry, ObjectContext,
};

/// Builder parameters for [`ContextBuilder::build`].
#[derive(Debug, Clone, Default)]
pub struct ContextBuildParams {
    pub object: JsonValue,
    pub object_type: String,
    pub ancestors: Vec<ContextEntry>,
    pub children: Vec<ContextEntry>,
    pub edges: Vec<ContextEdge>,
    pub collection: Option<ContextCollection>,
}

/// Pure shaper from raw graph data into [`ObjectContext`] + a system
/// message renderer.
pub struct ContextBuilder {
    options: ContextBuilderOptions,
}

impl ContextBuilder {
    pub fn new(options: ContextBuilderOptions) -> Self {
        Self { options }
    }

    /// Build an [`ObjectContext`] from raw graph data. Truncates
    /// ancestors / children / edges to the configured caps.
    pub fn build(&self, params: ContextBuildParams) -> ObjectContext {
        let ContextBuildParams {
            object,
            object_type,
            mut ancestors,
            mut children,
            mut edges,
            collection,
        } = params;
        ancestors.truncate(self.options.max_ancestor_depth);
        children.truncate(self.options.max_children);
        edges.truncate(self.options.max_edges);
        ObjectContext {
            object,
            object_type,
            ancestors,
            children,
            edges,
            collection,
        }
    }

    /// Format an [`ObjectContext`] as a system-role [`AiMessage`],
    /// matching the legacy TS formatting.
    pub fn to_system_message(ctx: &ObjectContext) -> AiMessage {
        let mut parts: Vec<String> = Vec::new();
        parts.push(format!(
            "You are assisting with a \"{}\" object.",
            ctx.object_type
        ));
        if let Some(coll) = &ctx.collection {
            parts.push(format!("Collection: \"{}\" ({})", coll.name, coll.id));
        }
        if !ctx.ancestors.is_empty() {
            let path = ctx
                .ancestors
                .iter()
                .map(|a| a.name.clone())
                .collect::<Vec<_>>()
                .join(" → ");
            parts.push(format!("Path: {path}"));
        }
        if !ctx.children.is_empty() {
            let list = ctx
                .children
                .iter()
                .map(|c| format!("{} [{}]", c.name, c.type_name))
                .collect::<Vec<_>>()
                .join(", ");
            parts.push(format!("Children ({}): {list}", ctx.children.len()));
        }
        if !ctx.edges.is_empty() {
            let list = ctx
                .edges
                .iter()
                .map(|e| format!("→ {}:{}", e.target_type, e.target_id))
                .collect::<Vec<_>>()
                .join(", ");
            parts.push(format!("Connections ({}): {list}", ctx.edges.len()));
        }
        let data =
            serde_json::to_string_pretty(&ctx.object).unwrap_or_else(|_| "<unserialisable>".into());
        parts.push(format!("\nObject data:\n{data}"));

        AiMessage::system(parts.join("\n"))
    }
}

impl Default for ContextBuilder {
    fn default() -> Self {
        Self::new(ContextBuilderOptions::default())
    }
}
