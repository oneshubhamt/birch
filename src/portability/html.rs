use std::collections::HashMap;

use serde_json::Value;

use crate::{
    editor_document::parse_validated_document,
    link::is_safe_href,
    portability::safe_relative_resource_path,
};

use super::{
    PortabilityResult,
    document::{ImportedDocument, apply_textblock_layout_from_attrs},
    markdown,
};

pub(super) fn decode(html: &str) -> PortabilityResult<ImportedDocument> {
    let trusted_rustnote_html = is_rustnote_html(html);
    let exported_title = trusted_rustnote_html.then(|| html_title(html)).flatten();
    let (markdown_text, layouts, warnings) = html_to_markdown(html, trusted_rustnote_html);
    let mut decoded = markdown::decode(&markdown_text)?;
    if exported_title.is_some() {
        decoded.title_hint = exported_title;
    }
    decoded.warnings.extend(warnings);
    let mut layout_index = 0_usize;
    apply_layouts_in_document_order(&mut decoded.document, &layouts, &mut layout_index);
    Ok(decoded)
}

fn apply_layouts_in_document_order(
    node: &mut Value,
    layouts: &[HashMap<String, String>],
    index: &mut usize,
) {
    if matches!(node.get("type").and_then(Value::as_str), Some("paragraph" | "heading")) {
        if let Some(attrs) = layouts.get(*index) {
            apply_textblock_layout_from_attrs(node, attrs);
        }
        *index = index.saturating_add(1);
    }
    if let Some(content) = node.get_mut("content").and_then(Value::as_array_mut) {
        for child in content {
            apply_layouts_in_document_order(child, layouts, index);
        }
    }
}

pub(super) fn encode(
    title: &str,
    editor_state: &str,
    attachment_paths: &HashMap<String, String>,
    note_paths: &HashMap<String, String>,
) -> PortabilityResult<String> {
    let document = parse_validated_document(editor_state).map_err(|error| error.to_string())?;
    let mut body = String::new();
    if let Some(content) = document.get("content").and_then(Value::as_array) {
        for node in content {
            render_block(node, &mut body, attachment_paths, note_paths)?;
        }
    }
    Ok(format!(
        "<!doctype html>\n<html><head><meta charset=\"utf-8\"><meta name=\"generator\" content=\"Rustnote\"><title>{}</title><style>{}</style></head><body><main class=\"rustnote-document\"><h1 class=\"rustnote-note-title\">{}</h1>{}</main></body></html>\n",
        escape_html(if title.trim().is_empty() { "Untitled note" } else { title.trim() }),
        EXPORT_CSS,
        escape_html(if title.trim().is_empty() { "Untitled note" } else { title.trim() }),
        body
    ))
}

const EXPORT_CSS: &str = "body{font-family:system-ui,-apple-system,sans-serif;max-width:760px;margin:3rem auto;padding:0 1.5rem;line-height:1.5}.rustnote-document img{max-width:100%;height:auto}.rustnote-attachment{margin:1rem 0}.rustnote-drop-cap::first-letter{float:left;font-size:3em;line-height:.8;padding:.08em .12em 0 0}table{border-collapse:collapse}td,th{border:1px solid #aaa;padding:.35rem .5rem}pre{white-space:pre-wrap}blockquote{margin-inline:1.5rem}";

