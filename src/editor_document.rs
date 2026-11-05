use std::{
    collections::BTreeSet,
    error::Error,
    fmt::{self, Display, Formatter},
};

use serde::Deserialize;
use serde_json::{Map, Value};

use crate::{
    domain::NoteId,
    editor_document_spec::{
        AttributeRule, ValidationRule, context_allowed, document_spec, domain_contains, limit,
        mark_rule, node_rule,
    },
    link::is_safe_href,
};

pub const EMPTY_EDITOR_STATE: &str = include_str!("../assets/blank-editor-state.json");

#[derive(Debug)]
pub enum DocumentValidationError {
    InvalidJson(serde_json::Error),
    InvalidDocument(String),
}

impl DocumentValidationError {
    fn invalid(message: impl Into<String>) -> Self {
        Self::InvalidDocument(message.into())
    }
}

impl Display for DocumentValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJson(error) => write!(formatter, "editor state is not valid JSON: {error}"),
            Self::InvalidDocument(message) => formatter.write_str(message),
        }
    }
}

impl Error for DocumentValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidJson(error) => Some(error),
            Self::InvalidDocument(_) => None,
        }
    }
}

impl From<String> for DocumentValidationError {
    fn from(message: String) -> Self {
        Self::InvalidDocument(message)
    }
}

pub type DocumentValidationResult<T> = Result<T, DocumentValidationError>;

fn invalid<T>(message: impl Into<String>) -> DocumentValidationResult<T> {
    Err(DocumentValidationError::invalid(message))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DocumentHeading {
    pub level: u8,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DocumentAnalysis {
    pub attachment_ids: Vec<String>,
    pub pending_attachment_request_ids: Vec<String>,
    pub note_links: Vec<NoteId>,
}

#[derive(Debug, Deserialize)]
struct VersionedEditorDocument {
    schema_version: u32,
    editor_engine: String,
    document_schema: String,
    document_schema_version: u32,
    document: Value,
}

pub fn validate_editor_state(serialized: &str) -> DocumentValidationResult<()> {
    let envelope = parse_editor_state_envelope(serialized)?;
    validate_document(&envelope.document)
}

pub(crate) fn parse_validated_document(serialized: &str) -> DocumentValidationResult<Value> {
    let envelope = parse_editor_state_envelope(serialized)?;
    validate_document(&envelope.document)?;
    Ok(envelope.document)
}

pub(crate) fn analyze_editor_state(serialized: &str) -> DocumentValidationResult<DocumentAnalysis> {
    let envelope = parse_editor_state_envelope(serialized)?;
    validate_and_analyze_document(&envelope.document)
}

/// Lightweight live-outline extraction. The canonical save path uses
/// `analyze_editor_state`, which validates and collects structure in one walk.
/// The live Outline panel only needs headings from ProseMirror-owned state, so
/// it avoids re-running the full validator on every editor revision.
pub(crate) fn extract_outline(serialized: &str) -> DocumentValidationResult<Vec<DocumentHeading>> {
    let envelope = parse_editor_state_envelope(serialized)?;
    let mut headings = Vec::new();
    collect_outline_headings(&envelope.document, &mut headings);
    Ok(headings)
}

#[derive(Default)]
struct DocumentAnalysisBuilder {
    collecting: bool,
    attachment_ids: BTreeSet<String>,
    pending_attachment_request_ids: BTreeSet<String>,
    note_links: BTreeSet<NoteId>,
}

impl DocumentAnalysisBuilder {
    fn collecting() -> Self {
        Self {
            collecting: true,
            ..Self::default()
        }
    }

    fn finish(self) -> DocumentAnalysis {
        DocumentAnalysis {
            attachment_ids: self.attachment_ids.into_iter().collect(),
            pending_attachment_request_ids: self
                .pending_attachment_request_ids
                .into_iter()
                .collect(),
            note_links: self.note_links.into_iter().collect(),
        }
    }
}

fn normalize_heading_text(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        "Untitled heading".to_string()
    } else {
        text
    }
}

fn collect_outline_headings(value: &Value, headings: &mut Vec<DocumentHeading>) {
    let Some(node) = value.as_object() else {
        return;
    };
    if node.get("type").and_then(Value::as_str) == Some("heading") {
        let minimum_level = u8::try_from(limit("heading_level_min"))
            .expect("document-spec heading minimum must fit in u8");
        let maximum_level = u8::try_from(limit("heading_level_max"))
            .expect("document-spec heading maximum must fit in u8");
        let level = node
            .get("attrs")
            .and_then(Value::as_object)
            .and_then(|attrs| attrs.get("level"))
            .and_then(Value::as_u64)
            .and_then(|level| u8::try_from(level).ok())
            .unwrap_or(minimum_level)
            .clamp(minimum_level, maximum_level);
        let mut text = String::new();
        collect_outline_text(node.get("content"), &mut text);
        headings.push(DocumentHeading {
            level,
            text: normalize_heading_text(&text),
        });
        return;
    }
    if let Some(content) = node.get("content").and_then(Value::as_array) {
        for child in content {
            collect_outline_headings(child, headings);
        }
    }
}

fn collect_outline_text(content: Option<&Value>, output: &mut String) {
    let Some(content) = content.and_then(Value::as_array) else {
        return;
    };
    for child in content {
        let Some(node) = child.as_object() else {
            continue;
        };
        match node.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(text) = node.get("text").and_then(Value::as_str) {
                    output.push_str(text);
                }
            }
            Some("hard_break") => output.push(' '),
            _ => collect_outline_text(node.get("content"), output),
        }
    }
}

fn parse_editor_state_envelope(serialized: &str) -> DocumentValidationResult<VersionedEditorDocument> {
    let serialized = serialized.trim();
    if serialized.is_empty() {
        return invalid("editor state is required");
    }
    let envelope: VersionedEditorDocument = serde_json::from_str(serialized)
        .map_err(DocumentValidationError::InvalidJson)?;
    let expected = &document_spec().envelope;
    if envelope.schema_version != expected.schema_version {
        return invalid(format!(
            "editor state schema {} is unsupported; expected {}",
            envelope.schema_version, expected.schema_version
        ));
    }
    if envelope.editor_engine != expected.editor_engine {
        return invalid(format!(
            "editor engine {} is unsupported; expected {}",
            envelope.editor_engine, expected.editor_engine
        ));
    }
    if envelope.document_schema != expected.document_schema {
        return invalid(format!(
            "document schema {} is unsupported; expected {}",
            envelope.document_schema, expected.document_schema
        ));
    }
    if envelope.document_schema_version != expected.document_schema_version {
        return invalid(format!(
            "document schema version {} is unsupported; expected {}",
            envelope.document_schema_version, expected.document_schema_version
        ));
    }
    Ok(envelope)
}

