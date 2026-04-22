use std::collections::HashMap;

use prism_builder::{BuilderDocument, Node, NodeId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub node_id: NodeId,
    pub component: String,
    pub field: String,
    pub snippet: String,
    pub score: f64,
}

pub struct SearchIndex {
    docs: Vec<IndexedDoc>,
    idf: HashMap<String, f64>,
}

struct IndexedDoc {
    node_id: NodeId,
    component: String,
    field: String,
    text: String,
    terms: HashMap<String, f64>,
}

impl SearchIndex {
    pub fn new() -> Self {
        Self {
            docs: Vec::new(),
            idf: HashMap::new(),
        }
    }

    pub fn build(document: &BuilderDocument) -> Self {
        let mut index = Self::new();
        if let Some(ref root) = document.root {
            index.index_node(root);
        }
        index.compute_idf();
        index
    }

    fn index_node(&mut self, node: &Node) {
        if let Some(obj) = node.props.as_object() {
            for (field, value) in obj {
                let text = match value {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    _ => continue,
                };
                if text.is_empty() {
                    continue;
                }
                let terms = tokenize(&text);
                let mut tf = HashMap::new();
                let total = terms.len() as f64;
                if total == 0.0 {
                    continue;
                }
                for term in &terms {
                    *tf.entry(term.clone()).or_insert(0.0) += 1.0;
                }
                for v in tf.values_mut() {
                    *v /= total;
                }
                self.docs.push(IndexedDoc {
                    node_id: node.id.clone(),
                    component: node.component.clone(),
                    field: field.clone(),
                    text: text.clone(),
                    terms: tf,
                });
            }
        }
        for child in &node.children {
            self.index_node(child);
        }
    }

    fn compute_idf(&mut self) {
        let n = self.docs.len() as f64;
        if n == 0.0 {
            return;
        }
        let mut df: HashMap<String, usize> = HashMap::new();
        for doc in &self.docs {
            for term in doc.terms.keys() {
                *df.entry(term.clone()).or_insert(0) += 1;
            }
        }
        for (term, count) in &df {
            self.idf
                .insert(term.clone(), (n / (*count as f64)).ln() + 1.0);
        }
    }

    pub fn query(&self, query_str: &str) -> Vec<SearchResult> {
        let query_terms = tokenize(query_str);
        if query_terms.is_empty() {
            return Vec::new();
        }
        let mut results: Vec<SearchResult> = self
            .docs
            .iter()
            .filter_map(|doc| {
                let mut score = 0.0;
                for qt in &query_terms {
                    if let Some(tf) = doc.terms.get(qt) {
                        let idf = self.idf.get(qt).copied().unwrap_or(1.0);
                        score += tf * idf;
                    }
                }
                if score > 0.0 {
                    Some(SearchResult {
                        node_id: doc.node_id.clone(),
                        component: doc.component.clone(),
                        field: doc.field.clone(),
                        snippet: snippet(&doc.text, 80),
                        score,
                    })
                } else {
                    None
                }
            })
            .collect();
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results
    }
}

impl Default for SearchIndex {
    fn default() -> Self {
        Self::new()
    }
}

fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty() && s.len() >= 2)
        .map(String::from)
        .collect()
}

fn snippet(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        let mut end = max_len;
        while end > 0 && !text.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &text[..end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_doc() -> BuilderDocument {
        BuilderDocument {
            root: Some(Node {
                id: "root".into(),
                component: "container".into(),
                props: json!({ "spacing": 16 }),
                children: vec![
                    Node {
                        id: "hero".into(),
                        component: "text".into(),
                        props: json!({ "body": "Welcome to Prism", "level": "h1" }),
                        children: vec![],
                        ..Default::default()
                    },
                    Node {
                        id: "intro".into(),
                        component: "text".into(),
                        props: json!({
                            "body": "The distributed visual operating system for creative work."
                        }),
                        children: vec![],
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn build_indexes_all_text_fields() {
        let idx = SearchIndex::build(&sample_doc());
        assert!(idx.docs.len() >= 3);
    }

    #[test]
    fn query_finds_matching_nodes() {
        let idx = SearchIndex::build(&sample_doc());
        let results = idx.query("prism");
        assert!(!results.is_empty());
        assert_eq!(results[0].node_id, "hero");
    }

    #[test]
    fn query_ranks_by_relevance() {
        let idx = SearchIndex::build(&sample_doc());
        let results = idx.query("distributed visual operating");
        assert!(!results.is_empty());
        assert_eq!(results[0].node_id, "intro");
    }

    #[test]
    fn query_empty_returns_nothing() {
        let idx = SearchIndex::build(&sample_doc());
        assert!(idx.query("").is_empty());
    }

    #[test]
    fn query_no_match_returns_empty() {
        let idx = SearchIndex::build(&sample_doc());
        assert!(idx.query("xyznonexistent").is_empty());
    }

    #[test]
    fn snippet_truncates_long_text() {
        let long = "a".repeat(200);
        let snip = snippet(&long, 80);
        assert!(snip.len() <= 84);
        assert!(snip.ends_with('…'));
    }

    #[test]
    fn snippet_preserves_short_text() {
        assert_eq!(snippet("hello", 80), "hello");
    }

    #[test]
    fn tokenize_splits_and_lowercases() {
        let tokens = tokenize("Hello World, foo-bar!");
        assert!(tokens.contains(&"hello".to_string()));
        assert!(tokens.contains(&"world".to_string()));
        assert!(tokens.contains(&"foo".to_string()));
        assert!(tokens.contains(&"bar".to_string()));
    }

    #[test]
    fn tokenize_skips_short_tokens() {
        let tokens = tokenize("I am a test");
        assert!(!tokens.contains(&"i".to_string()));
        assert!(!tokens.contains(&"a".to_string()));
        assert!(tokens.contains(&"am".to_string()));
        assert!(tokens.contains(&"test".to_string()));
    }
}