fn render_block(node: &Value, output: &mut String, attachment_paths: &HashMap<String, String>, note_paths: &HashMap<String, String>) -> PortabilityResult<()> {
    match node.get("type").and_then(Value::as_str) {
        Some("paragraph") => {
            output.push_str("<p"); render_layout_attrs(node, output); output.push('>'); render_inline(node, output, note_paths); output.push_str("</p>\n");
        }
        Some("heading") => {
            let level = node.get("attrs").and_then(Value::as_object).and_then(|attrs| attrs.get("level")).and_then(Value::as_u64).unwrap_or(1).clamp(1, 6);
            output.push_str(&format!("<h{level}")); render_layout_attrs(node, output); output.push('>'); render_inline(node, output, note_paths); output.push_str(&format!("</h{level}>\n"));
        }
        Some("blockquote") => {
            output.push_str("<blockquote>\n");
            if let Some(content) = node.get("content").and_then(Value::as_array) { for child in content { render_block(child, output, attachment_paths, note_paths)?; } }
            output.push_str("</blockquote>\n");
        }
        Some("code_block") => {
            let language = node.get("attrs").and_then(Value::as_object).and_then(|attrs| attrs.get("language")).and_then(Value::as_str).unwrap_or_default();
            output.push_str("<pre><code"); if !language.is_empty() { output.push_str(" data-language=\""); output.push_str(&escape_attr(language)); output.push('"'); } output.push('>'); output.push_str(&escape_html(&plain_text(node))); output.push_str("</code></pre>\n");
        }
        Some("horizontal_rule") => output.push_str("<hr>\n"),
        Some("attachment") => render_attachment(node, output, attachment_paths),
        Some("bullet_list" | "ordered_list" | "task_list") => {
            render_list(node, output, attachment_paths, note_paths)?;
        }
        Some("table") => render_table(node, output, attachment_paths, note_paths)?,
        Some("math_display") => { output.push_str("<div data-rustnote-math-display=\"true\"><pre>"); output.push_str(&escape_html(&plain_text(node))); output.push_str("</pre></div>\n"); }
        Some("attachment_pending") => {}
        _ => {
            let text = plain_text(node);
            if !text.is_empty() { output.push_str("<p>"); output.push_str(&escape_html(&text)); output.push_str("</p>\n"); }
        }
    }
    Ok(())
}

fn render_inline(node: &Value, output: &mut String, note_paths: &HashMap<String, String>) {
    if let Some(content) = node.get("content").and_then(Value::as_array) {
        for child in content {
            match child.get("type").and_then(Value::as_str) {
                Some("text") => render_text(child, output, note_paths),
                Some("hard_break") => output.push_str("<br>"),
                Some("math_inline") => { output.push_str("<span data-rustnote-math-inline=\"true\">"); output.push_str(&escape_html(&plain_text(child))); output.push_str("</span>"); }
                _ => output.push_str(&escape_html(&plain_text(child))),
            }
        }
    }
}

fn render_text(node: &Value, output: &mut String, note_paths: &HashMap<String, String>) {
    let text = escape_html(node.get("text").and_then(Value::as_str).unwrap_or_default());
    let marks = node.get("marks").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut openings = Vec::new();
    let mut closings = Vec::new();
    for mark in &marks {
        match mark.get("type").and_then(Value::as_str) {
            Some("strong") => { openings.push("<strong>".to_string()); closings.insert(0, "</strong>".to_string()); }
            Some("em") => { openings.push("<em>".to_string()); closings.insert(0, "</em>".to_string()); }
            Some("underline") => { openings.push("<u>".to_string()); closings.insert(0, "</u>".to_string()); }
            Some("strike") => { openings.push("<del>".to_string()); closings.insert(0, "</del>".to_string()); }
            Some("superscript") => { openings.push("<sup>".to_string()); closings.insert(0, "</sup>".to_string()); }
            Some("subscript") => { openings.push("<sub>".to_string()); closings.insert(0, "</sub>".to_string()); }
            Some("link") => {
                let href = mark.get("attrs").and_then(Value::as_object).and_then(|attrs| attrs.get("href")).and_then(Value::as_str).unwrap_or("#");
                openings.push(format!("<a href=\"{}\">", escape_attr(href))); closings.insert(0, "</a>".to_string());
            }
            Some("note_link") => {
                let id = mark.get("attrs").and_then(Value::as_object).and_then(|attrs| attrs.get("noteId")).and_then(Value::as_str).unwrap_or_default();
                let href = note_paths.get(id).map(String::as_str).unwrap_or("#");
                openings.push(format!("<a data-rustnote-note-id=\"{}\" href=\"{}\">", escape_attr(id), escape_attr(href))); closings.insert(0, "</a>".to_string());
            }
            _ => {}
        }
    }
    for opening in openings { output.push_str(&opening); }
    output.push_str(&text);
    for closing in closings { output.push_str(&closing); }
}

