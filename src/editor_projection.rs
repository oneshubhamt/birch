use serde_json::Value;

use crate::{
    editor_document::{DocumentValidationError, parse_validated_document},
    link::is_safe_href,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorProjections {
    pub markdown: String,
    pub plain_text: String,
}

/// Derive Rustnote's Markdown and plain-text projections from the canonical
/// ProseMirror document. The editor bridge transfers only canonical state;
/// this module is the single production authority for derived projections.
pub fn derive_editor_projections(serialized: &str) -> Result<EditorProjections, DocumentValidationError> {
    let document = parse_validated_document(serialized)?;
    let markdown = markdown_from_document(&document).trim_end().to_string();
    let plain_text = plain_text_from_document(&document).trim().to_string();
    Ok(EditorProjections {
        markdown,
        plain_text,
    })
}

fn markdown_from_document(document: &Value) -> String {
    let mut output = String::new();
    for child in node_content(document) {
        output.push_str(&serialize_block(child));
    }
    output
}

fn plain_text_from_document(document: &Value) -> String {
    let mut blocks = Vec::new();
    for child in node_content(document) {
        collect_plain_text_blocks(child, &mut blocks);
    }
    blocks.join("\n\n")
}

fn serialize_block(node: &Value) -> String {
    match node_type(node) {
        Some("paragraph") => {
            if is_atomic_boundary_paragraph(node) {
                String::new()
            } else {
                format!("{}\n\n", serialize_inline(node, true))
            }
        }
        Some("heading") => {
            let level = node_attr_u64(node, "level").unwrap_or(1).clamp(1, 6);
            format!("{} {}\n\n", "#".repeat(level as usize), serialize_inline(node, false))
        }
        Some("blockquote") => {
            let mut inner = String::new();
            for child in node_content(node) {
                inner.push_str(&serialize_block(child));
            }
            let quoted = inner
                .trim_end()
                .split('\n')
                .map(|line| format!("> {line}"))
                .collect::<Vec<_>>()
                .join("\n");
            format!("{quoted}\n\n")
        }
        Some("code_block") => {
            let language = node_attr_str(node, "language").unwrap_or_default();
            let text = node_text_content(node);
            let fence = adaptive_code_fence(&text);
            let body = if text.ends_with('\n') {
                text
            } else {
                format!("{text}\n")
            };
            format!("{fence}{language}\n{body}{fence}\n\n")
        }
        Some("math_display") => format!("$$\n{}\n$$\n\n", node_text_content(node)),
        Some("horizontal_rule") => "---\n\n".to_string(),
        Some("attachment_pending") => String::new(),
        Some("attachment") => serialize_attachment_markdown(node),
        Some("table") => serialize_table(node),
        Some("bullet_list" | "ordered_list" | "task_list") => serialize_list(node, ""),
        _ => format!("{}\n\n", node_text_content(node)),
    }
}

fn serialize_attachment_markdown(node: &Value) -> String {
    let name = node_attr_str(node, "displayName").unwrap_or("Attachment");
    let kind = node_attr_str(node, "kind").unwrap_or("file");
    let caption = node_attr_str(node, "caption").unwrap_or("").trim();
    let label = match kind {
        "image" => "Image",
        "video" => "Video",
        "audio" => "Audio",
        _ => "Attachment",
    };
    let mut sections = vec![format!(
        "[{label}: {}]",
        escape_markdown_text_with_context(name, false)
    )];
    if !caption.is_empty() {
        sections.push(escape_markdown_block_leading_syntax(
            &escape_markdown_text_with_context(caption, false),
        ));
    }

    if matches!(kind, "audio" | "video") {
        if let Some(trim) = media_trim_projection(node) {
            sections.push(format!("Playback trim: {trim}"));
        }
        let annotations = media_annotations(node);
        if !annotations.is_empty() {
            let rendered = annotations
                .iter()
                .map(serialize_media_annotation_markdown)
                .collect::<Vec<_>>()
                .join("\n");
            sections.push(format!("Media notes:\n\n{rendered}"));
        }
    }

    format!("{}\n\n", sections.join("\n\n"))
}

fn serialize_media_annotation_markdown(annotation: &Value) -> String {
    let start_ms = annotation
        .get("startMs")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let end_ms = annotation.get("endMs").and_then(Value::as_u64);
    let time_label = media_annotation_time_label(start_ms, end_ms);
    let label = annotation
        .get("label")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let tag = annotation
        .get("tag")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let note = annotation
        .get("note")
        .and_then(Value::as_str)
        .unwrap_or_default();

    let mut output = format!("- [{time_label}]");
    let label = markdown_media_inline_value(label);
    if !label.is_empty() {
        output.push(' ');
        output.push_str(&label);
    }

    let tag = markdown_media_inline_value(tag);
    if !tag.is_empty() {
        output.push_str("\n  - Tag: ");
        output.push_str(&tag);
    }

    let note = markdown_media_note_value(note);
    if !note.is_empty() {
        let mut lines = note.split('\n');
        output.push_str("\n  - Note: ");
        output.push_str(lines.next().unwrap_or_default());
        for line in lines {
            output.push_str("\n    ");
            output.push_str(line);
        }
    }
    output
}

fn markdown_media_inline_value(value: &str) -> String {
    escape_markdown_text_with_context(&collapse_whitespace(value), false)
}

fn markdown_media_note_value(value: &str) -> String {
    let normalized = value.replace("\r\n", "\n").replace('\r', "\n");
    let trimmed = normalized.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    escape_markdown_block_leading_syntax(&escape_markdown_text_with_context(trimmed, false))
}

fn media_annotation_time_label(start_ms: u64, end_ms: Option<u64>) -> String {
    let start = media_time_label_ms(start_ms);
    match end_ms {
        Some(end) if end > start_ms => format!("{start}–{}", media_time_label_ms(end)),
        _ => start,
    }
}

fn media_time_label_ms(time_ms: u64) -> String {
    let total_seconds = time_ms / 1000;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

fn media_trim_projection(node: &Value) -> Option<String> {
    let attrs = node.get("attrs")?;
    let start = attrs.get("mediaTrimStartMs").and_then(Value::as_u64);
    let end = attrs.get("mediaTrimEndMs").and_then(Value::as_u64);
    match (start.filter(|value| *value > 0), end.filter(|value| *value > 0)) {
        (Some(start), Some(end)) if end > start => {
            Some(format!("{}–{}", media_time_label_ms(start), media_time_label_ms(end)))
        }
        (Some(start), _) => Some(format!("starts at {}", media_time_label_ms(start))),
        (None, Some(end)) => Some(format!("0:00–{}", media_time_label_ms(end))),
        _ => None,
    }
}

fn media_annotations(node: &Value) -> &[Value] {
    node.get("attrs")
        .and_then(|attrs| attrs.get("mediaAnnotations"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MarkdownDelimiterStyle {
    Markdown,
    Html,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MarkdownDelimiterKind {
    Strong,
    Em,
    Strike,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum MarkdownExportMark {
    NoteLink,
    Link(String),
    Strong(MarkdownDelimiterStyle),
    Em(MarkdownDelimiterStyle),
    Strike(MarkdownDelimiterStyle),
    Underline,
    Superscript,
    Subscript,
    Highlight,
    Code,
}

impl MarkdownExportMark {
    fn priority(&self) -> usize {
        match self {
            Self::NoteLink => 0,
            Self::Link(_) => 1,
            Self::Strong(_) => 2,
            Self::Em(_) => 3,
            Self::Strike(_) => 4,
            Self::Underline => 5,
            Self::Superscript => 6,
            Self::Subscript => 7,
            Self::Highlight => 8,
            Self::Code => 9,
        }
    }

    fn open(&self) -> &'static str {
        match self {
            Self::NoteLink => "[[",
            Self::Link(_) => "[",
            Self::Strong(MarkdownDelimiterStyle::Markdown) => "**",
            Self::Strong(MarkdownDelimiterStyle::Html) => "<strong>",
            Self::Em(MarkdownDelimiterStyle::Markdown) => "*",
            Self::Em(MarkdownDelimiterStyle::Html) => "<em>",
            Self::Strike(MarkdownDelimiterStyle::Markdown) => "~~",
            Self::Strike(MarkdownDelimiterStyle::Html) => "<del>",
            Self::Underline => "<u>",
            Self::Superscript => "<sup>",
            Self::Subscript => "<sub>",
            Self::Highlight => "<mark>",
            Self::Code => "",
        }
    }

    fn close(&self) -> String {
        match self {
            Self::NoteLink => "]]".to_string(),
            Self::Link(href) => format!("]({})", escape_link_destination(href)),
            Self::Strong(MarkdownDelimiterStyle::Markdown) => "**".to_string(),
            Self::Strong(MarkdownDelimiterStyle::Html) => "</strong>".to_string(),
            Self::Em(MarkdownDelimiterStyle::Markdown) => "*".to_string(),
            Self::Em(MarkdownDelimiterStyle::Html) => "</em>".to_string(),
            Self::Strike(MarkdownDelimiterStyle::Markdown) => "~~".to_string(),
            Self::Strike(MarkdownDelimiterStyle::Html) => "</del>".to_string(),
            Self::Underline => "</u>".to_string(),
            Self::Superscript => "</sup>".to_string(),
            Self::Subscript => "</sub>".to_string(),
            Self::Highlight => "</mark>".to_string(),
            Self::Code => String::new(),
        }
    }

    fn delimiter_kind(&self) -> Option<MarkdownDelimiterKind> {
        match self {
            Self::Strong(_) => Some(MarkdownDelimiterKind::Strong),
            Self::Em(_) => Some(MarkdownDelimiterKind::Em),
            Self::Strike(_) => Some(MarkdownDelimiterKind::Strike),
            _ => None,
        }
    }

    fn has_delimiter_kind(&self, kind: MarkdownDelimiterKind) -> bool {
        self.delimiter_kind() == Some(kind)
    }

    fn use_html_delimiter(&mut self, kind: MarkdownDelimiterKind) {
        match (self, kind) {
            (Self::Strong(style), MarkdownDelimiterKind::Strong)
            | (Self::Em(style), MarkdownDelimiterKind::Em)
            | (Self::Strike(style), MarkdownDelimiterKind::Strike) => {
                *style = MarkdownDelimiterStyle::Html;
            }
            _ => {}
        }
    }

    fn is_markdown_delimiter(&self) -> bool {
        self.delimiter_kind().is_some()
    }
}

#[derive(Debug)]
enum MarkdownInlineToken<'a> {
    Text {
        text: &'a str,
        marks: Vec<MarkdownExportMark>,
    },
    Raw(String),
}

fn serialize_inline(node: &Value, escape_block_starts: bool) -> String {
    let mut tokens = markdown_inline_tokens(node);
    apply_markdown_delimiter_safety(&mut tokens);
    let mut output = String::new();
    let mut active_marks: Vec<MarkdownExportMark> = Vec::new();

    for (index, token) in tokens.iter().enumerate() {
        match token {
            MarkdownInlineToken::Raw(raw) => {
                close_markdown_marks(&mut output, &mut active_marks, 0);
                output.push_str(raw);
            }
            MarkdownInlineToken::Text { text, marks }
                if marks.len() == 1 && marks[0] == MarkdownExportMark::Code =>
            {
                close_markdown_marks(&mut output, &mut active_marks, 0);
                output.push_str(&inline_code_span(text));
            }
            MarkdownInlineToken::Text { text, marks } => {
                let common = common_markdown_mark_prefix(&active_marks, marks);
                close_markdown_marks(&mut output, &mut active_marks, common);
                for mark in marks.iter().skip(common) {
                    output.push_str(mark.open());
                    active_marks.push(mark.clone());
                }

                let next_starts_link = matches!(
                    tokens.get(index + 1),
                    Some(MarkdownInlineToken::Text { marks: next_marks, .. })
                        if next_marks
                            .first()
                            .is_some_and(|mark| matches!(mark, MarkdownExportMark::Link(_) | MarkdownExportMark::NoteLink))
                );
                output.push_str(&escape_markdown_text_with_context(
                    text,
                    marks.is_empty() && next_starts_link,
                ));
            }
        }
    }

    close_markdown_marks(&mut output, &mut active_marks, 0);
    if escape_block_starts {
        escape_markdown_block_leading_syntax(&output)
    } else {
        output
    }
}

fn markdown_inline_tokens(node: &Value) -> Vec<MarkdownInlineToken<'_>> {
    let mut tokens = Vec::new();
    for child in node_content(node) {
        match node_type(child) {
            Some("text") => {
                let text = child.get("text").and_then(Value::as_str).unwrap_or_default();
                let marks = markdown_export_marks(
                    child
                        .get("marks")
                        .and_then(Value::as_array)
                        .map(Vec::as_slice)
                        .unwrap_or_default(),
                );
                append_markdown_text_tokens(&mut tokens, text, marks);
            }
            Some("math_inline") => tokens.push(MarkdownInlineToken::Raw(format!(
                "${}$",
                node_text_content(child)
            ))),
            Some("hard_break") => tokens.push(MarkdownInlineToken::Raw("  \n".to_string())),
            _ => {}
        }
    }
    tokens
}

fn markdown_export_marks(marks: &[Value]) -> Vec<MarkdownExportMark> {
    let mut exported = Vec::new();
    for mark in marks {
        let value = match mark_type(mark) {
            Some("note_link") => Some(MarkdownExportMark::NoteLink),
            Some("link") => mark_attr_str(mark, "href")
                .filter(|href| is_safe_href(href))
                .map(|href| MarkdownExportMark::Link(href.to_string())),
            Some("strong") => Some(MarkdownExportMark::Strong(MarkdownDelimiterStyle::Markdown)),
            Some("em") => Some(MarkdownExportMark::Em(MarkdownDelimiterStyle::Markdown)),
            Some("strike") => Some(MarkdownExportMark::Strike(MarkdownDelimiterStyle::Markdown)),
            Some("underline") => Some(MarkdownExportMark::Underline),
            Some("superscript") => Some(MarkdownExportMark::Superscript),
            Some("subscript") => Some(MarkdownExportMark::Subscript),
            Some("highlight") => Some(MarkdownExportMark::Highlight),
            Some("code") => Some(MarkdownExportMark::Code),
            // Typography that Markdown cannot represent must not split an otherwise
            // continuous exported mark run.
            Some("font_family") | Some("font_size") | Some("text_color") | None => None,
            Some(_) => None,
        };
        if let Some(value) = value {
            exported.push(value);
        }
    }
    exported.sort_by_key(MarkdownExportMark::priority);
    if exported.contains(&MarkdownExportMark::Code) {
        return vec![MarkdownExportMark::Code];
    }
    exported
}

fn append_markdown_text_tokens<'a>(
    tokens: &mut Vec<MarkdownInlineToken<'a>>,
    text: &'a str,
    marks: Vec<MarkdownExportMark>,
) {
    if text.is_empty() {
        return;
    }
    if marks.contains(&MarkdownExportMark::Code)
        || !marks.iter().any(MarkdownExportMark::is_markdown_delimiter)
    {
        tokens.push(MarkdownInlineToken::Text { text, marks });
        return;
    }

    let (leading, core, trailing) = split_markdown_delimiter_whitespace(text);
    if core.is_empty() {
        tokens.push(MarkdownInlineToken::Text {
            text,
            marks: Vec::new(),
        });
        return;
    }
    if !leading.is_empty() {
        tokens.push(MarkdownInlineToken::Text {
            text: leading,
            marks: Vec::new(),
        });
    }
    tokens.push(MarkdownInlineToken::Text { text: core, marks });
    if !trailing.is_empty() {
        tokens.push(MarkdownInlineToken::Text {
            text: trailing,
            marks: Vec::new(),
        });
    }
}

fn apply_markdown_delimiter_safety(tokens: &mut [MarkdownInlineToken<'_>]) {
    for kind in [
        MarkdownDelimiterKind::Strong,
        MarkdownDelimiterKind::Em,
        MarkdownDelimiterKind::Strike,
    ] {
        for (start, end) in unsafe_markdown_delimiter_ranges(tokens, kind) {
            for token in &mut tokens[start..=end] {
                if let MarkdownInlineToken::Text { marks, .. } = token {
                    for mark in marks {
                        mark.use_html_delimiter(kind);
                    }
                }
            }
        }
    }
}

fn unsafe_markdown_delimiter_ranges(
    tokens: &[MarkdownInlineToken<'_>],
    kind: MarkdownDelimiterKind,
) -> Vec<(usize, usize)> {
    let mut unsafe_ranges = Vec::new();
    let mut index = 0_usize;
    while index < tokens.len() {
        if !token_has_delimiter_kind(&tokens[index], kind) {
            index += 1;
            continue;
        }

        let start = index;
        while index + 1 < tokens.len()
            && tokens_share_delimiter_run(&tokens[index], &tokens[index + 1], kind)
        {
            index += 1;
        }
        let end = index;

        let safe = token_first_character(&tokens[start])
            .zip(token_last_character(&tokens[end]))
            .is_some_and(|(first, last)| {
                markdown_delimiter_can_open(previous_token_character(tokens, start), first)
                    && markdown_delimiter_can_close(last, next_token_character(tokens, end))
            });
        if !safe {
            unsafe_ranges.push((start, end));
        }
        index += 1;
    }
    unsafe_ranges
}

fn token_has_delimiter_kind(token: &MarkdownInlineToken<'_>, kind: MarkdownDelimiterKind) -> bool {
    matches!(
        token,
        MarkdownInlineToken::Text { marks, .. }
            if marks.iter().any(|mark| mark.has_delimiter_kind(kind))
    )
}

fn tokens_share_delimiter_run(
    left: &MarkdownInlineToken<'_>,
    right: &MarkdownInlineToken<'_>,
    kind: MarkdownDelimiterKind,
) -> bool {
    match (
        token_delimiter_prefix(left, kind),
        token_delimiter_prefix(right, kind),
    ) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

fn token_delimiter_prefix<'token>(
    token: &'token MarkdownInlineToken<'_>,
    kind: MarkdownDelimiterKind,
) -> Option<&'token [MarkdownExportMark]> {
    let MarkdownInlineToken::Text { marks, .. } = token else {
        return None;
    };
    let delimiter_index = marks
        .iter()
        .position(|mark| mark.has_delimiter_kind(kind))?;
    Some(&marks[..=delimiter_index])
}

fn token_first_character(token: &MarkdownInlineToken<'_>) -> Option<char> {
    match token {
        MarkdownInlineToken::Text { text, .. } => text.chars().next(),
        MarkdownInlineToken::Raw(raw) => raw.chars().next(),
    }
}

fn token_last_character(token: &MarkdownInlineToken<'_>) -> Option<char> {
    match token {
        MarkdownInlineToken::Text { text, .. } => text.chars().next_back(),
        MarkdownInlineToken::Raw(raw) => raw.chars().next_back(),
    }
}

fn previous_token_character(tokens: &[MarkdownInlineToken<'_>], start: usize) -> Option<char> {
    tokens[..start]
        .iter()
        .rev()
        .find_map(token_last_character)
}

fn next_token_character(tokens: &[MarkdownInlineToken<'_>], end: usize) -> Option<char> {
    tokens[end + 1..].iter().find_map(token_first_character)
}

fn markdown_delimiter_can_open(before: Option<char>, after: char) -> bool {
    if after.is_whitespace() {
        return false;
    }
    !markdown_punctuation_like(after)
        || before.is_none_or(|character| {
            character.is_whitespace() || markdown_punctuation_like(character)
        })
}

fn markdown_delimiter_can_close(before: char, after: Option<char>) -> bool {
    if before.is_whitespace() {
        return false;
    }
    !markdown_punctuation_like(before)
        || after.is_none_or(|character| {
            character.is_whitespace() || markdown_punctuation_like(character)
        })
}

fn markdown_punctuation_like(character: char) -> bool {
    !character.is_whitespace() && !character.is_alphanumeric()
}

fn common_markdown_mark_prefix(
    left: &[MarkdownExportMark],
    right: &[MarkdownExportMark],
) -> usize {
    left.iter()
        .zip(right.iter())
        .take_while(|(left, right)| left == right)
        .count()
}

fn close_markdown_marks(
    output: &mut String,
    active_marks: &mut Vec<MarkdownExportMark>,
    target_length: usize,
) {
    while active_marks.len() > target_length {
        if let Some(mark) = active_marks.pop() {
            output.push_str(&mark.close());
        }
    }
}

fn split_markdown_delimiter_whitespace(text: &str) -> (&str, &str, &str) {
    let start = text
        .char_indices()
        .find(|(_, character)| !character.is_whitespace())
        .map(|(index, _)| index)
        .unwrap_or(text.len());
    if start == text.len() {
        return (text, "", "");
    }
    let end = text
        .char_indices()
        .rev()
        .find(|(_, character)| !character.is_whitespace())
        .map(|(index, character)| index + character.len_utf8())
        .unwrap_or(start);
    (&text[..start], &text[start..end], &text[end..])
}

fn serialize_list(node: &Value, indentation: &str) -> String {
    let mut output = String::new();
    let mut order = node_attr_u64(node, "order").unwrap_or(1).max(1);
    for item in node_content(node) {
        let marker = match node_type(node) {
            Some("ordered_list") => {
                let value = format!("{order}. ");
                order = order.saturating_add(1);
                value
            }
            Some("task_list") => {
                if node_attr_bool(item, "checked").unwrap_or(false) {
                    "- [x] ".to_string()
                } else {
                    "- [ ] ".to_string()
                }
            }
            _ => "- ".to_string(),
        };
        let continuation_width = if node_type(node) == Some("task_list") {
            2
        } else {
            marker.len()
        };
        output.push_str(&serialize_list_item(
            item,
            &marker,
            indentation,
            continuation_width,
        ));
    }
    output.push('\n');
    output
}

fn serialize_list_item(
    item: &Value,
    marker: &str,
    indentation: &str,
    continuation_width: usize,
) -> String {
    let children = node_content(item);
    let continuation_indent = format!("{indentation}{}", " ".repeat(continuation_width));
    let first_text = children
        .first()
        .filter(|child| is_textblock(child))
        .map(|child| serialize_inline(child, true))
        .unwrap_or_default();
    let mut output = format!(
        "{indentation}{marker}{}",
        indent_continuation_lines(&first_text, &continuation_indent)
    );

    for child in children.iter().skip(1) {
        if matches!(
            node_type(child),
            Some("bullet_list" | "ordered_list" | "task_list")
        ) {
            output.push('\n');
            output.push_str(serialize_list(child, &continuation_indent).trim_end());
        } else {
            output.push_str("\n\n");
            output.push_str(&indent_lines(
                serialize_block(child).trim_end(),
                &continuation_indent,
            ));
        }
    }
    output.push('\n');
    output
}

fn indent_continuation_lines(text: &str, indentation: &str) -> String {
    let mut lines = text.split('\n');
    let first = lines.next().unwrap_or_default();
    let mut output = first.to_string();
    for line in lines {
        output.push('\n');
        output.push_str(indentation);
        output.push_str(line);
    }
    output
}

fn indent_lines(text: &str, indentation: &str) -> String {
    text.split('\n')
        .map(|line| {
            if line.is_empty() {
                String::new()
            } else {
                format!("{indentation}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn serialize_table(node: &Value) -> String {
    let rows = table_matrix(node, serialize_table_cell);
    let width = rows.first().map_or(0, Vec::len);
    if rows.is_empty() || width == 0 {
        return String::new();
    }

    let first_row = node_content(node).first();
    let has_header = first_row.is_some_and(|row| {
        let cells = node_content(row);
        !cells.is_empty()
            && cells
                .iter()
                .all(|cell| node_type(cell) == Some("table_header"))
    });
    let header = if has_header {
        rows[0].clone()
    } else {
        vec![String::new(); width]
    };
    let body = if has_header { &rows[1..] } else { &rows[..] };
    let mut output = Vec::with_capacity(body.len() + 2);
    let separator = vec!["---".to_string(); width];
    output.push(markdown_table_line(&header));
    output.push(markdown_table_line(&separator));
    output.extend(body.iter().map(|row| markdown_table_line(row)));
    format!("{}\n\n", output.join("\n"))
}

fn serialize_table_cell(cell: &Value) -> String {
    let mut blocks = Vec::new();
    for child in node_content(cell) {
        let value = match node_type(child) {
            Some("math_display") => format!("${}$", node_text_content(child)),
            _ if is_textblock(child) => serialize_inline(child, false),
            _ => serialize_block(child).trim().to_string(),
        };
        if !value.is_empty() {
            blocks.push(value);
        }
    }
    normalize_table_cell_text(&blocks.join(" / "))
}

fn normalize_table_cell_text(value: &str) -> String {
    let characters = value.chars().collect::<Vec<_>>();
    let mut line_normalized = String::with_capacity(value.len());
    let mut index = 0;
    while index < characters.len() {
        if characters[index] == '\r'
            && characters.get(index + 1) == Some(&'\n')
        {
            index += 1;
            while characters.get(index) == Some(&'\n') {
                index += 1;
            }
            line_normalized.push_str(" / ");
            continue;
        }
        if characters[index] == '\n' {
            while characters.get(index) == Some(&'\n') {
                index += 1;
            }
            line_normalized.push_str(" / ");
            continue;
        }
        if characters[index] == '|' {
            line_normalized.push_str("\\|");
        } else {
            line_normalized.push(characters[index]);
        }
        index += 1;
    }
    collapse_whitespace(&line_normalized)
}

fn collapse_whitespace(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut pending_space = false;
    for character in value.chars() {
        if character.is_whitespace() {
            pending_space = true;
            continue;
        }
        if pending_space && !output.is_empty() {
            output.push(' ');
        }
        pending_space = false;
        output.push(character);
    }
    output.trim().to_string()
}

fn markdown_table_line(cells: &[String]) -> String {
    format!(
        "| {} |",
        cells
            .iter()
            .map(|cell| if cell.is_empty() { " " } else { cell.as_str() })
            .collect::<Vec<_>>()
            .join(" | ")
    )
}

fn collect_plain_text_blocks(node: &Value, blocks: &mut Vec<String>) {
    match node_type(node) {
        Some("math_inline" | "math_display") => {
            let text = node_text_content(node);
            if !text.trim().is_empty() {
                blocks.push(text);
            }
        }
        Some("table") => {
            let text = table_plain_text(node);
            if !text.is_empty() {
                blocks.push(text);
            }
        }
        Some("attachment_pending") => {}
        Some("attachment") => blocks.push(attachment_plain_text(node)),
        _ if is_textblock(node) => {
            if !is_atomic_boundary_paragraph(node) {
                blocks.push(inline_plain_text(node));
            }
        }
        _ => {
            for child in node_content(node) {
                collect_plain_text_blocks(child, blocks);
            }
        }
    }
}

fn attachment_plain_text(node: &Value) -> String {
    let name = node_attr_str(node, "displayName").unwrap_or("Attachment");
    let caption = node_attr_str(node, "caption").unwrap_or("").trim();
    let alt_text = node_attr_str(node, "altText").unwrap_or("").trim();
    let kind = node_attr_str(node, "kind").unwrap_or("file");
    let mut lines = vec![name.to_string()];
    if !caption.is_empty() {
        lines[0].push(' ');
        lines[0].push_str(caption);
    }
    if !alt_text.is_empty() && alt_text != caption {
        lines[0].push(' ');
        lines[0].push_str(alt_text);
    }

    if matches!(kind, "audio" | "video") {
        if let Some(trim) = media_trim_projection(node) {
            lines.push(format!("Playback trim: {trim}"));
        }
        for annotation in media_annotations(node) {
            let start_ms = annotation
                .get("startMs")
                .and_then(Value::as_u64)
                .unwrap_or_default();
            let end_ms = annotation.get("endMs").and_then(Value::as_u64);
            let mut line = media_annotation_time_label(start_ms, end_ms);
            for field in ["label", "tag", "note"] {
                let value = annotation
                    .get(field)
                    .and_then(Value::as_str)
                    .map(collapse_whitespace)
                    .unwrap_or_default();
                if !value.is_empty() {
                    line.push(' ');
                    line.push_str(&value);
                }
            }
            lines.push(line);
        }
    }
    lines.join("\n")
}

fn inline_plain_text(node: &Value) -> String {
    let mut output = String::new();
    for child in node_content(node) {
        match node_type(child) {
            Some("text") => output.push_str(
                child
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            ),
            Some("hard_break") => output.push('\n'),
            Some("math_inline" | "math_display") => output.push_str(&node_text_content(child)),
            _ => output.push_str(&node_text_content(child)),
        }
    }
    output
}

fn table_plain_text(table: &Value) -> String {
    table_matrix(table, |cell| {
        let mut cell_blocks = Vec::new();
        for child in node_content(cell) {
            let value = if is_textblock(child) {
                inline_plain_text(child)
            } else if matches!(node_type(child), Some("math_inline" | "math_display")) {
                node_text_content(child)
            } else {
                text_between(child, " / ")
            };
            cell_blocks.push(value);
        }
        collapse_horizontal_whitespace(&cell_blocks.join(" / "))
    })
    .into_iter()
    .map(|cells| cells.join("\t"))
    .collect::<Vec<_>>()
    .join("\n")
    .trim()
    .to_string()
}

fn table_matrix<F>(table: &Value, serialize_cell: F) -> Vec<Vec<String>>
where
    F: Fn(&Value) -> String,
{
    let mut rows = Vec::new();
    let mut occupied: Vec<Vec<bool>> = Vec::new();

    for (row_index, row) in node_content(table).iter().enumerate() {
        let reserved = occupied.get(row_index).cloned().unwrap_or_default();
        let mut cells = reserved
            .iter()
            .map(|is_occupied| {
                if *is_occupied {
                    Some(String::new())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let mut column = 0_usize;

        for cell in node_content(row) {
            let colspan = node_attr_u64(cell, "colspan").unwrap_or(1).max(1) as usize;
            let rowspan = node_attr_u64(cell, "rowspan").unwrap_or(1).max(1) as usize;
            while (0..colspan).any(|offset| {
                reserved
                    .get(column + offset)
                    .copied()
                    .unwrap_or(false)
            }) {
                column += 1;
            }
            if cells.len() < column + colspan {
                cells.resize(column + colspan, None);
            }
            cells[column] = Some(serialize_cell(cell));
            for offset in 1..colspan {
                cells[column + offset] = Some(String::new());
            }
            for row_offset in 1..rowspan {
                let future_row = row_index + row_offset;
                if occupied.len() <= future_row {
                    occupied.resize_with(future_row + 1, Vec::new);
                }
                if occupied[future_row].len() < column + colspan {
                    occupied[future_row].resize(column + colspan, false);
                }
                for column_offset in 0..colspan {
                    occupied[future_row][column + column_offset] = true;
                }
            }
            column += colspan;
        }

        rows.push(
            cells
                .into_iter()
                .map(Option::unwrap_or_default)
                .collect::<Vec<_>>(),
        );
    }

    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    for row in &mut rows {
        row.resize(width, String::new());
    }
    rows
}

fn text_between(node: &Value, block_separator: &str) -> String {
    let mut output = String::new();
    let mut first_block = true;
    for child in node_content(node) {
        append_text_between(child, block_separator, &mut first_block, &mut output);
    }
    output
}

fn append_text_between(
    node: &Value,
    block_separator: &str,
    first_block: &mut bool,
    output: &mut String,
) {
    if node_type(node) == Some("text") {
        output.push_str(
            node.get("text")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        );
        return;
    }

    let content = node_content(node);
    let is_leaf = content.is_empty();
    let leaf_text = if is_leaf && matches!(node_type(node), Some("hard_break" | "horizontal_rule")) {
        "\n"
    } else {
        ""
    };
    let separates_block = is_textblock(node)
        || (node_type(node) == Some("horizontal_rule") && !leaf_text.is_empty());
    if separates_block && !block_separator.is_empty() {
        if *first_block {
            *first_block = false;
        } else {
            output.push_str(block_separator);
        }
    }
    output.push_str(leaf_text);

    if !is_leaf {
        for child in content {
            append_text_between(child, block_separator, first_block, output);
        }
    }
}

fn collapse_horizontal_whitespace(value: &str) -> String {
    let mut output = String::new();
    let mut pending_space = false;
    for character in value.chars() {
        if matches!(character, ' ' | '\t') {
            pending_space = true;
        } else {
            if pending_space && !output.is_empty() && !output.ends_with('\n') {
                output.push(' ');
            }
            pending_space = false;
            output.push(character);
        }
    }
    output.trim().to_string()
}

fn node_content(node: &Value) -> &[Value] {
    node.get("content")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn node_type(node: &Value) -> Option<&str> {
    node.get("type").and_then(Value::as_str)
}

fn node_attr_str<'a>(node: &'a Value, key: &str) -> Option<&'a str> {
    node.get("attrs")?.get(key)?.as_str()
}

fn node_attr_u64(node: &Value, key: &str) -> Option<u64> {
    node.get("attrs")?.get(key)?.as_u64()
}

fn node_attr_bool(node: &Value, key: &str) -> Option<bool> {
    node.get("attrs")?.get(key)?.as_bool()
}

fn mark_type(mark: &Value) -> Option<&str> {
    mark.get("type").and_then(Value::as_str)
}

fn mark_attr_str<'a>(mark: &'a Value, key: &str) -> Option<&'a str> {
    mark.get("attrs")?.get(key)?.as_str()
}

fn is_textblock(node: &Value) -> bool {
    matches!(node_type(node), Some("paragraph" | "heading" | "code_block"))
}

fn is_atomic_boundary_paragraph(node: &Value) -> bool {
    node_type(node) == Some("paragraph") && node_attr_bool(node, "boundary") == Some(true)
}

fn node_text_content(node: &Value) -> String {
    if node_type(node) == Some("text") {
        return node
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
    }
    let mut output = String::new();
    for child in node_content(node) {
        output.push_str(&node_text_content(child));
    }
    output
}

fn escape_markdown_text_with_context(text: &str, escape_trailing_bang: bool) -> String {
    let mut output = String::with_capacity(text.len() + if escape_trailing_bang { 1 } else { 0 });
    let trailing_bang_index = if escape_trailing_bang {
        text.char_indices()
            .next_back()
            .filter(|(_, character)| *character == '!')
            .map(|(index, _)| index)
    } else {
        None
    };
    for (index, character) in text.char_indices() {
        if matches!(character, '\\' | '`' | '*' | '_' | '~' | '[' | ']' | '<' | '>' | '&')
            || trailing_bang_index == Some(index)
        {
            output.push('\\');
        }
        output.push(character);
    }
    output
}

fn escape_markdown_block_leading_syntax(value: &str) -> String {
    value
        .split('\n')
        .map(escape_markdown_block_leading_line)
        .collect::<Vec<_>>()
        .join("\n")
}

fn escape_markdown_block_leading_line(line: &str) -> String {
    let leading_spaces = line.bytes().take_while(|byte| *byte == b' ').count();
    if leading_spaces > 3 {
        return format!("&#32;{}", &line[1..]);
    }
    let prefix = &line[..leading_spaces];
    let rest = &line[leading_spaces..];
    if let Some(after_tab) = rest.strip_prefix('\t') {
        return format!("{prefix}&#9;{after_tab}");
    }

    if markdown_heading_start(rest)
        || rest.starts_with('>')
        || markdown_fence_start(rest)
        || markdown_rule_start(rest)
        || markdown_setext_underline_start(rest)
    {
        return format!("{prefix}\\{rest}");
    }
    if let Some(marker) = rest.chars().next()
        && matches!(marker, '-' | '+' | '*')
        && rest[marker.len_utf8()..]
            .chars()
            .next()
            .is_some_and(char::is_whitespace)
    {
        return format!("{prefix}\\{rest}");
    }
    if let Some(marker_index) = ordered_list_marker_index(rest) {
        let (before_marker, from_marker) = rest.split_at(marker_index);
        return format!("{prefix}{before_marker}\\{from_marker}");
    }
    line.to_string()
}

fn markdown_heading_start(value: &str) -> bool {
    let count = value.bytes().take_while(|byte| *byte == b'#').count();
    (1..=6).contains(&count)
        && value[count..]
            .chars()
            .next()
            .is_none_or(char::is_whitespace)
}

fn ordered_list_marker_index(value: &str) -> Option<usize> {
    let digit_count = value
        .bytes()
        .take_while(u8::is_ascii_digit)
        .count();
    if digit_count == 0 {
        return None;
    }
    let marker = *value.as_bytes().get(digit_count)?;
    if !matches!(marker, b'.' | b')') {
        return None;
    }
    value[digit_count + 1..]
        .chars()
        .next()
        .is_some_and(char::is_whitespace)
        .then_some(digit_count)
}

fn markdown_fence_start(value: &str) -> bool {
    let Some(first) = value.chars().next() else {
        return false;
    };
    if !matches!(first, '`' | '~') {
        return false;
    }
    value.chars().take_while(|character| *character == first).count() >= 3
}

fn markdown_rule_start(value: &str) -> bool {
    let mut marker = None;
    let mut marker_count = 0_usize;
    for character in value.trim_end().chars() {
        if character.is_whitespace() {
            continue;
        }
        if !matches!(character, '-' | '*' | '_') {
            return false;
        }
        if let Some(expected) = marker {
            if character != expected {
                return false;
            }
        } else {
            marker = Some(character);
        }
        marker_count += 1;
    }
    marker_count >= 3
}

fn markdown_setext_underline_start(value: &str) -> bool {
    let trimmed = value.trim_end();
    !trimmed.is_empty() && trimmed.chars().all(|character| character == '=')
}

fn escape_link_destination(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(character, '(' | ')' | '\\') {
            output.push('\\');
        }
        output.push(character);
    }
    output
}

fn inline_code_span(value: &str) -> String {
    let longest = longest_run(value, '`');
    let fence = "`".repeat((longest + 1).max(1));
    let all_spaces = !value.is_empty() && value.chars().all(|character| character == ' ');
    let needs_padding = value.starts_with('`')
        || value.ends_with('`')
        || (!all_spaces && value.starts_with(' ') && value.ends_with(' '));
    if needs_padding {
        format!("{fence} {value} {fence}")
    } else {
        format!("{fence}{value}{fence}")
    }
}

fn adaptive_code_fence(value: &str) -> String {
    "`".repeat((longest_run(value, '`') + 1).max(3))
}

fn longest_run(value: &str, target: char) -> usize {
    let mut longest = 0;
    let mut current = 0;
    for character in value.chars() {
        if character == target {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    longest
}