fn validate_document(document: &Value) -> DocumentValidationResult<()> {
    let mut analysis = DocumentAnalysisBuilder::default();
    validate_node(document, NodeContext::Root, "document", &mut analysis)
}

fn validate_and_analyze_document(document: &Value) -> DocumentValidationResult<DocumentAnalysis> {
    let mut analysis = DocumentAnalysisBuilder::collecting();
    validate_node(document, NodeContext::Root, "document", &mut analysis)?;
    Ok(analysis.finish())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NodeContext {
    Root,
    Block,
    Inline,
    ListItem,
    TaskItem,
    TableRow,
    TableCell,
    TableCellBlock,
    Scientific,
}

impl NodeContext {
    fn as_spec_name(self) -> &'static str {
        match self {
            Self::Root => "root",
            Self::Block => "block",
            Self::Inline => "inline",
            Self::ListItem => "list_item",
            Self::TaskItem => "task_item",
            Self::TableRow => "table_row",
            Self::TableCell => "table_cell",
            Self::TableCellBlock => "table_cell_block",
            Self::Scientific => "scientific",
        }
    }

    fn from_spec_name(name: &str) -> Option<Self> {
        match name {
            "root" => Some(Self::Root),
            "block" => Some(Self::Block),
            "inline" => Some(Self::Inline),
            "list_item" => Some(Self::ListItem),
            "task_item" => Some(Self::TaskItem),
            "table_row" => Some(Self::TableRow),
            "table_cell" => Some(Self::TableCell),
            "table_cell_block" => Some(Self::TableCellBlock),
            "scientific" => Some(Self::Scientific),
            _ => None,
        }
    }
}

fn node_validator_invariants(validator: &str) -> Option<&'static [&'static str]> {
    match validator {
        "doc" => Some(&["children_match_context", "content_minimum"]),
        "textblock" => Some(&[
            "boundary_layout_default",
            "children_match_context",
            "drop_cap_first_line_exclusive",
        ]),
        "heading" => Some(&["children_match_context", "drop_cap_first_line_exclusive"]),
        "blockquote" => Some(&["children_match_context", "content_minimum"]),
        "code_block" => Some(&[
            "allowed_mark_restriction",
            "child_type_restriction",
            "children_match_context",
        ]),
        "list" => Some(&["content_minimum", "list_item_context", "list_item_type"]),
        "list_item" => Some(&[
            "children_match_context",
            "content_minimum",
            "first_child_type",
        ]),
        "text" => Some(&[
            "markless_context",
            "marks_array",
            "max_one_of_marks",
            "mutually_exclusive_marks",
            "text_leaf",
            "text_required_non_empty",
        ]),
        "hard_break" => Some(&["hard_break_leaf"]),
        "horizontal_rule" => Some(&["horizontal_rule_leaf"]),
        "scientific" => Some(&[
            "child_type_restriction",
            "children_match_context",
            "scientific_plain_text_only",
        ]),
        "pending_attachment" => Some(&[
            "attachment_leaf",
            "nested_item_attrs",
            "unique_attribute",
        ]),
        "attachment" => Some(&[
            "attachment_leaf",
            "media_annotation_attrs",
            "media_annotation_end_after_start",
            "media_annotation_sort_order",
            "media_annotation_unique_id",
            "media_trim_end_after_start",
            "time_based_annotation_scope",
            "time_based_trim_scope",
        ]),
        "table" => Some(&[
            "child_type_restriction",
            "children_match_context",
            "content_minimum",
            "structural_table_fields",
            "table_geometry",
        ]),
        "table_row" => Some(&[
            "child_type_restriction",
            "children_match_context",
            "content_minimum",
            "structural_table_fields",
        ]),
        "table_cell" => Some(&[
            "children_match_context",
            "column_width_shape",
            "content_minimum",
            "table_cell_inline_fields",
        ]),
        _ => None,
    }
}

fn mark_validator_invariants(validator: &str) -> Option<&'static [&'static str]> {
    match validator {
        "simple" | "font_family" | "font_size" | "text_color" => Some(&[]),
        "highlight" => Some(&["highlight_explicit_attrs_color"]),
        "link" => Some(&["safe_href"]),
        "note_link" => Some(&["valid_note_id"]),
        "media_link" => Some(&["media_link_end_after_start", "valid_note_id"]),
        _ => None,
    }
}

fn validate_invariant_contract(
    rule: &ValidationRule,
    implemented: Option<&[&str]>,
    path: &str,
) -> DocumentValidationResult<()> {
    let implemented = implemented.ok_or_else(|| {
        format!(
            "{path}: document specification references unsupported validator {}",
            rule.validator
        )
    })?;
    if rule.invariants.len() != implemented.len()
        || rule
            .invariants
            .iter()
            .any(|name| !implemented.contains(&name.as_str()))
    {
        return invalid(format!(
            "{path}: document specification invariant contract does not match validator {}",
            rule.validator
        ));
    }
    Ok(())
}