fn render_layout_attrs(node: &Value, output: &mut String) {
    let Some(attrs) = node.get("attrs").and_then(Value::as_object) else { return; };
    if let Some(value) = attrs.get("lineSpacing").and_then(Value::as_str) { output.push_str(" data-rustnote-line-spacing=\""); output.push_str(&escape_attr(value)); output.push('"'); }
    if let Some(value) = attrs.get("textAlignment").and_then(Value::as_str).filter(|value| *value != "left") { output.push_str(" data-rustnote-text-alignment=\""); output.push_str(&escape_attr(value)); output.push('"'); }
    for (key, html_key) in [("leftIndent", "data-rustnote-left-indent"), ("rightIndent", "data-rustnote-right-indent"), ("firstLineIndent", "data-rustnote-first-line-indent")] {
        let value = attrs.get(key).and_then(Value::as_i64).unwrap_or(0);
        if value > 0 { output.push(' '); output.push_str(html_key); output.push_str("=\""); output.push_str(&value.to_string()); output.push('"'); }
    }
    if attrs.get("dropCap").and_then(Value::as_bool).unwrap_or(false) { output.push_str(" data-rustnote-drop-cap=\"true\" class=\"rustnote-drop-cap\""); }
    let alignment = attrs.get("textAlignment").and_then(Value::as_str).unwrap_or("left");
    let left = attrs.get("leftIndent").and_then(Value::as_i64).unwrap_or(0);
    let right = attrs.get("rightIndent").and_then(Value::as_i64).unwrap_or(0);
    let first = attrs.get("firstLineIndent").and_then(Value::as_i64).unwrap_or(0);
    let spacing = attrs.get("lineSpacing").and_then(Value::as_str);
    if alignment != "left" || left > 0 || right > 0 || first > 0 || spacing.is_some() {
        output.push_str(" style=\"");
        if alignment != "left" { output.push_str("text-align:"); output.push_str(alignment); output.push(';'); }
        if left > 0 { output.push_str(&format!("margin-inline-start:{left}pt;")); }
        if right > 0 { output.push_str(&format!("margin-inline-end:{right}pt;")); }
        if first > 0 { output.push_str(&format!("text-indent:{first}pt;")); }
        if let Some(spacing) = spacing { output.push_str("line-height:"); output.push_str(spacing); output.push(';'); }
        output.push('"');
    }
}

fn render_attachment(node: &Value, output: &mut String, attachment_paths: &HashMap<String, String>) {
    let attrs = node.get("attrs").and_then(Value::as_object);
    let id = attrs.and_then(|a| a.get("attachmentId")).and_then(Value::as_str).unwrap_or_default();
    let name = attrs.and_then(|a| a.get("displayName")).and_then(Value::as_str).unwrap_or("Attachment");
    let kind = attrs.and_then(|a| a.get("kind")).and_then(Value::as_str).unwrap_or("file");
    let caption = attrs.and_then(|a| a.get("caption")).and_then(Value::as_str).unwrap_or_default();
    output.push_str("<figure class=\"rustnote-attachment\" data-rustnote-attachment=\"true\">");
    if let Some(path) = attachment_paths.get(id) {
        if kind == "image" {
            output.push_str("<img src=\""); output.push_str(&escape_attr(path)); output.push_str("\" alt=\""); output.push_str(&escape_attr(name)); output.push_str("\">");
        } else {
            output.push_str("<a href=\""); output.push_str(&escape_attr(path)); output.push_str("\" data-rustnote-attachment-link=\"true\">"); output.push_str(&escape_html(name)); output.push_str("</a>");
        }
    } else { output.push_str(&escape_html(name)); }
    if !caption.trim().is_empty() { output.push_str("<figcaption>"); output.push_str(&escape_html(caption)); output.push_str("</figcaption>"); }
    output.push_str("</figure>\n");
}

