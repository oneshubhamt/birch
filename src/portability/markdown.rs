use std::collections::HashMap;

use super::PortabilityResult;

use serde_json::{Value, json};

use crate::{
    editor_document::parse_validated_document,
    link::is_safe_href,
    portability::{is_asset_namespace_path, safe_relative_resource_path},
};

use super::document::{
    ImportedDocument, ImportedResource, heading_node, link_mark, paragraph_node,
    portable_note_link_mark, portable_resource_node, rebase_imported_resources, simple_mark,
    text_node,
};

pub(super) fn decode(markdown: &str) -> PortabilityResult<ImportedDocument> {
    let normalized = markdown.replace("\r\n", "\n").replace('\r', "\n");
    let lines = normalized.lines().collect::<Vec<_>>();
    let mut blocks = Vec::new();
    let mut resources = Vec::new();
    let mut warnings = Vec::new();
    let mut title_hint = None;
    let mut index = 0_usize;

    if lines.first().is_some_and(|line| line.trim() == "---") {
        index = 1;
        let mut frontmatter_title = None;
        while index < lines.len() && lines[index].trim() != "---" {
            if let Some(value) = lines[index].trim().strip_prefix("title:") {
                frontmatter_title = parse_frontmatter_title(value.trim());
            }
            index += 1;
        }
        if index < lines.len() {
            title_hint = frontmatter_title;
            index += 1;
        } else {
            index = 0;
        }
    }

    while index < lines.len() {
        let line = lines[index];
        if line.trim().is_empty() {
            index += 1;
            continue;
        }

        if let Some((fence, language)) = code_fence_start(line) {
            index += 1;
            let mut code = String::new();
            while index < lines.len() && !lines[index].trim_start().starts_with(fence) {
                if !code.is_empty() {
                    code.push('\n');
                }
                code.push_str(lines[index]);
                index += 1;
            }
            if index < lines.len() {
                index += 1;
            }
            let content = if code.is_empty() {
                Vec::new()
            } else {
                vec![text_node(&code, &[])]
            };
            blocks.push(json!({"type": "code_block", "attrs": {"language": language, "lineSpacing": null}, "content": content}));
            continue;
        }

        if let Some((level, text)) = heading(line) {
            let content = parse_inline(text, &mut warnings);
            if title_hint.is_none() && level == 1 {
                title_hint = Some(inline_text(&content));
            }
            blocks.push(heading_node(level, content));
            index += 1;
            continue;
        }

        if horizontal_rule(line) {
            blocks.push(json!({"type": "horizontal_rule"}));
            index += 1;
            continue;
        }

        if let Some(resource) = standalone_resource(line, resources.len()) {
            let node = portable_resource_node(&resource);
            resources.push(resource);
            blocks.push(node);
            index += 1;
            continue;
        }

        if line.trim_start().starts_with('>') {
            let mut nested = String::new();
            while index < lines.len() && lines[index].trim_start().starts_with('>') {
                if !nested.is_empty() {
                    nested.push('\n');
                }
                let body = lines[index]
                    .trim_start()
                    .trim_start_matches('>')
                    .strip_prefix(' ')
                    .unwrap_or_else(|| lines[index].trim_start().trim_start_matches('>'));
                nested.push_str(body);
                index += 1;
            }
            let mut decoded = decode(&nested)?;
            rebase_imported_resources(
                &mut decoded.document,
                &mut decoded.resources,
                resources.len(),
            );
            resources.extend(decoded.resources);
            warnings.extend(decoded.warnings);
            let content = decoded
                .document
                .get("content")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_else(|| vec![paragraph_node(Vec::new())]);
            blocks.push(json!({"type": "blockquote", "content": content}));
            continue;
        }

        if list_item(line).is_some() {
            let (node, next) = parse_list(&lines, index, &mut warnings);
            blocks.push(node);
            index = next;
            continue;
        }

        if index + 1 < lines.len()
            && looks_like_table_row(line)
            && table_separator(lines[index + 1])
        {
            let (table, next) = parse_table(&lines, index, &mut warnings);
            blocks.push(table);
            index = next;
            continue;
        }

        let mut paragraph = String::new();
        while index < lines.len() {
            let current = lines[index];
            if current.trim().is_empty() {
                break;
            }
            if !paragraph.is_empty() && starts_block(&lines, index) {
                break;
            }
            if !paragraph.is_empty() {
                paragraph.push('\n');
            }
            paragraph.push_str(current);
            index += 1;
        }
        let mut content = Vec::new();
        for (line_index, part) in paragraph.lines().enumerate() {
            if line_index > 0 {
                content.push(json!({"type": "hard_break"}));
            }
            content.extend(parse_inline(part, &mut warnings));
        }
        blocks.push(paragraph_node(content));
    }

    if title_hint.is_some()
        && blocks.first().is_some_and(|node| {
            node.get("type").and_then(Value::as_str) == Some("heading")
                && node
                    .get("attrs")
                    .and_then(Value::as_object)
                    .and_then(|attrs| attrs.get("level"))
                    .and_then(Value::as_u64)
                    == Some(1)
        })
    {
        blocks.remove(0);
    }
    if blocks.is_empty() {
        blocks.push(paragraph_node(Vec::new()));
    }
    Ok(ImportedDocument {
        title_hint: title_hint.filter(|title| !title.trim().is_empty()),
        document: json!({"type": "doc", "content": blocks}),
        resources,
        warnings,
    })
}

