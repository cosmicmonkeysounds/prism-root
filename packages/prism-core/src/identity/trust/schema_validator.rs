//! Poison-pill schema validator — checks imported JSON against a set
//! of composable rules (depth, string size, array size, key count,
//! disallowed keys). Port of `createSchemaValidator` in
//! `trust/trust.ts`.

use regex::Regex;
use serde_json::Value;

use super::types::{
    SchemaValidationIssue, SchemaValidationResult, SchemaValidationSeverity, SchemaValidatorOptions,
};

/// One validation rule. Each rule owns its scan and returns a list of
/// issues it found under the supplied root path.
pub trait SchemaValidationRule: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> String;
    fn check(&self, data: &Value, path: &str) -> Vec<SchemaValidationIssue>;
}

/// Composite validator, built via [`create_schema_validator`]. New
/// rules can be bolted on after construction with [`Self::add_rule`].
pub struct SchemaValidator {
    rules: Vec<Box<dyn SchemaValidationRule>>,
}

impl std::fmt::Debug for SchemaValidator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SchemaValidator")
            .field("rules", &self.rule_names())
            .finish()
    }
}

impl SchemaValidator {
    pub fn validate(&self, data: &Value) -> SchemaValidationResult {
        let mut all_issues = Vec::new();
        for rule in &self.rules {
            all_issues.extend(rule.check(data, "$"));
        }
        let valid = !all_issues
            .iter()
            .any(|i| matches!(i.severity, SchemaValidationSeverity::Error));
        SchemaValidationResult {
            valid,
            issues: all_issues,
        }
    }

    pub fn add_rule(&mut self, rule: Box<dyn SchemaValidationRule>) {
        self.rules.push(rule);
    }

    pub fn rule_names(&self) -> Vec<String> {
        self.rules.iter().map(|r| r.name().to_string()).collect()
    }
}

/// Factory mirror of the TS `createSchemaValidator`.
pub fn create_schema_validator(options: SchemaValidatorOptions) -> SchemaValidator {
    let rules: Vec<Box<dyn SchemaValidationRule>> = vec![
        Box::new(DepthRule {
            max_depth: options.max_depth,
        }),
        Box::new(StringSizeRule {
            max_length: options.max_string_length,
        }),
        Box::new(ArraySizeRule {
            max_length: options.max_array_length,
        }),
        Box::new(KeyCountRule {
            max_keys: options.max_total_keys,
        }),
        Box::new(DisallowedKeysRule {
            patterns: options.disallowed_key_patterns,
        }),
    ];
    SchemaValidator { rules }
}

// ── Built-in rules ─────────────────────────────────────────────────────────

struct DepthRule {
    max_depth: usize,
}

impl SchemaValidationRule for DepthRule {
    fn name(&self) -> &str {
        "max-depth"
    }
    fn description(&self) -> String {
        format!("Rejects data nested deeper than {} levels", self.max_depth)
    }
    fn check(&self, data: &Value, path: &str) -> Vec<SchemaValidationIssue> {
        let mut issues = Vec::new();
        walk_depth(data, path, 0, self.max_depth, &mut issues);
        issues
    }
}

fn walk_depth(
    data: &Value,
    path: &str,
    depth: usize,
    max_depth: usize,
    issues: &mut Vec<SchemaValidationIssue>,
) {
    if depth > max_depth {
        issues.push(SchemaValidationIssue {
            path: path.to_string(),
            message: format!("Nesting depth {depth} exceeds maximum {max_depth}"),
            severity: SchemaValidationSeverity::Error,
            rule: "max-depth".to_string(),
        });
        return;
    }
    match data {
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                walk_depth(item, &format!("{path}[{i}]"), depth + 1, max_depth, issues);
            }
        }
        Value::Object(map) => {
            for (key, val) in map {
                walk_depth(val, &format!("{path}.{key}"), depth + 1, max_depth, issues);
            }
        }
        _ => {}
    }
}

struct StringSizeRule {
    max_length: usize,
}

impl SchemaValidationRule for StringSizeRule {
    fn name(&self) -> &str {
        "max-string-length"
    }
    fn description(&self) -> String {
        format!("Rejects strings longer than {} characters", self.max_length)
    }
    fn check(&self, data: &Value, path: &str) -> Vec<SchemaValidationIssue> {
        let mut issues = Vec::new();
        walk_strings(data, path, self.max_length, &mut issues);
        issues
    }
}