fn render_list(node: &Value, output: &mut String, attachment_paths: &HashMap<String, String>, note_paths: &HashMap<String, String>) -> PortabilityResult<()> {
    let kind = node.get("type").and_then(Value::as_str).unwrap_or("bullet_list");
    let tag = if kind == "ordered_list" { "ol" } else { "ul" };
    output.push('<'); output.push_str(tag); output.push_str(">\n");
    if let Some(items) = node.get("content").and_then(Value::as_array) {
        for item in items {
            output.push_str("<li>");
            if kind == "task_list" {
                let checked = item.get("attrs").and_then(Value::as_object).and_then(|a| a.get("checked")).and_then(Value::as_bool).unwrap_or(false);
                output.push_str(if checked { "☑ " } else { "☐ " });
            }
            if let Some(content) = item.get("content").and_then(Value::as_array) {
                if let Some(first) = content.first() { render_inline(first, output, note_paths); }
                for child in content.iter().skip(1) { render_block(child, output, attachment_paths, note_paths)?; }
            }
            output.push_str("</li>\n");
        }
    }
    output.push_str("</"); output.push_str(tag); output.push_str(">\n");
    Ok(())
}

fn render_table(node: &Value, output: &mut String, attachment_paths: &HashMap<String, String>, note_paths: &HashMap<String, String>) -> PortabilityResult<()> {
    output.push_str("<table>\n");
    if let Some(rows) = node.get("content").and_then(Value::as_array) {
        for row in rows {
            output.push_str("<tr>");
            if let Some(cells) = row.get("content").and_then(Value::as_array) {
                for cell in cells {
                    let tag = if cell.get("type").and_then(Value::as_str) == Some("table_header") { "th" } else { "td" };
                    output.push('<'); output.push_str(tag); output.push('>');
                    if let Some(content) = cell.get("content").and_then(Value::as_array) { for block in content { render_block(block, output, attachment_paths, note_paths)?; } }
                    output.push_str("</"); output.push_str(tag); output.push('>');
                }
            }
            output.push_str("</tr>\n");
        }
    }
    output.push_str("</table>\n");
    Ok(())
}