pub(super) fn encode(
    title: &str,
    editor_state: &str,
    attachment_paths: &HashMap<String, String>,
    note_paths: &HashMap<String, String>,
) -> PortabilityResult<String> {
    let document = parse_validated_document(editor_state).map_err(|error| error.to_string())?;
    let mut output = String::new();
    if !title.trim().is_empty() {
        output.push_str("---\ntitle: ");
        output.push_str(&serde_json::to_string(title.trim()).map_err(|error| error.to_string())?);
        output.push_str("\n---\n\n");
    }
    if let Some(content) = document.get("content").and_then(Value::as_array) {
        for node in content {
            render_block(node, &mut output, attachment_paths, note_paths, "")?;
        }
    }
    while output.ends_with("\n\n\n") {
        output.pop();
    }
    if !output.ends_with('\n') {
        output.push('\n');
    }
    Ok(output)
}

fn render_block(
    node: &Value,
    output: &mut String,
    attachment_paths: &HashMap<String, String>,
    note_paths: &HashMap<String, String>,
    prefix: &str,
) -> PortabilityResult<()> {
    match node.get("type").and_then(Value::as_str) {
        Some("paragraph") => {
            output.push_str(prefix);
            output.push_str(&render_inline(node, note_paths));
            output.push_str("\n\n");
        }
        Some("heading") => {
            let level = node
                .get("attrs")
                .and_then(Value::as_object)
                .and_then(|a| a.get("level"))
                .and_then(Value::as_u64)
                .unwrap_or(1)
                .clamp(1, 6);
            output.push_str(prefix);
            output.push_str(&"#".repeat(level as usize));
            output.push(' ');
            output.push_str(&render_inline(node, note_paths));
            output.push_str("\n\n");
        }
        Some("blockquote") => {
            let mut nested = String::new();
            if let Some(content) = node.get("content").and_then(Value::as_array) {
                for child in content {
                    render_block(child, &mut nested, attachment_paths, note_paths, "")?;
                }
            }
            for line in nested.trim_end().lines() {
                output.push_str(prefix);
                output.push_str("> ");
                output.push_str(line);
                output.push('\n');
            }
            output.push('\n');
        }
        Some("code_block") => {
            let language = node
                .get("attrs")
                .and_then(Value::as_object)
                .and_then(|a| a.get("language"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            output.push_str(prefix);
            output.push_str("```");
            output.push_str(language);
            output.push('\n');
            output.push_str(&plain_node_text(node));
            output.push('\n');
            output.push_str(prefix);
            output.push_str("```\n\n");
        }
        Some("horizontal_rule") => {
            output.push_str(prefix);
            output.push_str("---\n\n");
        }
        Some("attachment") => {
            let attrs = node
                .get("attrs")
                .and_then(Value::as_object)
                .ok_or_else(|| "attachment attrs are missing".to_string())?;
            let id = attrs
                .get("attachmentId")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let name = attrs
                .get("displayName")
                .and_then(Value::as_str)
                .unwrap_or("Attachment");
            let kind = attrs.get("kind").and_then(Value::as_str).unwrap_or("file");
            if let Some(path) = attachment_paths.get(id) {
                output.push_str(prefix);
                if kind == "image" {
                    output.push_str("![");
                } else {
                    output.push('[');
                }
                output.push_str(&escape_link_label(name));
                output.push_str("](");
                output.push_str(&escape_link_destination(path));
                output.push_str(")\n\n");
            } else {
                output.push_str(prefix);
                output.push_str("[Attachment: ");
                output.push_str(&escape_inline(name));
                output.push_str("]\n\n");
            }
        }
        Some("bullet_list" | "ordered_list" | "task_list") => {
            render_list(node, output, attachment_paths, note_paths, prefix)?
        }
        Some("table") => render_table(node, output)?,
        Some("math_display") => {
            output.push_str(prefix);
            output.push_str("$$\n");
            output.push_str(&plain_node_text(node));
            output.push_str("\n$$\n\n");
        }
        Some("attachment_pending") => {}
        _ => {
            let text = plain_node_text(node);
            if !text.is_empty() {
                output.push_str(prefix);
                output.push_str(&escape_inline(&text));
                output.push_str("\n\n");
            }
        }
    }
    Ok(())
}

fn render_list(
    node: &Value,
    output: &mut String,
    attachment_paths: &HashMap<String, String>,
    note_paths: &HashMap<String, String>,
    prefix: &str,
) -> PortabilityResult<()> {
    let kind = node
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("bullet_list");
    let order = node
        .get("attrs")
        .and_then(Value::as_object)
        .and_then(|a| a.get("order"))
        .and_then(Value::as_u64)
        .unwrap_or(1);
    if let Some(items) = node.get("content").and_then(Value::as_array) {
        for (index, item) in items.iter().enumerate() {
            let marker = match kind {
                "ordered_list" => format!("{}. ", order + index as u64),
                "task_list" => {
                    let checked = item
                        .get("attrs")
                        .and_then(Value::as_object)
                        .and_then(|a| a.get("checked"))
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    format!("- [{}] ", if checked { "x" } else { " " })
                }
                _ => "- ".to_string(),
            };
            let content = item
                .get("content")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if let Some(first) = content.first() {
                output.push_str(prefix);
                output.push_str(&marker);
                output.push_str(&render_inline(first, note_paths));
                output.push('\n');
            }
            let nested_prefix = format!("{prefix}  ");
            for child in content.iter().skip(1) {
                render_block(child, output, attachment_paths, note_paths, &nested_prefix)?;
            }
        }
    }
    output.push('\n');
    Ok(())
}

fn render_table(node: &Value, output: &mut String) -> PortabilityResult<()> {
    let rows = node
        .get("content")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if rows.is_empty() {
        return Ok(());
    }
    let mut rendered = Vec::new();
    for row in rows {
        let cells = row
            .get("content")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        rendered.push(
            cells
                .iter()
                .map(|cell| escape_table_cell(&plain_node_text(cell)))
                .collect::<Vec<_>>(),
        );
    }
    let width = rendered.iter().map(Vec::len).max().unwrap_or(0);
    if width == 0 {
        return Ok(());
    }
    let first = rendered
        .first()
        .cloned()
        .unwrap_or_else(|| vec![String::new(); width]);
    output.push('|');
    for index in 0..width {
        output.push(' ');
        output.push_str(first.get(index).map(String::as_str).unwrap_or(""));
        output.push_str(" |");
    }
    output.push('\n');
    output.push('|');
    for _ in 0..width {
        output.push_str(" --- |");
    }
    output.push('\n');
    for row in rendered.iter().skip(1) {
        output.push('|');
        for index in 0..width {
            output.push(' ');
            output.push_str(row.get(index).map(String::as_str).unwrap_or(""));
            output.push_str(" |");
        }
        output.push('\n');
    }
    output.push('\n');
    Ok(())
}

fn render_inline(node: &Value, note_paths: &HashMap<String, String>) -> String {
    let mut output = String::new();
    if let Some(content) = node.get("content").and_then(Value::as_array) {
        for child in content {
            match child.get("type").and_then(Value::as_str) {
                Some("text") => output.push_str(&render_text_with_marks(child, note_paths)),
                Some("hard_break") => output.push_str("  \n"),
                Some("math_inline") => {
                    output.push('$');
                    output.push_str(&plain_node_text(child));
                    output.push('$');
                }
                _ => output.push_str(&escape_inline(&plain_node_text(child))),
            }
        }
    }
    output
}

fn render_text_with_marks(node: &Value, note_paths: &HashMap<String, String>) -> String {
    let mut text = escape_inline(node.get("text").and_then(Value::as_str).unwrap_or_default());
    let marks = node
        .get("marks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for mark in marks.iter().rev() {
        match mark.get("type").and_then(Value::as_str) {
            Some("strong") => text = format!("**{text}**"),
            Some("em") => text = format!("*{text}*"),
            Some("strike") => text = format!("~~{text}~~"),
            Some("link") => {
                let href = mark
                    .get("attrs")
                    .and_then(Value::as_object)
                    .and_then(|a| a.get("href"))
                    .and_then(Value::as_str)
                    .unwrap_or("#");
                text = format!("[{text}]({})", escape_link_destination(href));
            }
            Some("note_link") => {
                let id = mark
                    .get("attrs")
                    .and_then(Value::as_object)
                    .and_then(|a| a.get("noteId"))
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let target = note_paths
                    .get(id)
                    .map(String::as_str)
                    .unwrap_or_else(|| "#");
                text = format!("[{text}]({})", escape_link_destination(target));
            }
            _ => {}
        }
    }
    text
}

fn parse_inline(input: &str, warnings: &mut Vec<String>) -> Vec<Value> {
    let mut nodes = Vec::new();
    let mut rest = input;
    while !rest.is_empty() {
        if let Some(target) = rest
            .strip_prefix("[[")
            .and_then(|body| body.find("]]").map(|end| (&body[..end], &body[end + 2..])))
        {
            let (label, tail) = target;
            if !label.is_empty() {
                nodes.push(text_node(label, &[portable_note_link_mark(label)]));
                rest = tail;
                continue;
            }
        }
        if rest.starts_with("**")
            && let Some(end) = rest[2..].find("**")
        {
            let body = &rest[2..2 + end];
            nodes.push(text_node(body, &[simple_mark("strong")]));
            rest = &rest[2 + end + 2..];
            continue;
        }
        if rest.starts_with("~~")
            && let Some(end) = rest[2..].find("~~")
        {
            let body = &rest[2..2 + end];
            nodes.push(text_node(body, &[simple_mark("strike")]));
            rest = &rest[2 + end + 2..];
            continue;
        }
        if rest.starts_with('*')
            && let Some(end) = rest[1..].find('*')
        {
            let body = &rest[1..1 + end];
            nodes.push(text_node(body, &[simple_mark("em")]));
            rest = &rest[1 + end + 1..];
            continue;
        }
        if rest.starts_with('`')
            && let Some(end) = rest[1..].find('`')
        {
            let body = &rest[1..1 + end];
            nodes.push(text_node(body, &[]));
            rest = &rest[1 + end + 1..];
            continue;
        }
        if rest.starts_with('[')
            && let Some(close) = rest.find("](")
            && let Some(end) = rest[close + 2..].find(')')
        {
            let label = &rest[1..close];
            let target = &rest[close + 2..close + 2 + end];
            let decoded_target = decode_local_destination(target);
            let mark = if is_probable_note_path(&decoded_target) {
                Some(portable_note_link_mark(&decoded_target))
            } else if is_safe_href(target) {
                Some(link_mark(target, None))
            } else {
                warnings.push(format!("Dropped unsafe Markdown link target: {target}"));
                None
            };
            nodes.push(text_node(label, &mark.into_iter().collect::<Vec<_>>()));
            rest = &rest[close + 2 + end + 1..];
            continue;
        }
        let next = next_special(rest);
        let (text, tail) = rest.split_at(next.max(1).min(rest.len()));
        nodes.push(text_node(&unescape_inline(text), &[]));
        rest = tail;
    }
    nodes
}

fn standalone_resource(line: &str, index: usize) -> Option<ImportedResource> {
    let trimmed = line.trim();
    let (image, body) = if let Some(body) = trimmed.strip_prefix("![") {
        (true, body)
    } else {
        (false, trimmed.strip_prefix('[')?)
    };
    let close = body.find("](")?;
    let label = &body[..close];
    let tail = &body[close + 2..];
    let end = tail.find(')')?;
    if !tail[end + 1..].trim().is_empty() {
        return None;
    }
    let target = &tail[..end];
    let decoded_target = decode_local_destination(target);
    let path = safe_relative_resource_path(&decoded_target)?;
    if !image && is_probable_note_path(&decoded_target) && !is_asset_namespace_path(&path) {
        return None;
    }
    Some(ImportedResource {
        token: format!("resource-{index}"),
        path: path.to_string_lossy().replace('\\', "/"),
        display_name: if label.trim().is_empty() {
            path.file_name()?.to_string_lossy().to_string()
        } else {
            label.trim().to_string()
        },
        image,
    })
}

fn parse_list(lines: &[&str], start: usize, warnings: &mut Vec<String>) -> (Value, usize) {
    let first = list_item(lines[start]).expect("list start must parse");
    let list_type = match first.kind {
        ListKind::Ordered(_) => "ordered_list",
        ListKind::Task(_) => "task_list",
        ListKind::Bullet => "bullet_list",
    };
    let mut items = Vec::new();
    let mut index = start;
    let mut order = match first.kind {
        ListKind::Ordered(value) => value,
        _ => 1,
    };
    while index < lines.len() {
        let Some(item) = list_item(lines[index]) else {
            break;
        };
        if item.list_type() != list_type {
            break;
        }
        let content = parse_inline(item.body, warnings);
        let paragraph = paragraph_node(content);
        let node = match item.kind {
            ListKind::Task(checked) => {
                json!({"type": "task_item", "attrs": {"checked": checked}, "content": [paragraph]})
            }
            _ => json!({"type": "list_item", "content": [paragraph]}),
        };
        items.push(node);
        if let ListKind::Ordered(value) = item.kind {
            order = order.min(value);
        }
        index += 1;
    }
    let node = match list_type {
        "ordered_list" => {
            json!({"type": "ordered_list", "attrs": {"order": order, "style": "auto", "restart": false}, "content": items})
        }
        "task_list" => json!({"type": "task_list", "content": items}),
        _ => json!({"type": "bullet_list", "attrs": {"marker": "disc"}, "content": items}),
    };
    (node, index)
}

fn parse_table(lines: &[&str], start: usize, warnings: &mut Vec<String>) -> (Value, usize) {
    let header = split_table_row(lines[start]);
    let mut rows = vec![table_row(&header, true, warnings)];
    let mut index = start + 2;
    while index < lines.len()
        && looks_like_table_row(lines[index])
        && !lines[index].trim().is_empty()
    {
        rows.push(table_row(&split_table_row(lines[index]), false, warnings));
        index += 1;
    }
    (json!({"type": "table", "content": rows}), index)
}

fn table_row(cells: &[String], header: bool, warnings: &mut Vec<String>) -> Value {
    let kind = if header { "table_header" } else { "table_cell" };
    let cells = cells
        .iter()
        .map(|cell| {
            json!({
                "type": kind,
                "attrs": {"colspan": 1, "rowspan": 1, "colwidth": null},
                "content": [paragraph_node(parse_inline(cell.trim(), warnings))]
            })
        })
        .collect::<Vec<_>>();
    json!({"type": "table_row", "content": cells})
}

fn split_table_row(line: &str) -> Vec<String> {
    line.trim()
        .trim_matches('|')
        .split('|')
        .map(|value| value.replace("\\|", "|").trim().to_string())
        .collect()
}

fn heading(line: &str) -> Option<(u8, &str)> {
    let trimmed = line.trim_start();
    let count = trimmed
        .chars()
        .take_while(|character| *character == '#')
        .count();
    if !(1..=6).contains(&count) || !trimmed[count..].starts_with(' ') {
        return None;
    }
    Some((count as u8, trimmed[count + 1..].trim_end()))
}

fn horizontal_rule(line: &str) -> bool {
    let value = line.trim();
    matches!(value, "---" | "***" | "___")
}

fn code_fence_start(line: &str) -> Option<(&'static str, Option<String>)> {
    let trimmed = line.trim_start();
    if let Some(rest) = trimmed.strip_prefix("```") {
        return Some((
            "```",
            (!rest.trim().is_empty()).then(|| rest.trim().to_string()),
        ));
    }
    if let Some(rest) = trimmed.strip_prefix("~~~") {
        return Some((
            "~~~",
            (!rest.trim().is_empty()).then(|| rest.trim().to_string()),
        ));
    }
    None
}

#[derive(Clone, Copy)]
enum ListKind {
    Bullet,
    Ordered(u64),
    Task(bool),
}
struct ListItem<'a> {
    kind: ListKind,
    body: &'a str,
}
impl ListItem<'_> {
    fn list_type(&self) -> &'static str {
        match self.kind {
            ListKind::Bullet => "bullet_list",
            ListKind::Ordered(_) => "ordered_list",
            ListKind::Task(_) => "task_list",
        }
    }
}
fn list_item(line: &str) -> Option<ListItem<'_>> {
    let trimmed = line.trim_start();
    if let Some(rest) = trimmed.strip_prefix("- [ ] ") {
        return Some(ListItem {
            kind: ListKind::Task(false),
            body: rest,
        });
    }
    if let Some(rest) = trimmed
        .strip_prefix("- [x] ")
        .or_else(|| trimmed.strip_prefix("- [X] "))
    {
        return Some(ListItem {
            kind: ListKind::Task(true),
            body: rest,
        });
    }
    for marker in ["- ", "* ", "+ "] {
        if let Some(rest) = trimmed.strip_prefix(marker) {
            return Some(ListItem {
                kind: ListKind::Bullet,
                body: rest,
            });
        }
    }
    let digits = trimmed.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 && trimmed[digits..].starts_with(". ") {
        let order = trimmed[..digits].parse().ok()?;
        return Some(ListItem {
            kind: ListKind::Ordered(order),
            body: &trimmed[digits + 2..],
        });
    }
    None
}

