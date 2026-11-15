use crate::domain::NoteSummary;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchField {
    Any,
    Title,
    Body,
    Tag,
    Attachment,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchTerm {
    pub field: SearchField,
    pub value: String,
    pub phrase: bool,
    pub excluded: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParsedSearch {
    pub groups: Vec<Vec<SearchTerm>>,
}

impl ParsedSearch {
    pub fn parse(input: &str) -> Self {
        let tokens = tokenize(input);
        let mut groups = vec![Vec::new()];
        let mut exclude_next = false;
        for token in tokens {
            if token.eq_ignore_ascii_case("OR") {
                if groups.last().is_some_and(|group| !group.is_empty()) {
                    groups.push(Vec::new());
                }
                exclude_next = false;
                continue;
            }
            if token.eq_ignore_ascii_case("AND") {
                continue;
            }
            if token.eq_ignore_ascii_case("NOT") {
                exclude_next = true;
                continue;
            }
            if let Some(mut term) = parse_term(&token) {
                if exclude_next {
                    term.excluded = true;
                    exclude_next = false;
                }
                if let Some(group) = groups.last_mut() {
                    group.push(term);
                } else {
                    groups.push(vec![term]);
                }
            }
        }
        groups.retain(|group| !group.is_empty());
        Self { groups }
    }

    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    pub fn highlight_terms(&self, field: SearchField) -> Vec<String> {
        let mut terms = Vec::new();
        let mut normalized_terms = Vec::new();
        for term in self.groups.iter().flatten().filter(|term| !term.excluded) {
            if term.field == SearchField::Any || term.field == field {
                let normalized = normalize_search_value(&term.value);
                if !normalized.is_empty() && !normalized_terms.contains(&normalized) {
                    normalized_terms.push(normalized);
                    terms.push(term.value.clone());
                }
            }
        }
        terms
    }

    pub fn first_highlight_term(&self, field: SearchField) -> Option<String> {
        self.highlight_terms(field).into_iter().next()
    }
}

pub fn contextual_summary_preview(note: &NoteSummary, search: &ParsedSearch) -> String {
    contextual_preview_fields(&note.preview_text, &note.attachment_names, search, || {
        note.preview()
    })
}

fn contextual_preview_fields(
    preview_text: &str,
    attachment_names: &[String],
    search: &ParsedSearch,
    fallback: impl FnOnce() -> String,
) -> String {
    let body_terms = search.highlight_terms(SearchField::Body);
    if let Some((start, end)) = first_match_range(preview_text, &body_terms) {
        return window_around_match(preview_text, start, end, 48, 112);
    }

    let attachment_terms = search.highlight_terms(SearchField::Attachment);
    if let Some(name) = attachment_names
        .iter()
        .find(|name| contains_any(name, &attachment_terms))
    {
        return format!("Attachment: {name}");
    }

    fallback()
}

pub fn highlight_segments(text: &str, terms: &[String]) -> Vec<(String, bool)> {
    if terms.is_empty() || text.is_empty() {
        return vec![(text.to_string(), false)];
    }

    let normalized_text = NormalizedText::new(text);
    let mut ranges = Vec::new();
    for term in terms {
        let normalized_term = normalize_search_value(term);
        if normalized_term.is_empty() {
            continue;
        }
        let mut offset = 0;
        while let Some(position) = normalized_text.value[offset..].find(&normalized_term) {
            let normalized_start = offset + position;
            let normalized_end = normalized_start + normalized_term.len();
            if let Some(range) = normalized_text.original_range(normalized_start, normalized_end) {
                ranges.push(range);
            }
            offset = normalized_end;
            if offset >= normalized_text.value.len() {
                break;
            }
        }
    }
    if ranges.is_empty() {
        return vec![(text.to_string(), false)];
    }
    ranges.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in ranges {
        if let Some(last) = merged.last_mut()
            && start <= last.1
        {
            last.1 = last.1.max(end);
            continue;
        }
        merged.push((start, end));
    }

    let mut boundaries = Vec::new();
    let mut cursor = 0;
    for (start, end) in merged {
        if cursor < start {
            boundaries.push((text[cursor..start].to_string(), false));
        }
        boundaries.push((text[start..end].to_string(), true));
        cursor = end;
    }
    if cursor < text.len() {
        boundaries.push((text[cursor..].to_string(), false));
    }
    boundaries
}

fn tokenize(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for character in input.chars() {
        match character {
            '"' => {
                quoted = !quoted;
                current.push(character);
            }
            character if character.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(character),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn parse_term(raw: &str) -> Option<SearchTerm> {
    let value = raw.trim();
    if value.is_empty() {
        return None;
    }
    let (excluded, value) = match value.strip_prefix('-') {
        Some(stripped) if !stripped.is_empty() => (true, stripped),
        _ => (false, value),
    };

    let (field, remainder) = if let Some((prefix, remainder)) = value.split_once(':') {
        let field = match prefix.to_ascii_lowercase().as_str() {
            "title" => Some(SearchField::Title),
            "body" | "content" => Some(SearchField::Body),
            "tag" => Some(SearchField::Tag),
            "file" | "attachment" => Some(SearchField::Attachment),
            _ => None,
        };
        field.map_or((SearchField::Any, value), |field| (field, remainder))
    } else if let Some(remainder) = value.strip_prefix('#') {
        (SearchField::Tag, remainder)
    } else {
        (SearchField::Any, value)
    };

    let phrase = remainder.starts_with('"') && remainder.ends_with('"') && remainder.len() >= 2;
    let cleaned = if phrase {
        &remainder[1..remainder.len() - 1]
    } else {
        remainder
    }
    .trim();
    if cleaned.is_empty() {
        return None;
    }

    Some(SearchTerm {
        field,
        value: cleaned.to_string(),
        phrase,
        excluded,
    })
}

fn contains_any(value: &str, terms: &[String]) -> bool {
    let normalized_value = normalize_search_value(value);
    terms.iter().any(|term| {
        let normalized_term = normalize_search_value(term);
        !normalized_term.is_empty() && normalized_value.contains(&normalized_term)
    })
}

fn first_match_range(value: &str, terms: &[String]) -> Option<(usize, usize)> {
    let normalized_value = NormalizedText::new(value);
    terms
        .iter()
        .filter_map(|term| {
            let normalized_term = normalize_search_value(term);
            if normalized_term.is_empty() {
                return None;
            }
            let start = normalized_value.value.find(&normalized_term)?;
            normalized_value.original_range(start, start + normalized_term.len())
        })
        .min_by_key(|range| range.0)
}

#[derive(Clone, Copy, Debug)]
struct NormalizedUnit {
    normalized_start: usize,
    normalized_end: usize,
    original_start: usize,
    original_end: usize,
}

#[derive(Debug)]
struct NormalizedText {
    value: String,
    units: Vec<NormalizedUnit>,
}

impl NormalizedText {
    fn new(input: &str) -> Self {
        let mut normalized = Self {
            value: String::new(),
            units: Vec::new(),
        };
        let mut characters = input.char_indices().peekable();
        while let Some((original_start, character)) = characters.next() {
            let original_end = characters
                .peek()
                .map_or(input.len(), |(next_start, _)| *next_start);
            for lowered in character.to_lowercase() {
                append_search_fold(&mut normalized, lowered, original_start, original_end);
            }
        }
        normalized
    }

    fn original_range(
        &self,
        normalized_start: usize,
        normalized_end: usize,
    ) -> Option<(usize, usize)> {
        if normalized_start >= normalized_end {
            return None;
        }
        let first = self
            .units
            .iter()
            .find(|unit| unit.normalized_end > normalized_start)?;
        let last = self
            .units
            .iter()
            .rev()
            .find(|unit| unit.normalized_start < normalized_end)?;
        Some((first.original_start, last.original_end))
    }

    fn push(&mut self, folded: &str, original_start: usize, original_end: usize) {
        if folded.is_empty() {
            return;
        }
        let normalized_start = self.value.len();
        self.value.push_str(folded);
        self.units.push(NormalizedUnit {
            normalized_start,
            normalized_end: self.value.len(),
            original_start,
            original_end,
        });
    }
}

fn normalize_search_value(value: &str) -> String {
    NormalizedText::new(value).value
}

fn append_search_fold(
    normalized: &mut NormalizedText,
    character: char,
    original_start: usize,
    original_end: usize,
) {
    if is_combining_mark(character) {
        if let Some(last) = normalized.units.last_mut() {
            last.original_end = original_end;
        }
        return;
    }
    let folded = match character {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' | 'ǎ' | 'ǟ' | 'ǡ' | 'ǻ' | 'ȁ' | 'ȃ'
        | 'ȧ' | 'ạ' | 'ả' | 'ấ' | 'ầ' | 'ẩ' | 'ẫ' | 'ậ' | 'ắ' | 'ằ' | 'ẳ' | 'ẵ' | 'ặ' => {
            "a"
        }
        'æ' | 'ǣ' | 'ǽ' => "ae",
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => "c",
        'ď' | 'đ' | 'ð' => "d",
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' | 'ȅ' | 'ȇ' | 'ẹ' | 'ẻ' | 'ẽ' | 'ế'
        | 'ề' | 'ể' | 'ễ' | 'ệ' => "e",
        'ƒ' => "f",
        'ĝ' | 'ğ' | 'ġ' | 'ģ' | 'ǧ' => "g",
        'ĥ' | 'ħ' => "h",
        'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' | 'ǐ' | 'ȉ' | 'ȋ' | 'ị' | 'ỉ' => {
            "i"
        }
        'ĵ' => "j",
        'ķ' => "k",
        'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => "l",
        'ñ' | 'ń' | 'ņ' | 'ň' | 'ŉ' | 'ŋ' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ŏ' | 'ő' | 'ǒ' | 'ǫ' | 'ǭ' | 'ȍ' | 'ȏ' | 'ȯ'
        | 'ȱ' | 'ọ' | 'ỏ' | 'ố' | 'ồ' | 'ổ' | 'ỗ' | 'ộ' | 'ớ' | 'ờ' | 'ở' | 'ỡ' | 'ợ' => {
            "o"
        }
        'œ' => "oe",
        'ŕ' | 'ŗ' | 'ř' | 'ȑ' | 'ȓ' => "r",
        'ś' | 'ŝ' | 'ş' | 'š' | 'ș' => "s",
        'ß' => "ss",
        'ţ' | 'ť' | 'ŧ' | 'ț' => "t",
        'þ' => "th",
        'ù' | 'ú' | 'û' | 'ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' | 'ǔ' | 'ǖ' | 'ǘ' | 'ǚ' | 'ǜ'
        | 'ȕ' | 'ȗ' | 'ụ' | 'ủ' | 'ứ' | 'ừ' | 'ử' | 'ữ' | 'ự' => "u",
        'ŵ' => "w",
        'ý' | 'ÿ' | 'ŷ' | 'ȳ' | 'ỳ' | 'ỵ' | 'ỷ' | 'ỹ' => "y",
        'ź' | 'ż' | 'ž' => "z",
        _ => {
            let mut buffer = [0_u8; 4];
            normalized.push(
                character.encode_utf8(&mut buffer),
                original_start,
                original_end,
            );
            return;
        }
    };
    normalized.push(folded, original_start, original_end);
}

fn is_combining_mark(character: char) -> bool {
    matches!(
        character as u32,
        0x0300..=0x036f
            | 0x1ab0..=0x1aff
            | 0x1dc0..=0x1dff
            | 0x20d0..=0x20ff
            | 0xfe20..=0xfe2f
    )
}

fn window_around_match(
    value: &str,
    start: usize,
    end: usize,
    before: usize,
    after: usize,
) -> String {
    let start = floor_char_boundary(value, start.saturating_sub(before));
    let end = ceil_char_boundary(value, (end + after).min(value.len()));
    let mut snippet = value[start..end]
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if start > 0 {
        snippet.insert(0, '…');
    }
    if end < value.len() {
        snippet.push('…');
    }
    snippet
}

fn floor_char_boundary(value: &str, mut index: usize) -> usize {
    index = index.min(value.len());
    while index > 0 && !value.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn ceil_char_boundary(value: &str, mut index: usize) -> usize {
    index = index.min(value.len());
    while index < value.len() && !value.is_char_boundary(index) {
        index += 1;
    }
    index
}