fn html_to_markdown(
    html: &str,
    trusted_rustnote_html: bool,
) -> (String, Vec<HashMap<String, String>>, Vec<String>) {
    let mut output = String::new();
    let mut layouts = Vec::new();
    let mut warnings = Vec::new();
    let mut index = 0_usize;
    let mut skip_tag: Option<String> = None;
    let mut link_stack: Vec<Option<String>> = Vec::new();
    while index < html.len() {
        let Some(open_offset) = html[index..].find('<') else { if skip_tag.is_none() { output.push_str(&decode_entities(&html[index..])); } break; };
        let open = index + open_offset;
        if skip_tag.is_none() { output.push_str(&decode_entities(&html[index..open])); }
        let Some(close_offset) = html[open..].find('>') else { break; };
        let close = open + close_offset;
        let raw = html[open + 1..close].trim();
        let closing = raw.starts_with('/');
        let body = raw.trim_start_matches('/').trim();
        let name = body.split_whitespace().next().unwrap_or_default().trim_end_matches('/').to_ascii_lowercase();
        if let Some(skipped) = skip_tag.as_ref() {
            if closing && &name == skipped { skip_tag = None; }
            index = close + 1; continue;
        }
        if matches!(name.as_str(), "head" | "script" | "style" | "iframe" | "object" | "embed" | "form" | "svg") && !closing { skip_tag = Some(name); index = close + 1; continue; }
        let attrs = parse_attrs(body.strip_prefix(&name).unwrap_or_default());
        if trusted_rustnote_html
            && !closing
            && name == "h1"
            && attrs.get("class").is_some_and(|classes| classes.split_whitespace().any(|class| class == "rustnote-note-title"))
        {
            skip_tag = Some(name);
            index = close + 1;
            continue;
        }
        match (closing, name.as_str()) {
            (false, "p") => {
                ensure_blank_line(&mut output);
                layouts.push(if trusted_rustnote_html { attrs.clone() } else { HashMap::new() });
            }
            (true, "p") => ensure_blank_line(&mut output),
            (false, "h1" | "h2" | "h3" | "h4" | "h5" | "h6") => {
                ensure_blank_line(&mut output);
                let level = name[1..].parse::<usize>().unwrap_or(1);
                output.push_str(&"#".repeat(level));
                output.push(' ');
                layouts.push(if trusted_rustnote_html { attrs.clone() } else { HashMap::new() });
            }
            (true, "h1" | "h2" | "h3" | "h4" | "h5" | "h6") => ensure_blank_line(&mut output),
            (false, "strong" | "b") | (true, "strong" | "b") => output.push_str("**"),
            (false, "em" | "i") | (true, "em" | "i") => output.push('*'),
            (false, "del" | "s" | "strike") | (true, "del" | "s" | "strike") => output.push_str("~~"),
            (false, "br") => output.push('\n'),
            (false, "hr") => { ensure_blank_line(&mut output); output.push_str("---\n\n"); }
            (false, "blockquote") => { ensure_blank_line(&mut output); output.push_str("> "); }
            (true, "blockquote") => ensure_blank_line(&mut output),
            (false, "li") => { if !output.ends_with('\n') { output.push('\n'); } output.push_str("- "); }
            (true, "li") => output.push('\n'),
            (false, "ul" | "ol") => ensure_blank_line(&mut output),
            (true, "ul" | "ol") => ensure_blank_line(&mut output),
            (false, "pre") => { ensure_blank_line(&mut output); output.push_str("```\n"); }
            (true, "pre") => { output.push_str("\n```\n\n"); }
            (false, "a") => {
                let href = attrs.get("href").and_then(|value| safe_html_link_target(value));
                if href.is_some() {
                    output.push('[');
                } else if attrs.contains_key("href") {
                    warnings.push("Dropped an unsafe HTML link target".to_string());
                }
                link_stack.push(href);
            }
            (true, "a") => {
                if let Some(Some(href)) = link_stack.pop() {
                    output.push_str("](");
                    output.push_str(&href);
                    output.push(')');
                }
            }
            (false, "img") => {
                if let Some(src) = attrs.get("src") {
                    if let Some(path) = safe_relative_resource_path(src) {
                        output.push_str("![");
                        output.push_str(attrs.get("alt").map(String::as_str).unwrap_or("Image"));
                        output.push_str("](");
                        output.push_str(&path.to_string_lossy().replace('\\', "/"));
                        output.push(')');
                    } else {
                        warnings.push("Skipped a non-local HTML image resource".to_string());
                        if let Some(alt) = attrs.get("alt").filter(|value| !value.trim().is_empty()) {
                            output.push_str(alt);
                        }
                    }
                }
            }
            (false, "figure") if attrs.contains_key("data-rustnote-attachment") => ensure_blank_line(&mut output),
            (true, "figure") => ensure_blank_line(&mut output),
            (false, "tr") => { if !output.ends_with('\n') { output.push('\n'); } output.push('|'); }
            (true, "tr") => output.push('\n'),
            (false, "td" | "th") => output.push(' '),
            (true, "td" | "th") => output.push_str(" |"),
            _ => {}
        }
        if raw.to_ascii_lowercase().contains("onload=") || raw.to_ascii_lowercase().contains("onclick=") { warnings.push("Ignored HTML event-handler attributes".to_string()); }
        index = close + 1;
    }
    (output, layouts, warnings)
}