fn validate_node(
    value: &Value,
    context: NodeContext,
    path: &str,
    analysis: &mut DocumentAnalysisBuilder,
) -> DocumentValidationResult<()> {
    let node = value
        .as_object()
        .ok_or_else(|| format!("{path} must be a JSON object"))?;
    let node_type = required_string(node, "type", path)?;
    let rule = node_rule(node_type)
        .ok_or_else(|| format!("{path} contains unsupported node type {node_type}"))?;
    validate_invariant_contract(rule, node_validator_invariants(&rule.validator), path)?;
    if !context_allowed(rule, context.as_spec_name()) {
        return invalid(format!(
            "{path}: {node_type} appears in an invalid document position"
        ));
    }
    validate_spec_attrs(node, rule, path)?;
    match rule.validator.as_str() {
        "doc" => validate_doc(node, context, path, rule, analysis),
        "textblock" => validate_textblock(node, context, path, rule, analysis),
        "heading" => validate_heading(node, context, path, rule, analysis),
        "blockquote" => validate_block_container(node, context, path, rule, analysis),
        "code_block" => validate_code_block(node, context, path, rule, analysis),
        "list" => validate_list(node, context, path, node_type, rule, analysis),
        "list_item" => validate_list_item(node, context, path, rule, analysis),
        "text" => validate_text(node, context, path, rule, analysis),
        "hard_break" => validate_hard_break(node, context, path),
        "horizontal_rule" => validate_horizontal_rule(node, context, path),
        "pending_attachment" => {
            validate_pending_attachment_node(node, context, path, rule, analysis)
        }
        "attachment" => validate_attachment_node(node, context, path, rule, analysis),
        "scientific" => validate_scientific_node(node, context, path, rule, analysis),
        "table" => validate_table(node, context, path, rule, analysis),
        "table_row" => validate_table_row(node, context, path, rule, analysis),
        "table_cell" => validate_table_cell(node, context, path, rule, analysis),
        unknown => invalid(format!(
            "{path}: document specification references unsupported node validator {unknown}"
        )),
    }
}

fn validate_spec_attrs(
    object: &Map<String, Value>,
    rule: &ValidationRule,
    path: &str,
) -> DocumentValidationResult<()> {
    if rule.attr_policy == "passthrough" {
        return Ok(());
    }
    let Some(attrs_value) = object.get("attrs") else {
        if let Some((name, _)) = rule.attrs.iter().find(|(_, attr)| attr.required) {
            return invalid(format!("{path}.attrs.{name} is required"));
        }
        return Ok(());
    };
    let attrs = attrs_value
        .as_object()
        .ok_or_else(|| format!("{path}.attrs must be a JSON object"))?;
    validate_attribute_object(attrs, &rule.attrs, &rule.attr_policy, &format!("{path}.attrs"))
}

fn validate_attribute_object(
    attrs: &Map<String, Value>,
    rules: &std::collections::BTreeMap<String, AttributeRule>,
    policy: &str,
    path: &str,
) -> DocumentValidationResult<()> {
    if policy == "strict" {
        for key in attrs.keys() {
            if !rules.contains_key(key) {
                return invalid(format!("{path}.{key} is unsupported"));
            }
        }
    }
    if policy != "passthrough" {
        for (name, rule) in rules {
            let Some(value) = attrs.get(name) else {
                if rule.required {
                    return invalid(format!("{path}.{name} is required"));
                }
                continue;
            };
            validate_attribute_value(value, rule, &format!("{path}.{name}"))?;
        }
    }
    Ok(())
}

fn validate_attribute_value(value: &Value, rule: &AttributeRule, path: &str) -> DocumentValidationResult<()> {
    if value.is_null() {
        return if rule.nullable {
            Ok(())
        } else {
            invalid(format!("{path} cannot be null"))
        };
    }

    match rule.value_type.as_str() {
        "string" => {
            let value = value
                .as_str()
                .ok_or_else(|| format!("{path} must be a string"))?;
            if rule.trim_non_empty && value.trim().is_empty() {
                return invalid(format!("{path} must not be blank"));
            }
            if rule.trim_stable && value.trim() != value {
                return invalid(format!("{path} must not contain surrounding whitespace"));
            }
            if rule.non_empty && value.is_empty() {
                return invalid(format!("{path} must not be empty"));
            }
            if rule.no_control_chars
                && value.chars().any(|character| {
                    let code = character as u32;
                    code <= 0x1f || (0x7f..=0x9f).contains(&code)
                })
            {
                return invalid(format!("{path} contains a control character"));
            }
            if let Some(limit_name) = rule.max_bytes_limit.as_deref()
                && value.len() as u64 > limit(limit_name)
            {
                return invalid(format!("{path} exceeds its byte limit"));
            }
            if let Some(domain) = rule.enum_domain.as_deref()
                && !domain_contains(domain, value)
            {
                return invalid(format!("{path} is outside its allowed domain"));
            }
        }
        "integer" => {
            let value = value
                .as_u64()
                .ok_or_else(|| format!("{path} must be a non-negative integer"))?;
            let minimum = rule
                .min
                .or_else(|| rule.min_limit.as_deref().map(limit));
            let maximum = rule
                .max
                .or_else(|| rule.max_limit.as_deref().map(limit))
                .or_else(|| Some(limit("json_safe_integer_max")));
            if minimum.is_some_and(|minimum| value < minimum) {
                return invalid(format!("{path} is below its minimum"));
            }
            if maximum.is_some_and(|maximum| value > maximum) {
                return invalid(format!("{path} exceeds its maximum"));
            }
        }
        "boolean" => {
            if !value.is_boolean() {
                return invalid(format!("{path} must be a boolean"));
            }
        }
        "array" => {
            let values = value
                .as_array()
                .ok_or_else(|| format!("{path} must be an array"))?;
            if let Some(limit_name) = rule.max_items_limit.as_deref()
                && values.len() as u64 > limit(limit_name)
            {
                return invalid(format!("{path} contains too many items"));
            }
        }
        unknown => {
            return invalid(format!(
                "document specification uses unsupported attribute type {unknown} at {path}"
            ));
        }
    }

    if let Some(expected) = &rule.const_value
        && value != expected
    {
        return invalid(format!("{path} has an unsupported fixed value"));
    }
    Ok(())
}

fn spec_child_context(rule: &ValidationRule, path: &str) -> DocumentValidationResult<NodeContext> {
    rule.child_context
        .as_deref()
        .and_then(NodeContext::from_spec_name)
        .ok_or_else(|| {
            DocumentValidationError::invalid(format!(
                "{path}: document specification is missing a valid child context"
            ))
        })
}

fn validate_spec_children(
    content: &[Value],
    rule: &ValidationRule,
    path: &str,
    analysis: &mut DocumentAnalysisBuilder,
) -> DocumentValidationResult<()> {
    let child_context = spec_child_context(rule, path)?;
    for (index, child) in content.iter().enumerate() {
        let child_path = format!("{path}.content[{index}]");
        if !rule.child_types.is_empty() {
            let child_type = child
                .get("type")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{child_path}.type is required"))?;
            if !rule.child_types.iter().any(|allowed| allowed == child_type) {
                return invalid(format!("{child_path} contains an invalid child node"));
            }
        }
        validate_node(child, child_context, &child_path, analysis)?;
    }
    Ok(())
}