fn starts_block(lines: &[&str], index: usize) -> bool {
    let line = lines[index];
    heading(line).is_some()
        || code_fence_start(line).is_some()
        || horizontal_rule(line)
        || list_item(line).is_some()
        || line.trim_start().starts_with('>')
        || standalone_resource(line, 0).is_some()
        || (index + 1 < lines.len()
            && looks_like_table_row(line)
            && table_separator(lines[index + 1]))
}

fn looks_like_table_row(line: &str) -> bool {
    line.contains('|')
}
fn table_separator(line: &str) -> bool {
    let cells = split_table_row(line);
    !cells.is_empty()
        && cells.iter().all(|cell| {
            let value = cell.trim().trim_matches(':');
            value.len() >= 3 && value.chars().all(|c| c == '-')
        })
}

fn decode_local_destination(value: &str) -> String {
    if value.contains("://") || value.starts_with("data:") {
        return value.to_string();
    }
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0_usize;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) =
                (hex_nibble(bytes[index + 1]), hex_nibble(bytes[index + 2]))
        {
            output.push((high << 4) | low);
            index += 3;
            continue;
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8(output).unwrap_or_else(|_| value.to_string())
}

fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn is_probable_note_path(target: &str) -> bool {
    let clean = target
        .split('#')
        .next()
        .unwrap_or(target)
        .to_ascii_lowercase();
    clean.ends_with(".md")
        || clean.ends_with(".markdown")
        || clean.ends_with(".html")
        || clean.ends_with(".htm")
        || (!target.contains("://") && target.starts_with("[["))
}