fn safe_html_link_target(value: &str) -> Option<String> {
    let value = value.trim();
    if value.starts_with('#') && !value.chars().any(char::is_control) {
        return Some(value.to_string());
    }
    if is_safe_href(value) {
        return Some(value.to_string());
    }
    safe_relative_resource_path(value)
        .map(|path| path.to_string_lossy().replace('\\', "/"))
}

fn is_rustnote_html(html: &str) -> bool {
    let mut index = 0_usize;
    while index < html.len() {
        let Some(open_offset) = html[index..].find('<') else { break; };
        let open = index + open_offset;
        let Some(close_offset) = html[open..].find('>') else { break; };
        let close = open + close_offset;
        let raw = html[open + 1..close].trim();
        let body = raw.trim_start_matches('/').trim();
        let name = body
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .trim_end_matches('/')
            .to_ascii_lowercase();
        if name == "meta" {
            let attrs = parse_attrs(body.strip_prefix(&name).unwrap_or_default());
            if attrs
                .get("name")
                .is_some_and(|value| value.eq_ignore_ascii_case("generator"))
                && attrs
                    .get("content")
                    .is_some_and(|value| value.eq_ignore_ascii_case("Rustnote"))
            {
                return true;
            }
        }
        index = close + 1;
    }
    false
}

fn html_title(html: &str) -> Option<String> {
    let lowercase = html.to_ascii_lowercase();
    let open = lowercase.find("<title")?;
    let start = open + lowercase[open..].find('>')? + 1;
    let end = start + lowercase[start..].find("</title>")?;
    let title = decode_entities(&html[start..end]).trim().to_string();
    (!title.is_empty()).then_some(title)
}

fn parse_attrs(input: &str) -> HashMap<String, String> {
    let mut attrs = HashMap::new();
    let bytes = input.as_bytes(); let mut index = 0_usize;
    while index < bytes.len() {
        while index < bytes.len() && bytes[index].is_ascii_whitespace() { index += 1; }
        let start = index;
        while index < bytes.len() && !bytes[index].is_ascii_whitespace() && bytes[index] != b'=' { index += 1; }
        if start == index { index += 1; continue; }
        let key = input[start..index].trim().to_ascii_lowercase();
        while index < bytes.len() && bytes[index].is_ascii_whitespace() { index += 1; }
        let mut value = String::new();
        if index < bytes.len() && bytes[index] == b'=' {
            index += 1; while index < bytes.len() && bytes[index].is_ascii_whitespace() { index += 1; }
            if index < bytes.len() && matches!(bytes[index], b'\'' | b'"') {
                let quote = bytes[index]; index += 1; let value_start = index;
                while index < bytes.len() && bytes[index] != quote { index += 1; }
                value = decode_entities(&input[value_start..index]); if index < bytes.len() { index += 1; }
            } else { let value_start = index; while index < bytes.len() && !bytes[index].is_ascii_whitespace() { index += 1; } value = decode_entities(&input[value_start..index]); }
        }
        if !key.starts_with("on") { attrs.insert(key, value); }
    }
    attrs
}