fn walk_strings(
    data: &Value,
    path: &str,
    max_length: usize,
    issues: &mut Vec<SchemaValidationIssue>,
) {
    match data {
        Value::String(s) => {
            if s.chars().count() > max_length {
                issues.push(SchemaValidationIssue {
                    path: path.to_string(),
                    message: format!(
                        "String length {} exceeds maximum {}",
                        s.chars().count(),
                        max_length
                    ),
                    severity: SchemaValidationSeverity::Error,
                    rule: "max-string-length".to_string(),
                });
            }
        }
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                walk_strings(item, &format!("{path}[{i}]"), max_length, issues);
            }
        }
        Value::Object(map) => {
            for (key, val) in map {
                walk_strings(val, &format!("{path}.{key}"), max_length, issues);
            }
        }
        _ => {}
    }
}

struct ArraySizeRule {
    max_length: usize,
}

impl SchemaValidationRule for ArraySizeRule {
    fn name(&self) -> &str {
        "max-array-length"
    }
    fn description(&self) -> String {
        format!("Rejects arrays longer than {} elements", self.max_length)
    }
    fn check(&self, data: &Value, path: &str) -> Vec<SchemaValidationIssue> {
        let mut issues = Vec::new();
        walk_arrays(data, path, self.max_length, &mut issues);
        issues
    }
}

fn walk_arrays(
    data: &Value,
    path: &str,
    max_length: usize,
    issues: &mut Vec<SchemaValidationIssue>,
) {
    match data {
        Value::Array(items) => {
            if items.len() > max_length {
                issues.push(SchemaValidationIssue {
                    path: path.to_string(),
                    message: format!(
                        "Array length {} exceeds maximum {}",
                        items.len(),
                        max_length
                    ),
                    severity: SchemaValidationSeverity::Error,
                    rule: "max-array-length".to_string(),
                });
            }
            for (i, item) in items.iter().enumerate() {
                walk_arrays(item, &format!("{path}[{i}]"), max_length, issues);
            }
        }
        Value::Object(map) => {
            for (key, val) in map {
                walk_arrays(val, &format!("{path}.{key}"), max_length, issues);
            }
        }
        _ => {}
    }
}

struct KeyCountRule {
    max_keys: usize,
}

impl SchemaValidationRule for KeyCountRule {
    fn name(&self) -> &str {
        "max-total-keys"
    }
    fn description(&self) -> String {
        format!(
            "Rejects data with more than {} total object keys",
            self.max_keys
        )
    }
    fn check(&self, data: &Value, path: &str) -> Vec<SchemaValidationIssue> {
        let total = count_keys(data);
        if total > self.max_keys {
            vec![SchemaValidationIssue {
                path: path.to_string(),
                message: format!(
                    "Total key count {} exceeds maximum {}",
                    total, self.max_keys
                ),
                severity: SchemaValidationSeverity::Error,
                rule: "max-total-keys".to_string(),
            }]
        } else {
            Vec::new()
        }
    }
}

fn count_keys(data: &Value) -> usize {
    match data {
        Value::Array(items) => items.iter().map(count_keys).sum(),
        Value::Object(map) => map.len() + map.values().map(count_keys).sum::<usize>(),
        _ => 0,
    }
}

struct DisallowedKeysRule {
    patterns: Vec<Regex>,
}

impl SchemaValidationRule for DisallowedKeysRule {
    fn name(&self) -> &str {
        "disallowed-keys"
    }
    fn description(&self) -> String {
        "Rejects objects with keys matching dangerous patterns".to_string()
    }
    fn check(&self, data: &Value, path: &str) -> Vec<SchemaValidationIssue> {
        let mut issues = Vec::new();
        walk_keys(data, path, &self.patterns, &mut issues);
        issues
    }
}