fn parse_frontmatter_title(value: &str) -> Option<String> {
    if value.is_empty() {
        return None;
    }
    if value.starts_with('"') {
        return serde_json::from_str::<String>(value)
            .ok()
            .filter(|title| !title.trim().is_empty());
    }
    let value = value.trim_matches('\'').trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn next_special(value: &str) -> usize {
    ["[[", "**", "~~", "*", "`", "["]
        .iter()
        .filter_map(|needle| value.find(needle))
        .min()
        .unwrap_or(value.len())
}

fn inline_text(content: &[Value]) -> String {
    content
        .iter()
        .map(plain_node_text)
        .collect::<Vec<_>>()
        .join("")
}
fn plain_node_text(node: &Value) -> String {
    if node.get("type").and_then(Value::as_str) == Some("text") {
        return node
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
    }
    if node.get("type").and_then(Value::as_str) == Some("hard_break") {
        return "\n".to_string();
    }
    node.get("content")
        .and_then(Value::as_array)
        .map(|content| {
            content
                .iter()
                .map(plain_node_text)
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
}
fn unescape_inline(value: &str) -> String {
    let mut out = String::new();
    let mut escaped = false;
    for c in value.chars() {
        if escaped {
            out.push(c);
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else {
            out.push(c);
        }
    }
    if escaped {
        out.push('\\');
    }
    out
}
fn escape_inline(value: &str) -> String {
    value
        .chars()
        .flat_map(|c| {
            if matches!(c, '\\' | '*' | '_' | '[' | ']' | '`' | '~') {
                vec!['\\', c]
            } else {
                vec![c]
            }
        })
        .collect()
}
fn escape_link_label(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace(['\r', '\n'], " ")
}
fn escape_link_destination(value: &str) -> String {
    value
        .replace(' ', "%20")
        .replace('(', "%28")
        .replace(')', "%29")
}
fn escape_table_cell(value: &str) -> String {
    value.replace('|', "\\|").replace(['\r', '\n'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portability::document::canonical_editor_state;

    #[test]
    fn markdown_import_builds_valid_common_document() {
        let decoded = decode("# Title\n\nA **bold** paragraph.\n\n- one\n- two\n").expect("decode");
        canonical_editor_state(decoded.document).expect("canonical document");
        assert_eq!(decoded.title_hint.as_deref(), Some("Title"));
    }

    #[test]
    fn markdown_resource_is_a_placeholder_until_storage_resolves_it() {
        let decoded = decode("![Map](note.assets/map.png)\n").expect("decode");
        assert_eq!(decoded.resources.len(), 1);
        assert_eq!(decoded.resources[0].path, "note.assets/map.png");
    }

    #[test]
    fn rustnote_attachment_resources_round_trip_through_exported_assets() {
        let state = canonical_editor_state(json!({
            "type": "doc",
            "content": [{
                "type": "attachment",
                "attrs": {
                    "attachmentId": "a1",
                    "kind": "image",
                    "displayName": "image.png",
                    "byteSize": 1,
                    "caption": "",
                    "altText": "",
                    "alignment": "center",
                    "widthPercent": 100,
                    "sizeMode": "auto",
                    "nodeSchemaVersion": 2
                }
            }]
        }))
        .expect("canonical attachment fixture");
        let attachments = HashMap::from([("a1".to_string(), "Topic.assets/image.png".to_string())]);
        let exported = encode("Topic", &state, &attachments, &HashMap::new()).expect("export");
        assert!(exported.contains("Topic.assets/image.png"));
        let imported = decode(&exported).expect("import exported document");
        assert_eq!(imported.resources.len(), 1);
        assert_eq!(imported.resources[0].path, "Topic.assets/image.png");
    }

    #[test]
    fn encoded_local_asset_destinations_are_decoded_before_resolution() {
        let decoded = decode("![Map](Topic.assets/Map%201%20%282%29.png)\n")
            .expect("decode encoded asset path");
        assert_eq!(decoded.resources.len(), 1);
        assert_eq!(decoded.resources[0].path, "Topic.assets/Map 1 (2).png");
    }

    #[test]
    fn note_like_files_inside_asset_namespaces_remain_attachments() {
        let decoded = decode("[Reference](Topic.assets/reference.md)\n")
            .expect("decode note-like attachment");
        assert_eq!(decoded.resources.len(), 1);
        assert_eq!(decoded.resources[0].path, "Topic.assets/reference.md");
    }

    #[test]
    fn nested_blockquote_resources_receive_unique_tokens() {
        let decoded =
            decode("![Top](note.assets/top.png)\n\n> ![Quoted](note.assets/quoted.png)\n")
                .expect("decode nested resources");
        assert_eq!(decoded.resources.len(), 2);
        assert_ne!(decoded.resources[0].token, decoded.resources[1].token);
        assert_eq!(decoded.resources[0].token, "resource-0");
        assert_eq!(decoded.resources[1].token, "resource-1");
        let serialized = serde_json::to_string(&decoded.document).expect("serialize document");
        assert!(serialized.contains("resource-0"));
        assert!(serialized.contains("resource-1"));
    }

    #[test]
    fn rustnote_frontmatter_title_is_metadata_not_document_content() {
        let decoded = decode("---\ntitle: \"Parliament\"\n---\n\nBody\n").expect("decode");
        assert_eq!(decoded.title_hint.as_deref(), Some("Parliament"));
        let text = super::plain_node_text(&decoded.document);
        assert!(!text.contains("Parliament"));
        assert!(text.contains("Body"));
    }
}
