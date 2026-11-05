use std::collections::HashMap;

use serde_json::{Map, Value, json};

use super::PortabilityResult;

use crate::{
    domain::NoteId,
    editor_document::{parse_validated_document, validate_editor_state},
    editor_document_spec::document_spec,
};

const PORTABLE_RESOURCE_NODE: &str = "rustnote_portable_resource";
const PORTABLE_NOTE_LINK_MARK: &str = "rustnote_portable_note_link";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ImportedResource {
    pub(crate) token: String,
    pub(crate) path: String,
    pub(crate) display_name: String,
    pub(crate) image: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct ImportedDocument {
    pub(crate) title_hint: Option<String>,
    pub(crate) document: Value,
    pub(crate) resources: Vec<ImportedResource>,
    pub(crate) warnings: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct ResourceResolution {
    pub(crate) attachment_id: String,
    pub(crate) kind: String,
    pub(crate) display_name: String,
    pub(crate) media_type: Option<String>,
    pub(crate) byte_size: i64,
}

pub(crate) fn plain_text_document(text: &str) -> ImportedDocument {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut blocks = normalized
        .split("\n\n")
        .map(|paragraph| {
            let mut content = Vec::new();
            for (index, line) in paragraph.split('\n').enumerate() {
                if index > 0 {
                    content.push(json!({"type": "hard_break"}));
                }
                if !line.is_empty() {
                    content.push(text_node(line, &[]));
                }
            }
            paragraph_node(content)
        })
        .collect::<Vec<_>>();
    if blocks.is_empty() {
        blocks.push(paragraph_node(Vec::new()));
    }
    ImportedDocument {
        title_hint: None,
        document: json!({"type": "doc", "content": blocks}),
        resources: Vec::new(),
        warnings: Vec::new(),
    }
}

pub(crate) fn canonical_editor_state(document: Value) -> PortabilityResult<String> {
    let spec = &document_spec().envelope;
    let serialized = serde_json::to_string(&json!({
        "schema_version": spec.schema_version,
        "editor_engine": spec.editor_engine,
        "document_schema": spec.document_schema,
        "document_schema_version": spec.document_schema_version,
        "document": document,
    }))
    .map_err(|error| error.to_string())?;
    validate_editor_state(&serialized).map_err(|error| error.to_string())?;
    Ok(serialized)
}

pub(crate) fn validated_document(editor_state: &str) -> PortabilityResult<Value> {
    parse_validated_document(editor_state)
        .map_err(|error| super::PortabilityCodecError::new(error.to_string()))
}

pub(crate) fn paragraph_node(content: Vec<Value>) -> Value {
    json!({
        "type": "paragraph",
        "attrs": textblock_attrs(),
        "content": content,
    })
}

pub(crate) fn heading_node(level: u8, content: Vec<Value>) -> Value {
    let mut attrs = textblock_attrs();
    attrs.insert("level".to_string(), json!(level.clamp(1, 6)));
    json!({"type": "heading", "attrs": attrs, "content": content})
}

pub(crate) fn textblock_attrs() -> Map<String, Value> {
    Map::from_iter([
        ("lineSpacing".to_string(), Value::Null),
        ("textAlignment".to_string(), json!("left")),
        ("leftIndent".to_string(), json!(0)),
        ("rightIndent".to_string(), json!(0)),
        ("firstLineIndent".to_string(), json!(0)),
        ("dropCap".to_string(), json!(false)),
    ])
}

pub(crate) fn text_node(text: &str, marks: &[Value]) -> Value {
    let mut node = Map::new();
    node.insert("type".to_string(), json!("text"));
    node.insert("text".to_string(), json!(text));
    if !marks.is_empty() {
        node.insert("marks".to_string(), Value::Array(marks.to_vec()));
    }
    Value::Object(node)
}

pub(crate) fn simple_mark(kind: &str) -> Value {
    json!({"type": kind})
}

pub(crate) fn link_mark(href: &str, title: Option<&str>) -> Value {
    json!({"type": "link", "attrs": {"href": href, "title": title}})
}

pub(crate) fn portable_note_link_mark(target: &str) -> Value {
    json!({"type": PORTABLE_NOTE_LINK_MARK, "attrs": {"target": target}})
}

pub(crate) fn portable_resource_node(resource: &ImportedResource) -> Value {
    json!({
        "type": PORTABLE_RESOURCE_NODE,
        "attrs": {
            "token": resource.token,
        }
    })
}

pub(crate) fn rebase_imported_resources(
    document: &mut Value,
    resources: &mut [ImportedResource],
    start_index: usize,
) {
    let mut mapping = HashMap::<String, String>::new();
    for (offset, resource) in resources.iter_mut().enumerate() {
        let new_token = format!("resource-{}", start_index.saturating_add(offset));
        mapping.insert(resource.token.clone(), new_token.clone());
        resource.token = new_token;
    }

    fn walk(value: &mut Value, mapping: &HashMap<String, String>) {
        let Some(object) = value.as_object_mut() else { return; };
        if object.get("type").and_then(Value::as_str) == Some(PORTABLE_RESOURCE_NODE)
            && let Some(attrs) = object.get_mut("attrs").and_then(Value::as_object_mut)
            && let Some(old) = attrs.get("token").and_then(Value::as_str).map(str::to_string)
            && let Some(new) = mapping.get(&old)
        {
            attrs.insert("token".to_string(), json!(new));
        }
        if let Some(content) = object.get_mut("content").and_then(Value::as_array_mut) {
            for child in content {
                walk(child, mapping);
            }
        }
    }
    walk(document, &mapping);
}

pub(crate) fn resolve_import_placeholders(
    document: &mut Value,
    resources: &HashMap<String, ResourceResolution>,
) -> PortabilityResult<()> {
    fn walk(value: &mut Value, resources: &HashMap<String, ResourceResolution>) -> PortabilityResult<()> {
        let Some(object) = value.as_object_mut() else { return Ok(()); };
        if object.get("type").and_then(Value::as_str) == Some(PORTABLE_RESOURCE_NODE) {
            let token = object
                .get("attrs")
                .and_then(Value::as_object)
                .and_then(|attrs| attrs.get("token"))
                .and_then(Value::as_str)
                .ok_or_else(|| "portable resource node is missing its token".to_string())?;
            let resolution = resources
                .get(token)
                .ok_or_else(|| format!("portable resource {token} was not resolved"))?;
            *value = json!({
                "type": "attachment",
                "attrs": {
                    "attachmentId": resolution.attachment_id,
                    "kind": resolution.kind,
                    "displayName": resolution.display_name,
                    "mediaType": resolution.media_type,
                    "byteSize": resolution.byte_size,
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
            });
            return Ok(());
        }
        if let Some(content) = object.get_mut("content").and_then(Value::as_array_mut) {
            for child in content {
                walk(child, resources)?;
            }
        }
        Ok(())
    }
    walk(document, resources)
}

pub(crate) fn rewrite_note_link_placeholders(
    document: &mut Value,
    resolve: &impl Fn(&str) -> Option<NoteId>,
) {
    fn walk(value: &mut Value, resolve: &impl Fn(&str) -> Option<NoteId>) {
        let Some(object) = value.as_object_mut() else { return; };
        if let Some(marks) = object.get_mut("marks").and_then(Value::as_array_mut) {
            for mark in marks.iter_mut() {
                let Some(mark_object) = mark.as_object_mut() else { continue; };
                if mark_object.get("type").and_then(Value::as_str) != Some(PORTABLE_NOTE_LINK_MARK) {
                    continue;
                }
                let target = mark_object
                    .get("attrs")
                    .and_then(Value::as_object)
                    .and_then(|attrs| attrs.get("target"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                if let Some(note_id) = resolve(&target) {
                    *mark = json!({"type": "note_link", "attrs": {"noteId": note_id.as_str()}});
                } else {
                    *mark = json!({"type": "link", "attrs": {"href": target, "title": null}});
                }
            }
        }
        if let Some(content) = object.get_mut("content").and_then(Value::as_array_mut) {
            for child in content {
                walk(child, resolve);
            }
        }
    }
    walk(document, resolve);
}

pub(crate) fn apply_textblock_layout_from_attrs(node: &mut Value, attrs: &HashMap<String, String>) {
    let Some(node_attrs) = node.get_mut("attrs").and_then(Value::as_object_mut) else { return; };
    if let Some(value) = attrs.get("data-rustnote-line-spacing")
        && matches!(value.as_str(), "1" | "1.15" | "1.5" | "1.75" | "2")
    {
        node_attrs.insert("lineSpacing".to_string(), json!(value));
    }
    if let Some(value) = attrs.get("data-rustnote-text-alignment")
        && matches!(value.as_str(), "left" | "center" | "right" | "justify")
    {
        node_attrs.insert("textAlignment".to_string(), json!(value));
    }
    for (source, target, maximum) in [
        ("data-rustnote-left-indent", "leftIndent", 144_i64),
        ("data-rustnote-right-indent", "rightIndent", 144_i64),
        ("data-rustnote-first-line-indent", "firstLineIndent", 72_i64),
    ] {
        if let Some(value) = attrs.get(source).and_then(|value| value.parse::<i64>().ok()) {
            node_attrs.insert(target.to_string(), json!(value.clamp(0, maximum)));
        }
    }
    if attrs.get("data-rustnote-drop-cap").is_some_and(|value| value == "true") {
        node_attrs.insert("firstLineIndent".to_string(), json!(0));
        node_attrs.insert("dropCap".to_string(), json!(true));
    }
}

pub(crate) fn inline_plain_text(node: &Value) -> String {
    let mut output = String::new();
    collect_inline_text(node, &mut output);
    output
}

fn collect_inline_text(node: &Value, output: &mut String) {
    match node.get("type").and_then(Value::as_str) {
        Some("text") => output.push_str(node.get("text").and_then(Value::as_str).unwrap_or_default()),
        Some("hard_break") => output.push('\n'),
        _ => {
            if let Some(content) = node.get("content").and_then(Value::as_array) {
                for child in content {
                    collect_inline_text(child, output);
                }
            }
        }
    }
}

pub(crate) fn remap_canonical_editor_state(
    editor_state: &str,
    note_ids: &HashMap<String, NoteId>,
    attachment_ids: &HashMap<String, String>,
) -> PortabilityResult<String> {
    let mut document = validated_document(editor_state)?;
    fn walk(
        value: &mut Value,
        note_ids: &HashMap<String, NoteId>,
        attachment_ids: &HashMap<String, String>,
    ) {
        let Some(object) = value.as_object_mut() else { return; };
        match object.get("type").and_then(Value::as_str) {
            Some("attachment") => {
                if let Some(attrs) = object.get_mut("attrs").and_then(Value::as_object_mut)
                    && let Some(old) = attrs.get("attachmentId").and_then(Value::as_str).map(str::to_string)
                    && let Some(new) = attachment_ids.get(&old)
                {
                    attrs.insert("attachmentId".to_string(), json!(new));
                }
            }
            Some("text") => {
                if let Some(marks) = object.get_mut("marks").and_then(Value::as_array_mut) {
                    for mark in marks {
                        let Some(mark_object) = mark.as_object_mut() else { continue; };
                        let kind = mark_object.get("type").and_then(Value::as_str).unwrap_or_default().to_string();
                        if let Some(attrs) = mark_object.get_mut("attrs").and_then(Value::as_object_mut) {
                            if matches!(kind.as_str(), "note_link" | "media_link")
                                && let Some(old) = attrs.get("noteId").and_then(Value::as_str).map(str::to_string)
                                && let Some(new) = note_ids.get(&old)
                            {
                                attrs.insert("noteId".to_string(), json!(new.as_str()));
                            }
                            if kind == "media_link"
                                && let Some(old) = attrs.get("attachmentId").and_then(Value::as_str).map(str::to_string)
                                && let Some(new) = attachment_ids.get(&old)
                            {
                                attrs.insert("attachmentId".to_string(), json!(new));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        if let Some(content) = object.get_mut("content").and_then(Value::as_array_mut) {
            for child in content { walk(child, note_ids, attachment_ids); }
        }
    }
    walk(&mut document, note_ids, attachment_ids);
    canonical_editor_state(document)
}
