use std::{collections::BTreeMap, sync::OnceLock};

use serde::Deserialize;
use serde_json::Value;

const DOCUMENT_SPEC_SOURCE: &str = include_str!("../editor-spec/document-spec.json");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DocumentSpec {
    pub(crate) spec_version: u32,
    pub(crate) envelope: EnvelopeSpec,
    pub(crate) contexts: Vec<String>,
    pub(crate) domains: BTreeMap<String, Vec<String>>,
    pub(crate) limits: BTreeMap<String, u64>,
    pub(crate) nodes: BTreeMap<String, ValidationRule>,
    pub(crate) marks: BTreeMap<String, ValidationRule>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EnvelopeSpec {
    pub(crate) schema_version: u32,
    pub(crate) editor_engine: String,
    pub(crate) document_schema: String,
    pub(crate) document_schema_version: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ValidationRule {
    pub(crate) validator: String,
    #[serde(default)]
    pub(crate) invariants: Vec<String>,
    #[serde(default)]
    pub(crate) contexts: Vec<String>,
    #[serde(default)]
    pub(crate) attrs: BTreeMap<String, AttributeRule>,
    #[serde(default = "default_attr_policy")]
    pub(crate) attr_policy: String,
    #[serde(default)]
    pub(crate) allowed_marks: Vec<String>,
    pub(crate) list_item_type: Option<String>,
    pub(crate) list_item_context: Option<String>,
    pub(crate) first_child_type: Option<String>,
    pub(crate) child_context: Option<String>,
    #[serde(default)]
    pub(crate) child_types: Vec<String>,
    pub(crate) content_min: Option<usize>,
    #[serde(default)]
    pub(crate) markless_contexts: Vec<String>,
    #[serde(default)]
    pub(crate) mutually_exclusive_marks: Vec<Vec<String>>,
    #[serde(default)]
    pub(crate) max_one_of_marks: Vec<String>,
    #[serde(default)]
    pub(crate) item_attrs: BTreeMap<String, AttributeRule>,
    #[serde(default)]
    pub(crate) media_annotation_attrs: BTreeMap<String, AttributeRule>,
    pub(crate) unique_attr: Option<String>,
    pub(crate) time_based_kind_domain: Option<String>,
    #[serde(default)]
    pub(crate) media_annotation_sort_key: Vec<String>,
    #[serde(rename = "prosemirror")]
    pub(crate) _prosemirror: Option<Value>,
}

fn default_attr_policy() -> String {
    "passthrough".to_string()
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AttributeRule {
    #[serde(rename = "type")]
    pub(crate) value_type: String,
    #[serde(default)]
    pub(crate) nullable: bool,
    #[serde(default)]
    pub(crate) required: bool,
    pub(crate) enum_domain: Option<String>,
    pub(crate) min: Option<u64>,
    pub(crate) max: Option<u64>,
    pub(crate) min_limit: Option<String>,
    pub(crate) max_limit: Option<String>,
    pub(crate) max_bytes_limit: Option<String>,
    pub(crate) max_items_limit: Option<String>,
    #[serde(default)]
    pub(crate) trim_non_empty: bool,
    #[serde(default)]
    pub(crate) non_empty: bool,
    #[serde(default)]
    pub(crate) no_control_chars: bool,
    #[serde(default)]
    pub(crate) trim_stable: bool,
    #[serde(rename = "const")]
    pub(crate) const_value: Option<Value>,
    #[serde(rename = "default")]
    pub(crate) _default_value: Option<Value>,
}

static DOCUMENT_SPEC: OnceLock<DocumentSpec> = OnceLock::new();

pub(crate) fn document_spec() -> &'static DocumentSpec {
    DOCUMENT_SPEC.get_or_init(|| {
        let spec: DocumentSpec = serde_json::from_str(DOCUMENT_SPEC_SOURCE)
            .expect("embedded Rustnote editor document specification must be valid JSON");
        assert_eq!(spec.spec_version, 1, "unsupported embedded Rustnote document spec version");
        spec
    })
}

pub(crate) fn node_rule(name: &str) -> Option<&'static ValidationRule> {
    document_spec().nodes.get(name)
}

pub(crate) fn mark_rule(name: &str) -> Option<&'static ValidationRule> {
    document_spec().marks.get(name)
}

pub(crate) fn limit(name: &str) -> u64 {
    *document_spec()
        .limits
        .get(name)
        .unwrap_or_else(|| panic!("Rustnote document specification is missing limit {name}"))
}

pub(crate) fn domain_contains(name: &str, value: &str) -> bool {
    document_spec()
        .domains
        .get(name)
        .unwrap_or_else(|| panic!("Rustnote document specification is missing domain {name}"))
        .iter()
        .any(|candidate| candidate == value)
}

pub(crate) fn context_allowed(rule: &ValidationRule, context: &str) -> bool {
    document_spec()
        .contexts
        .iter()
        .any(|candidate| candidate == context)
        && rule.contexts.iter().any(|candidate| candidate == context)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::NoteId;

    #[test]
    fn canonical_document_spec_is_complete_and_self_consistent() {
        let spec = document_spec();
        assert_eq!(spec.spec_version, 1);
        assert!(!spec.contexts.is_empty());
        assert!(!spec.nodes.is_empty());
        assert!(!spec.marks.is_empty());
        assert_eq!(
            spec.limits.get("note_id_max_bytes").copied(),
            Some(NoteId::MAX_LEN as u64),
            "editor document note-ID limit must match the Rust NoteId domain contract"
        );

        let mut contexts = spec.contexts.clone();
        contexts.sort();
        contexts.dedup();
        assert_eq!(contexts.len(), spec.contexts.len(), "duplicate document contexts");

        for (name, rule) in spec.nodes.iter().chain(spec.marks.iter()) {
            assert!(!name.is_empty());
            assert!(!rule.validator.is_empty());
            let mut invariant_names = rule.invariants.clone();
            invariant_names.sort();
            invariant_names.dedup();
            assert_eq!(
                invariant_names.len(),
                rule.invariants.len(),
                "duplicate invariants for {name}"
            );
            for context in &rule.contexts {
                assert!(spec.contexts.contains(context), "missing context {context}");
            }
            for attrs in [&rule.attrs, &rule.item_attrs, &rule.media_annotation_attrs] {
                for attr in attrs.values() {
                    if let Some(domain) = attr.enum_domain.as_deref() {
                        assert!(spec.domains.contains_key(domain), "missing domain {domain}");
                    }
                    for limit_name in [
                        attr.min_limit.as_deref(),
                        attr.max_limit.as_deref(),
                        attr.max_bytes_limit.as_deref(),
                        attr.max_items_limit.as_deref(),
                    ]
                    .into_iter()
                    .flatten()
                    {
                        assert!(spec.limits.contains_key(limit_name), "missing limit {limit_name}");
                    }
                }
            }
        }
    }
}