fn ensure_blank_line(output: &mut String) { while output.ends_with("\n\n\n") { output.pop(); } if !output.is_empty() && !output.ends_with("\n\n") { if !output.ends_with('\n') { output.push('\n'); } output.push('\n'); } }
fn plain_text(node: &Value) -> String { super::document::inline_plain_text(node) }
fn escape_html(value: &str) -> String { value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;") }
fn escape_attr(value: &str) -> String { escape_html(value).replace('\'', "&#39;") }
fn decode_entities(value: &str) -> String {
    let mut result = value.replace("&nbsp;", " ").replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&apos;", "'");
    // Numeric entities are intentionally decoded only for Unicode scalar values.
    let mut start = 0_usize;
    while let Some(offset) = result[start..].find("&#") {
        let open = start + offset; let Some(end_offset) = result[open..].find(';') else { break; }; let end = open + end_offset;
        let body = &result[open + 2..end]; let decoded = body.strip_prefix('x').or_else(|| body.strip_prefix('X')).and_then(|hex| u32::from_str_radix(hex, 16).ok()).or_else(|| body.parse::<u32>().ok()).and_then(char::from_u32);
        if let Some(character) = decoded { result.replace_range(open..=end, &character.to_string()); start = open + character.len_utf8(); } else { start = end + 1; }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use crate::portability::document::canonical_editor_state;

    #[test]
    fn foreign_html_drops_script_and_rustnote_only_layout_metadata() {
        let decoded = decode("<script>alert(1)</script><p style='margin-left:99px' data-rustnote-left-indent='18'>Hello <strong>world</strong></p>").expect("decode");
        let state = canonical_editor_state(decoded.document).expect("canonical state");
        assert!(state.contains("\\\"leftIndent\\\":0") || state.contains("\"leftIndent\":0"));
        assert!(!state.contains("99px"));
        assert!(!state.contains("alert"));
    }

    #[test]
    fn rustnote_html_can_round_trip_bounded_layout_metadata() {
        let decoded = decode("<!doctype html><html><head><meta name='generator' content='Rustnote'><title>Parliament</title></head><body><h1 class='rustnote-note-title'>Parliament</h1><p data-rustnote-left-indent='18' data-rustnote-text-alignment='justify'>Hello</p></body></html>").expect("decode");
        let state = canonical_editor_state(decoded.document).expect("canonical state");
        assert_eq!(decoded.title_hint.as_deref(), Some("Parliament"));
        assert!(state.contains("\\\"leftIndent\\\":18") || state.contains("\"leftIndent\":18"));
        assert!(state.contains("\\\"textAlignment\\\":\\\"justify\\\"") || state.contains("\"textAlignment\":\"justify\""));
        assert_eq!(state.matches("Parliament").count(), 0);
    }

    #[test]
    fn nested_rustnote_layout_metadata_stays_on_its_textblock() {
        let decoded = decode("<!doctype html><html><head><meta name='generator' content='Rustnote'></head><body><blockquote><p data-rustnote-left-indent='18'>Quoted</p></blockquote><p>Tail</p></body></html>")
            .expect("decode nested layout");
        let state = canonical_editor_state(decoded.document).expect("canonical state");
        let document = crate::editor_document::parse_validated_document(&state).expect("validated document");
        let blocks = document.get("content").and_then(Value::as_array).expect("top-level blocks");
        let quoted = blocks[0]
            .get("content").and_then(Value::as_array).and_then(|content| content.first())
            .expect("quoted paragraph");
        assert_eq!(quoted.get("attrs").and_then(Value::as_object).and_then(|attrs| attrs.get("leftIndent")).and_then(Value::as_i64), Some(18));
        assert_eq!(blocks[1].get("attrs").and_then(Value::as_object).and_then(|attrs| attrs.get("leftIndent")).and_then(Value::as_i64), Some(0));
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
        })).expect("canonical attachment fixture");
        let attachments = HashMap::from([("a1".to_string(), "Topic.assets/image.png".to_string())]);
        let exported = encode("Topic", &state, &attachments, &HashMap::new()).expect("export");
        assert!(exported.contains("Topic.assets/image.png"));
        let imported = decode(&exported).expect("import exported document");
        assert_eq!(imported.resources.len(), 1);
        assert_eq!(imported.resources[0].path, "Topic.assets/image.png");
    }

    #[test]
    fn html_import_drops_unsafe_links_and_remote_images() {
        let decoded = decode("<p><a href='javascript:alert(1)'>bad</a></p><img src='https://example.com/x.png' alt='Remote'>").expect("decode");
        let state = canonical_editor_state(decoded.document).expect("canonical state");
        assert!(!state.contains("javascript:"));
        assert!(decoded.resources.is_empty());
        assert!(!decoded.warnings.is_empty());
    }
}