fn require_spec_content<'a>(
    node: &'a Map<String, Value>,
    rule: &ValidationRule,
    path: &str,
) -> DocumentValidationResult<&'a [Value]> {
    let content = required_content(node, path)?;
    if content.len() < rule.content_min.unwrap_or_default() {
        return invalid(format!("{path} does not contain enough child nodes"));
    }
    Ok(content)
}

fn validate_doc(
    node: &Map<String, Value>,
    _context: NodeContext,
    path: &str,
    rule: &ValidationRule,
    analysis: &mut DocumentAnalysisBuilder,
) -> DocumentValidationResult<()> {
    let content = require_spec_content(node, rule, path)?;
    validate_spec_children(content, rule, path, analysis)
}

fn validate_textblock_layout_invariants(
    node: &Map<String, Value>,
    path: &str,
    boundary_aware: bool,
) -> DocumentValidationResult<()> {
    let attrs = node.get("attrs").and_then(Value::as_object);
    let first_line_indent = attrs
        .and_then(|attrs| attrs.get("firstLineIndent"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let drop_cap = attrs
        .and_then(|attrs| attrs.get("dropCap"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if drop_cap && first_line_indent != 0 {
        return invalid(format!(
            "{path}.attrs.dropCap cannot be combined with firstLineIndent"
        ));
    }
    let boundary = attrs
        .and_then(|attrs| attrs.get("boundary"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !boundary_aware || !boundary {
        return Ok(());
    }
    let line_spacing_is_default = attrs
        .and_then(|attrs| attrs.get("lineSpacing"))
        .is_none_or(Value::is_null);
    let text_alignment_is_default = attrs
        .and_then(|attrs| attrs.get("textAlignment"))
        .and_then(Value::as_str)
        .is_none_or(|value| value == "left");
    let left_indent = attrs
        .and_then(|attrs| attrs.get("leftIndent"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let right_indent = attrs
        .and_then(|attrs| attrs.get("rightIndent"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    if !line_spacing_is_default
        || !text_alignment_is_default
        || left_indent != 0
        || right_indent != 0
        || first_line_indent != 0
        || drop_cap
    {
        return invalid(format!(
            "{path}: atomic boundary paragraphs must use default paragraph layout"
        ));
    }
    Ok(())
}

fn validate_textblock(
    node: &Map<String, Value>,
    _context: NodeContext,
    path: &str,
    rule: &ValidationRule,
    analysis: &mut DocumentAnalysisBuilder,
) -> DocumentValidationResult<()> {
    validate_spec_children(optional_content(node, path)?, rule, path, analysis)?;
    validate_textblock_layout_invariants(node, path, true)
}

fn validate_heading(
    node: &Map<String, Value>,
    _context: NodeContext,
    path: &str,
    rule: &ValidationRule,
    analysis: &mut DocumentAnalysisBuilder,
) -> DocumentValidationResult<()> {
    validate_spec_children(optional_content(node, path)?, rule, path, analysis)?;
    validate_textblock_layout_invariants(node, path, false)
}

fn validate_block_container(
    node: &Map<String, Value>,
    _context: NodeContext,
    path: &str,
    rule: &ValidationRule,
    analysis: &mut DocumentAnalysisBuilder,
) -> DocumentValidationResult<()> {
    let content = require_spec_content(node, rule, path)?;
    validate_spec_children(content, rule, path, analysis)
}

fn validate_code_block(
    node: &Map<String, Value>,
    _context: NodeContext,
    path: &str,
    rule: &ValidationRule,
    analysis: &mut DocumentAnalysisBuilder,
) -> DocumentValidationResult<()> {
    let content = optional_content(node, path)?;
    validate_spec_children(content, rule, path, analysis)?;
    for (index, child) in content.iter().enumerate() {
        let child_path = format!("{path}.content[{index}]");
        if let Some(marks) = child.get("marks") {
            let marks = marks
                .as_array()
                .ok_or_else(|| format!("{child_path}.marks must be an array"))?;
            for (mark_index, mark) in marks.iter().enumerate() {
                let mark_path = format!("{child_path}.marks[{mark_index}]");
                let mark_type = mark
                    .get("type")
                    .and_then(Value::as_str)
                    .ok_or_else(|| format!("{mark_path}.type is required"))?;
                if !rule.allowed_marks.iter().any(|allowed| allowed == mark_type) {
                    return invalid(format!(
                        "{mark_path}: code blocks support only typography marks"
                    ));
                }
            }
        }
    }
    Ok(())
}

fn validate_list(
    node: &Map<String, Value>,
    _context: NodeContext,
    path: &str,
    node_type: &str,
    rule: &ValidationRule,
    analysis: &mut DocumentAnalysisBuilder,
) -> DocumentValidationResult<()> {
    let expected_item = rule
        .list_item_type
        .as_deref()
        .ok_or_else(|| format!("{path}: {node_type} has no item type in the document specification"))?;
    let item_context = rule
        .list_item_context
        .as_deref()
        .and_then(NodeContext::from_spec_name)
        .ok_or_else(|| format!("{path}: {node_type} has no valid item context in the document specification"))?;
    let content = require_spec_content(node, rule, path)?;
    for (index, child) in content.iter().enumerate() {
        let child_path = format!("{path}.content[{index}]");
        if child.get("type").and_then(Value::as_str) != Some(expected_item) {
            return invalid(format!(
                "{child_path}: {node_type} must contain {expected_item} nodes"
            ));
        }
        validate_node(child, item_context, &child_path, analysis)?;
    }
    Ok(())
}

fn validate_list_item(
    node: &Map<String, Value>,
    _context: NodeContext,
    path: &str,
    rule: &ValidationRule,
    analysis: &mut DocumentAnalysisBuilder,
) -> DocumentValidationResult<()> {
    let content = require_spec_content(node, rule, path)?;
    let first_child = rule
        .first_child_type
        .as_deref()
        .ok_or_else(|| format!("{path}: list item has no required first child in the document specification"))?;
    if content[0].get("type").and_then(Value::as_str) != Some(first_child) {
        return invalid(format!("{path}: list items must begin with {first_child}"));
    }
    validate_spec_children(content, rule, path, analysis)
}

fn validate_text(
    node: &Map<String, Value>,
    context: NodeContext,
    path: &str,
    rule: &ValidationRule,
    analysis: &mut DocumentAnalysisBuilder,
) -> DocumentValidationResult<()> {
    let text = node
        .get("text")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{path}.text must be a string"))?;
    if text.is_empty() {
        return invalid(format!("{path}.text must not be empty"));
    }
    if node.get("content").is_some() {
        return invalid(format!("{path}: text nodes cannot contain child nodes"));
    }
    let Some(marks) = node.get("marks") else {
        return Ok(());
    };
    let marks = marks
        .as_array()
        .ok_or_else(|| format!("{path}.marks must be an array"))?;
    if rule
        .markless_contexts
        .iter()
        .any(|candidate| candidate == context.as_spec_name())
        && !marks.is_empty()
    {
        return invalid(format!("{path}: this text context cannot contain marks"));
    }
    let mut mark_types = Vec::with_capacity(marks.len());
    for (index, mark) in marks.iter().enumerate() {
        validate_mark(mark, &format!("{path}.marks[{index}]"), analysis)?;
        if let Some(mark_type) = mark.get("type").and_then(Value::as_str) {
            mark_types.push(mark_type);
        }
    }
    for exclusive_group in &rule.mutually_exclusive_marks {
        if exclusive_group
            .iter()
            .filter(|candidate| mark_types.contains(&candidate.as_str()))
            .count()
            > 1
        {
            return invalid(format!("{path}: mutually exclusive marks overlap"));
        }
    }
    if mark_types
        .iter()
        .filter(|mark| {
            rule.max_one_of_marks
                .iter()
                .any(|candidate| candidate.as_str() == **mark)
        })
        .count()
        > 1
    {
        return invalid(format!("{path}: semantic link marks overlap"));
    }
    Ok(())
}

fn validate_mark(
    value: &Value,
    path: &str,
    analysis: &mut DocumentAnalysisBuilder,
) -> DocumentValidationResult<()> {
    let mark = value
        .as_object()
        .ok_or_else(|| format!("{path} must be a JSON object"))?;
    let mark_type = required_string(mark, "type", path)?;
    let rule = mark_rule(mark_type)
        .ok_or_else(|| format!("{path} contains unsupported mark type {mark_type}"))?;
    validate_invariant_contract(rule, mark_validator_invariants(&rule.validator), path)?;
    validate_spec_attrs(mark, rule, path)?;
    match rule.validator.as_str() {
        "simple" | "font_family" | "font_size" | "text_color" => Ok(()),
        "highlight" => {
            if let Some(attrs) = mark.get("attrs")
                && attrs
                    .as_object()
                    .is_some_and(|attrs| !attrs.contains_key("color"))
            {
                return invalid(format!("{path}.attrs.color must be a string"));
            }
            Ok(())
        }
        "link" => {
            let href = required_attrs(mark, path)?
                .get("href")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{path}.attrs.href must be a string"))?;
            if is_safe_href(href) {
                Ok(())
            } else {
                invalid(format!(
                    "{path}.attrs.href uses an unsupported or incomplete URL"
                ))
            }
        }
        "note_link" => {
            let note_id = required_attrs(mark, path)?
                .get("noteId")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{path}.attrs.noteId must be a string"))?;
            let note_id = NoteId::parse(note_id)
                .map_err(|_| format!("{path}.attrs.noteId is invalid"))?;
            if analysis.collecting {
                analysis.note_links.insert(note_id);
            }
            Ok(())
        }
        "media_link" => {
            let attrs = required_attrs(mark, path)?;
            let note_id = attrs
                .get("noteId")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{path}.attrs.noteId must be a string"))?;
            let note_id = NoteId::parse(note_id)
                .map_err(|_| format!("{path}.attrs.noteId is invalid"))?;
            let start_ms = attrs
                .get("startMs")
                .and_then(Value::as_u64)
                .ok_or_else(|| format!("{path}.attrs.startMs must be a non-negative integer"))?;
            if let Some(end_ms) = attrs.get("endMs")
                && !end_ms.is_null()
            {
                let end_ms = end_ms
                    .as_u64()
                    .ok_or_else(|| format!("{path}.attrs.endMs must be an integer or null"))?;
                if end_ms <= start_ms {
                    return invalid(format!("{path}.attrs.endMs must be greater than startMs"));
                }
            }
            if analysis.collecting {
                analysis.note_links.insert(note_id);
            }
            Ok(())
        }
        unknown => invalid(format!(
            "{path}: document specification references unsupported mark validator {unknown}"
        )),
    }
}

fn validate_scientific_node(
    node: &Map<String, Value>,
    _context: NodeContext,
    path: &str,
    rule: &ValidationRule,
    analysis: &mut DocumentAnalysisBuilder,
) -> DocumentValidationResult<()> {
    if node.get("attrs").is_some() || node.get("text").is_some() || node.get("marks").is_some() {
        return invalid(format!(
            "{path}: scientific nodes may contain only plain source-text children"
        ));
    }
    validate_spec_children(optional_content(node, path)?, rule, path, analysis)
}

fn validate_pending_attachment_node(
    node: &Map<String, Value>,
    _context: NodeContext,
    path: &str,
    rule: &ValidationRule,
    analysis: &mut DocumentAnalysisBuilder,
) -> DocumentValidationResult<()> {
    if node.get("content").is_some() || node.get("text").is_some() || node.get("marks").is_some() {
        return invalid(format!(
            "{path}: pending attachment nodes cannot contain text, marks, or child content"
        ));
    }
    let attrs = required_attrs(node, path)?;
    let unique_attr = rule
        .unique_attr
        .as_deref()
        .ok_or_else(|| format!("{path}: document specification is missing a unique pending attribute"))?;
    let request_id = attrs
        .get(unique_attr)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{path}.attrs.{unique_attr} must be a string"))?;
    if !analysis
        .pending_attachment_request_ids
        .insert(request_id.to_string())
    {
        return invalid(format!(
            "{path}.attrs.{unique_attr} duplicates another pending attachment request"
        ));
    }
    if let Some(items) = attrs.get("items")
        && !items.is_null()
    {
        for (index, item) in items
            .as_array()
            .ok_or_else(|| format!("{path}.attrs.items must be an array or null"))?
            .iter()
            .enumerate()
        {
            let item_path = format!("{path}.attrs.items[{index}]");
            let item = item
                .as_object()
                .ok_or_else(|| format!("{item_path} must be an object"))?;
            validate_attribute_object(item, &rule.item_attrs, "strict", &item_path)?;
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum MediaAnnotationSortValue {
    Number(u64),
    Text(String),
}

fn media_annotation_sort_key(
    object: &Map<String, Value>,
    rule: &ValidationRule,
    path: &str,
) -> DocumentValidationResult<Vec<MediaAnnotationSortValue>> {
    if rule.media_annotation_sort_key.is_empty() {
        return invalid(format!(
            "{path}: document specification is missing media annotation sort order"
        ));
    }
    let start_ms = object
        .get("startMs")
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("{path}.startMs must be an integer"))?;
    let end_ms = object.get("endMs").and_then(Value::as_u64);
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{path}.id must be a string"))?;

    rule.media_annotation_sort_key
        .iter()
        .map(|part| match part.as_str() {
            "startMs" => Ok(MediaAnnotationSortValue::Number(start_ms)),
            "endMs_or_startMs" => Ok(MediaAnnotationSortValue::Number(end_ms.unwrap_or(start_ms))),
            "id_utf8" => Ok(MediaAnnotationSortValue::Text(id.to_string())),
            unknown => invalid(format!(
                "{path}: document specification uses unsupported media annotation sort key {unknown}"
            )),
        })
        .collect()
}

fn validate_attachment_node(
    node: &Map<String, Value>,
    _context: NodeContext,
    path: &str,
    rule: &ValidationRule,
    analysis: &mut DocumentAnalysisBuilder,
) -> DocumentValidationResult<()> {
    if node.get("content").is_some() || node.get("text").is_some() || node.get("marks").is_some() {
        return invalid(format!(
            "{path}: attachment nodes cannot contain text, marks, or child content"
        ));
    }
    let attrs = required_attrs(node, path)?;
    let attachment_id = attrs
        .get("attachmentId")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{path}.attrs.attachmentId must be a string"))?;
    if analysis.collecting {
        analysis.attachment_ids.insert(attachment_id.to_string());
    }
    let kind = attrs
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{path}.attrs.kind must be a string"))?;
    let time_based_domain = rule
        .time_based_kind_domain
        .as_deref()
        .ok_or_else(|| format!("{path}: document specification is missing a time-based kind domain"))?;
    let time_based_kind = |kind: &str| domain_contains(time_based_domain, kind);

    if let Some(media_annotations) = attrs.get("mediaAnnotations")
        && !media_annotations.is_null()
    {
        if !time_based_kind(kind) {
            return invalid(format!(
                "{path}.attrs.mediaAnnotations is only supported for time-based attachments"
            ));
        }
        let annotations = media_annotations
            .as_array()
            .ok_or_else(|| format!("{path}.attrs.mediaAnnotations must be an array or null"))?;
        let mut annotation_ids = BTreeSet::new();
        let mut previous_key: Option<Vec<MediaAnnotationSortValue>> = None;
        for (index, annotation) in annotations.iter().enumerate() {
            let annotation_path = format!("{path}.attrs.mediaAnnotations[{index}]");
            let object = annotation
                .as_object()
                .ok_or_else(|| format!("{annotation_path} must be an object"))?;
            validate_attribute_object(
                object,
                &rule.media_annotation_attrs,
                "strict",
                &annotation_path,
            )?;
            let id = object
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{annotation_path}.id must be a string"))?;
            if !annotation_ids.insert(id.to_string()) {
                return invalid(format!("{annotation_path}.id is duplicated"));
            }
            let start_ms = object
                .get("startMs")
                .and_then(Value::as_u64)
                .ok_or_else(|| format!("{annotation_path}.startMs must be an integer"))?;
            let end_ms = match object.get("endMs") {
                None | Some(Value::Null) => None,
                Some(value) => Some(
                    value
                        .as_u64()
                        .ok_or_else(|| format!("{annotation_path}.endMs must be an integer or null"))?,
                ),
            };
            if end_ms.is_some_and(|end| end <= start_ms) {
                return invalid(format!("{annotation_path}.endMs must be greater than startMs"));
            }
            let key = media_annotation_sort_key(object, rule, &annotation_path)?;
            if previous_key.as_ref().is_some_and(|previous| previous > &key) {
                return invalid(format!(
                    "{path}.attrs.mediaAnnotations must follow the canonical sort order"
                ));
            }
            previous_key = Some(key);
        }
    }

    let media_trim_start_ms = attrs.get("mediaTrimStartMs").and_then(Value::as_u64);
    let media_trim_end_ms = attrs.get("mediaTrimEndMs").and_then(Value::as_u64);
    if (media_trim_start_ms.is_some() || media_trim_end_ms.is_some()) && !time_based_kind(kind) {
        return invalid(format!(
            "{path}: media trim bounds are only supported for time-based attachments"
        ));
    }
    if let (Some(start), Some(end)) = (media_trim_start_ms, media_trim_end_ms)
        && end <= start
    {
        return invalid(format!(
            "{path}.attrs.mediaTrimEndMs must be greater than mediaTrimStartMs"
        ));
    }
    Ok(())
}

fn validate_horizontal_rule(
    node: &Map<String, Value>,
    _context: NodeContext,
    path: &str,
) -> DocumentValidationResult<()> {
    if node.get("content").is_some()
        || node.get("text").is_some()
        || node.get("marks").is_some()
        || node.get("attrs").is_some()
    {
        return invalid(format!(
            "{path}: horizontal rules cannot contain attributes or content"
        ));
    }
    Ok(())
}

fn validate_table(
    node: &Map<String, Value>,
    _context: NodeContext,
    path: &str,
    rule: &ValidationRule,
    analysis: &mut DocumentAnalysisBuilder,
) -> DocumentValidationResult<()> {
    ensure_structural_node_has_no_inline_fields(node, path)?;
    let content = require_spec_content(node, rule, path)?;
    validate_spec_children(content, rule, path, analysis)?;
    validate_table_geometry(content, path)
}

fn validate_table_geometry(rows: &[Value], path: &str) -> DocumentValidationResult<()> {
    let row_count = rows.len();
    let mut occupied = vec![Vec::<bool>::new(); row_count];
    let mut column_width_grid = vec![Vec::<Option<u64>>::new(); row_count];
    let mut table_width = 0_usize;

    for (row_index, row_value) in rows.iter().enumerate() {
        let row_path = format!("{path}.content[{row_index}]");
        let row = row_value
            .as_object()
            .ok_or_else(|| format!("{row_path} must be a JSON object"))?;
        let cells = required_content(row, &row_path)?;
        let mut column = 0_usize;

        for (cell_index, cell_value) in cells.iter().enumerate() {
            while occupied[row_index].get(column).copied().unwrap_or(false) {
                column += 1;
            }
            let cell_path = format!("{row_path}.content[{cell_index}]");
            let cell = cell_value
                .as_object()
                .ok_or_else(|| format!("{cell_path} must be a JSON object"))?;
            let attrs = required_attrs(cell, &cell_path)?;
            let colspan = attrs
                .get("colspan")
                .and_then(Value::as_u64)
                .ok_or_else(|| format!("{cell_path}.attrs.colspan must be a positive integer"))?
                as usize;
            let rowspan = attrs
                .get("rowspan")
                .and_then(Value::as_u64)
                .ok_or_else(|| format!("{cell_path}.attrs.rowspan must be a positive integer"))?
                as usize;

            if row_index + rowspan > row_count {
                return invalid(format!(
                    "{cell_path}.attrs.rowspan extends beyond the final table row"
                ));
            }

            for row_offset in 0..rowspan {
                let target_row = row_index + row_offset;
                let required_width = column + colspan;
                if occupied[target_row].len() < required_width {
                    occupied[target_row].resize(required_width, false);
                    column_width_grid[target_row].resize(required_width, None);
                }
                for column_offset in 0..colspan {
                    let target_column = column + column_offset;
                    if occupied[target_row][target_column] {
                        return invalid(format!(
                            "{cell_path} collides with another spanning table cell at column {}",
                            target_column + 1
                        ));
                    }
                }
            }

            let widths = attrs.get("colwidth").and_then(Value::as_array);
            for row_offset in 0..rowspan {
                let target_row = row_index + row_offset;
                for column_offset in 0..colspan {
                    let target_column = column + column_offset;
                    occupied[target_row][target_column] = true;
                    column_width_grid[target_row][target_column] = widths
                        .and_then(|values| values.get(column_offset))
                        .and_then(Value::as_u64);
                }
            }

            column += colspan;
        }
        table_width = table_width.max(occupied[row_index].len());
    }

    if table_width == 0 {
        return invalid(format!(
            "{path}: table must occupy at least one logical column"
        ));
    }
    for (row_index, row) in occupied.iter().enumerate() {
        for column in 0..table_width {
            if !row.get(column).copied().unwrap_or(false) {
                return invalid(format!(
                    "{path}.content[{row_index}] is missing logical table cell {}",
                    column + 1
                ));
            }
        }
    }

    for column in 0..table_width {
        let mut expected_width = None;
        let mut saw_unspecified_width = false;
        for row_widths in &column_width_grid {
            match row_widths.get(column).copied().flatten() {
                Some(width) => {
                    if let Some(expected) = expected_width {
                        if expected != width {
                            return invalid(format!(
                                "{path}: logical column {} has conflicting column widths",
                                column + 1
                            ));
                        }
                    } else {
                        expected_width = Some(width);
                    }
                }
                None => saw_unspecified_width = true,
            }
        }
        if expected_width.is_some() && saw_unspecified_width {
            return invalid(format!(
                "{path}: logical column {} has incomplete column-width metadata",
                column + 1
            ));
        }
    }
    Ok(())
}

fn validate_table_row(
    node: &Map<String, Value>,
    _context: NodeContext,
    path: &str,
    rule: &ValidationRule,
    analysis: &mut DocumentAnalysisBuilder,
) -> DocumentValidationResult<()> {
    ensure_structural_node_has_no_inline_fields(node, path)?;
    let content = require_spec_content(node, rule, path)?;
    validate_spec_children(content, rule, path, analysis)
}

fn validate_table_cell(
    node: &Map<String, Value>,
    _context: NodeContext,
    path: &str,
    rule: &ValidationRule,
    analysis: &mut DocumentAnalysisBuilder,
) -> DocumentValidationResult<()> {
    if node.get("text").is_some() || node.get("marks").is_some() {
        return invalid(format!(
            "{path}: table cells cannot contain inline fields directly"
        ));
    }
    let attrs = required_attrs(node, path)?;
    if let Some(widths) = attrs.get("colwidth")
        && !widths.is_null()
    {
        let widths = widths
            .as_array()
            .ok_or_else(|| format!("{path}.attrs.colwidth must be an array or null"))?;
        let colspan = attrs
            .get("colspan")
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("{path}.attrs.colspan must be an integer"))?;
        if widths.len() != colspan as usize {
            return invalid(format!(
                "{path}.attrs.colwidth must contain one width for each spanned column"
            ));
        }
        let minimum = limit("table_column_width_min");
        let maximum = limit("table_column_width_max");
        for (index, width) in widths.iter().enumerate() {
            let width = width
                .as_u64()
                .ok_or_else(|| format!("{path}.attrs.colwidth[{index}] must be an integer"))?;
            if !(minimum..=maximum).contains(&width) {
                return invalid(format!(
                    "{path}.attrs.colwidth[{index}] is outside the allowed width range"
                ));
            }
        }
    }
    let content = require_spec_content(node, rule, path)?;
    validate_spec_children(content, rule, path, analysis)
}

fn ensure_structural_node_has_no_inline_fields(
    node: &Map<String, Value>,
    path: &str,
) -> DocumentValidationResult<()> {
    if node.get("text").is_some() || node.get("marks").is_some() || node.get("attrs").is_some() {
        return invalid(format!(
            "{path}: structural table nodes cannot contain attributes or inline fields"
        ));
    }
    Ok(())
}

fn validate_hard_break(
    node: &Map<String, Value>,
    _context: NodeContext,
    path: &str,
) -> DocumentValidationResult<()> {
    if node.get("content").is_some() || node.get("text").is_some() {
        return invalid(format!("{path}: hard breaks cannot contain content"));
    }
    Ok(())
}

fn required_string<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    path: &str,
) -> DocumentValidationResult<&'a str> {
    object
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| DocumentValidationError::invalid(format!("{path}.{key} must be a string")))
}

fn required_attrs<'a>(
    object: &'a Map<String, Value>,
    path: &str,
) -> DocumentValidationResult<&'a Map<String, Value>> {
    object
        .get("attrs")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            DocumentValidationError::invalid(format!("{path}.attrs must be a JSON object"))
        })
}

fn required_content<'a>(
    object: &'a Map<String, Value>,
    path: &str,
) -> DocumentValidationResult<&'a Vec<Value>> {
    object
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            DocumentValidationError::invalid(format!("{path}.content must be an array"))
        })
}