fn walk_keys(
    data: &Value,
    path: &str,
    patterns: &[Regex],
    issues: &mut Vec<SchemaValidationIssue>,
) {
    match data {
        Value::Object(map) => {
            for (key, val) in map {
                for pattern in patterns {
                    if pattern.is_match(key) {
                        issues.push(SchemaValidationIssue {
                            path: format!("{path}.{key}"),
                            message: format!(
                                "Key \"{key}\" matches disallowed pattern {}",
                                pattern.as_str()
                            ),
                            severity: SchemaValidationSeverity::Error,
                            rule: "disallowed-keys".to_string(),
                        });
                    }
                }
                walk_keys(val, &format!("{path}.{key}"), patterns, issues);
            }
        }
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                walk_keys(item, &format!("{path}[{i}]"), patterns, issues);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validates_safe_data() {
        let v = create_schema_validator(SchemaValidatorOptions::default());
        let result = v.validate(&json!({ "name": "test", "value": 42 }));
        assert!(result.valid);
        assert!(result.issues.is_empty());
    }

    #[test]
    fn rejects_deeply_nested_data() {
        let v = create_schema_validator(SchemaValidatorOptions {
            max_depth: 3,
            ..Default::default()
        });
        let data = json!({ "a": { "b": { "c": { "d": { "e": 1 } } } } });
        let result = v.validate(&data);
        assert!(!result.valid);
        assert!(result.issues.iter().any(|i| i.rule == "max-depth"));
    }

    #[test]
    fn rejects_oversized_strings() {
        let v = create_schema_validator(SchemaValidatorOptions {
            max_string_length: 10,
            ..Default::default()
        });
        let result = v.validate(&json!({ "text": "a".repeat(20) }));
        assert!(!result.valid);
        assert!(result.issues.iter().any(|i| i.rule == "max-string-length"));
    }

    #[test]
    fn rejects_oversized_arrays() {
        let v = create_schema_validator(SchemaValidatorOptions {
            max_array_length: 5,
            ..Default::default()
        });
        let result = v.validate(&json!({ "items": vec![0; 10] }));
        assert!(!result.valid);
        assert!(result.issues.iter().any(|i| i.rule == "max-array-length"));
    }

    #[test]
    fn rejects_too_many_total_keys() {
        let v = create_schema_validator(SchemaValidatorOptions {
            max_total_keys: 5,
            ..Default::default()
        });
        let result = v.validate(&json!({
            "a": 1, "b": 2, "c": 3, "d": 4, "e": 5, "f": 6,
        }));
        assert!(!result.valid);
        assert!(result.issues.iter().any(|i| i.rule == "max-total-keys"));
    }

    #[test]
    fn rejects_proto_keys() {
        let v = create_schema_validator(SchemaValidatorOptions::default());
        let data: Value = serde_json::from_str(r#"{"__proto__": {"isAdmin": true}}"#).unwrap();
        let result = v.validate(&data);
        assert!(!result.valid);
        assert!(result.issues.iter().any(|i| i.rule == "disallowed-keys"));
        assert!(result
            .issues
            .iter()
            .any(|i| i.message.contains("__proto__")));
    }

    #[test]
    fn rejects_constructor_keys() {
        let v = create_schema_validator(SchemaValidatorOptions::default());
        let data: Value =
            serde_json::from_str(r#"{"constructor": {"prototype": {"evil": true}}}"#).unwrap();
        let result = v.validate(&data);
        assert!(!result.valid);
        assert!(result
            .issues
            .iter()
            .any(|i| i.message.contains("constructor")));
    }

    struct NoSecretRule;
    impl SchemaValidationRule for NoSecretRule {
        fn name(&self) -> &str {
            "no-secret"
        }
        fn description(&self) -> String {
            "Rejects objects with 'secret' key".to_string()
        }
        fn check(&self, data: &Value, path: &str) -> Vec<SchemaValidationIssue> {
            if let Value::Object(map) = data {
                if map.contains_key("secret") {
                    return vec![SchemaValidationIssue {
                        path: format!("{path}.secret"),
                        message: "Secret not allowed".into(),
                        severity: SchemaValidationSeverity::Error,
                        rule: "no-secret".into(),
                    }];
                }
            }
            Vec::new()
        }
    }

    #[test]
    fn adds_custom_validation_rules() {
        let mut v = create_schema_validator(SchemaValidatorOptions::default());
        v.add_rule(Box::new(NoSecretRule));
        assert!(v.rule_names().iter().any(|n| n == "no-secret"));
        let result = v.validate(&json!({ "secret": "password123" }));
        assert!(!result.valid);
    }

    #[test]
    fn lists_built_in_rule_names() {
        let v = create_schema_validator(SchemaValidatorOptions::default());
        let names = v.rule_names();
        for expected in [
            "max-depth",
            "max-string-length",
            "max-array-length",
            "max-total-keys",
            "disallowed-keys",
        ] {
            assert!(
                names.iter().any(|n| n == expected),
                "missing rule {expected}"
            );
        }
    }

    #[test]
    fn validates_nested_arrays() {
        let v = create_schema_validator(SchemaValidatorOptions {
            max_array_length: 3,
            ..Default::default()
        });
        let result = v.validate(&json!({ "lists": [[1, 2, 3, 4, 5]] }));
        assert!(!result.valid);
    }

    #[test]
    fn handles_null_and_primitives_gracefully() {
        let v = create_schema_validator(SchemaValidatorOptions::default());
        assert!(v.validate(&Value::Null).valid);
        assert!(v.validate(&json!(42)).valid);
        assert!(v.validate(&json!("hello")).valid);
        assert!(v.validate(&json!(true)).valid);
    }
}