fn optional_content<'a>(object: &'a Map<String, Value>, path: &str) -> DocumentValidationResult<&'a [Value]> {
    match object.get("content") {
        Some(value) => value
            .as_array()
            .map(Vec::as_slice)
            .ok_or_else(|| {
                DocumentValidationError::invalid(format!("{path}.content must be an array"))
            }),
        None => Ok(&[]),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn envelope(document: Value) -> String {
        let spec = &document_spec().envelope;
        serde_json::to_string(&json!({
            "schema_version": spec.schema_version,
            "editor_engine": spec.editor_engine,
            "document_schema": spec.document_schema,
            "document_schema_version": spec.document_schema_version,
            "document": document,
        }))
        .expect("serialize editor-state test fixture")
    }

    fn attachment_node(id: &str) -> Value {
        json!({
            "type": "attachment",
            "attrs": {
                "attachmentId": id,
                "kind": "file",
                "displayName": "reference.txt",
                "mediaType": "text/plain",
                "byteSize": 12,
                "caption": "",
                "altText": "",
                "alignment": "center",
                "widthPercent": 100,
                "sizeMode": "auto",
                "mediaAnnotations": null,
                "mediaTrimStartMs": null,
                "mediaTrimEndMs": null,
                "nodeSchemaVersion": 2
            }
        })
    }

    #[test]
    fn bundled_editor_states_validate() {
        validate_editor_state(EMPTY_EDITOR_STATE).expect("blank editor state should stay valid");
        validate_editor_state(include_str!("../assets/welcome-note-editor-state.json"))
            .expect("welcome editor state should stay valid");
    }

    #[test]
    fn editor_state_json_round_trip_preserves_the_canonical_document() {
        let original: Value = serde_json::from_str(
            include_str!("../assets/welcome-note-editor-state.json").trim(),
        )
        .expect("parse bundled editor state");
        let serialized = serde_json::to_string(&original).expect("serialize bundled editor state");

        validate_editor_state(&serialized).expect("round-tripped editor state should validate");
        let reparsed: Value = serde_json::from_str(&serialized).expect("reparse editor state");
        assert_eq!(reparsed, original);
    }

    #[test]
    fn analysis_collects_unique_attachment_pending_and_note_link_membership() {
        let state = envelope(json!({
            "type": "doc",
            "content": [
                attachment_node("att-a"),
                attachment_node("att-a"),
                {
                    "type": "attachment_pending",
                    "attrs": {
                        "requestId": "request-a",
                        "picker": "file",
                        "label": "Importing attachment",
                        "status": "Staging",
                        "items": null,
                        "nodeSchemaVersion": 1
                    }
                },
                {
                    "type": "paragraph",
                    "content": [
                        {
                            "type": "text",
                            "text": "note",
                            "marks": [{
                                "type": "note_link",
                                "attrs": {"noteId": "note-b"}
                            }]
                        },
                        {
                            "type": "text",
                            "text": " media",
                            "marks": [{
                                "type": "media_link",
                                "attrs": {
                                    "noteId": "note-a",
                                    "attachmentId": "att-a",
                                    "startMs": 1000,
                                    "endMs": 2500
                                }
                            }]
                        }
                    ]
                }
            ]
        }));

        let analysis = analyze_editor_state(&state).expect("analyze valid editor state");
        assert_eq!(analysis.attachment_ids, vec!["att-a".to_string()]);
        assert_eq!(
            analysis.pending_attachment_request_ids,
            vec!["request-a".to_string()]
        );
        assert_eq!(
            analysis
                .note_links
                .iter()
                .map(NoteId::as_str)
                .collect::<Vec<_>>(),
            vec!["note-a", "note-b"]
        );
    }

    #[test]
    fn invalid_contract_and_unsafe_content_are_rejected() {
        let wrong_version = json!({
            "schema_version": document_spec().envelope.schema_version + 1,
            "editor_engine": document_spec().envelope.editor_engine,
            "document_schema": document_spec().envelope.document_schema,
            "document_schema_version": document_spec().envelope.document_schema_version,
            "document": {"type": "doc", "content": [{"type": "paragraph"}]}
        });
        assert!(validate_editor_state(&wrong_version.to_string()).is_err());

        let unsafe_link = envelope(json!({
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "content": [{
                    "type": "text",
                    "text": "unsafe",
                    "marks": [{
                        "type": "link",
                        "attrs": {"href": "javascript:alert(1)"}
                    }]
                }]
            }]
        }));
        assert!(validate_editor_state(&unsafe_link).is_err());

        let unsupported_node = envelope(json!({
            "type": "doc",
            "content": [{"type": "future_node"}]
        }));
        assert!(validate_editor_state(&unsupported_node).is_err());
    }
    #[test]
    fn shared_editor_document_contract_corpus_matches_rust_validator() {
        let corpus: Value = serde_json::from_str(include_str!(
            "../editor-spec/validation-cases.json"
        ))
        .expect("parse shared editor document validation corpus");
        let cases = corpus
            .get("cases")
            .and_then(Value::as_array)
            .expect("shared validation corpus cases");
        assert!(!cases.is_empty());
        for test_case in cases {
            let name = test_case
                .get("name")
                .and_then(Value::as_str)
                .expect("validation case name");
            let expected = test_case
                .get("valid")
                .and_then(Value::as_bool)
                .expect("validation case expected result");
            let state = test_case.get("state").expect("validation case state");
            let serialized = serde_json::to_string(state).expect("serialize validation case state");
            let accepted = validate_editor_state(&serialized).is_ok();
            assert_eq!(accepted, expected, "shared document contract case: {name}");
        }
    }

}
