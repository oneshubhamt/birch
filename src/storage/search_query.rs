use crate::search::{ParsedSearch, SearchField, SearchTerm};

/// SQLite FTS5 query material derived from Rustnote's storage-neutral search AST.
///
/// Keeping this translation beside the storage layer means `search.rs` owns
/// parsing/highlighting semantics while only storage knows FTS column names,
/// MATCH syntax, and the SQL used to select note candidates.
pub(super) fn preview_fts_expression(search: &ParsedSearch) -> String {
    search
        .groups
        .iter()
        .filter_map(|group| group_fts_expression(group))
        .collect::<Vec<_>>()
        .join(" OR ")
}

pub(super) fn candidate_sql_predicate(
    search: &ParsedSearch,
    note_alias: &str,
    first_parameter: usize,
) -> (String, Vec<String>) {
    if search.is_empty() {
        return ("1".to_string(), Vec::new());
    }

    let mut next_parameter = first_parameter;
    let mut parameters = Vec::new();
    let mut group_predicates = Vec::with_capacity(search.groups.len());

    for group in &search.groups {
        if let Some(expression) = group_fts_expression(group) {
            parameters.push(expression);
            group_predicates.push(format!(
                "{note_alias}.id IN (SELECT note_id FROM notes_fts WHERE notes_fts MATCH ?{next_parameter})"
            ));
            next_parameter += 1;
            continue;
        }

        let excluded = group
            .iter()
            .filter(|term| term.excluded)
            .map(fts_term)
            .collect::<Vec<_>>();
        if !excluded.is_empty() {
            parameters.push(format!("({})", excluded.join(" OR ")));
            group_predicates.push(format!(
                "{note_alias}.id NOT IN (SELECT note_id FROM notes_fts WHERE note_id IS NOT NULL AND notes_fts MATCH ?{next_parameter})"
            ));
            next_parameter += 1;
        }
    }

    if group_predicates.is_empty() {
        ("1".to_string(), parameters)
    } else {
        (format!("({})", group_predicates.join(" OR ")), parameters)
    }
}

fn group_fts_expression(group: &[SearchTerm]) -> Option<String> {
    let included = group
        .iter()
        .filter(|term| !term.excluded)
        .map(fts_term)
        .collect::<Vec<_>>();
    if included.is_empty() {
        return None;
    }

    let mut expression = included.join(" AND ");
    for excluded in group.iter().filter(|term| term.excluded) {
        expression = format!("({expression}) NOT {}", fts_term(excluded));
    }
    Some(format!("({expression})"))
}

fn fts_term(term: &SearchTerm) -> String {
    let escaped = term.value.replace('"', "\"\"");
    let phrase = if term.phrase || term.value.chars().any(char::is_whitespace) {
        format!("\"{escaped}\"")
    } else {
        format!("\"{escaped}\"*")
    };
    match term.field {
        SearchField::Any => phrase,
        SearchField::Title => format!("title : {phrase}"),
        SearchField::Body => format!("body : {phrase}"),
        SearchField::Tag => format!("tags : {phrase}"),
        SearchField::Attachment => format!("attachment_names : {phrase}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fts_translation_stays_inside_storage() {
        let parsed = ParsedSearch::parse(
            r#"title:"Civil Services" body:ethics -tag:draft OR -attachment:audio"#,
        );

        let preview = preview_fts_expression(&parsed);
        assert!(preview.contains("title : \"Civil Services\""));
        assert!(preview.contains("body : \"ethics\"*"));
        assert!(preview.contains("tags : \"draft\"*"));

        let (predicate, parameters) = candidate_sql_predicate(&parsed, "n", 4);
        assert!(predicate.contains("notes_fts MATCH ?4"));
        assert!(predicate.contains("notes_fts MATCH ?5"));
        assert_eq!(parameters.len(), 2);
        assert!(parameters[1].contains("attachment_names : \"audio\"*"));
    }

    #[test]
    fn all_negative_branch_uses_indexed_not_in_predicate() {
        let parsed = ParsedSearch::parse("-tag:archive -body:obsolete");
        let (predicate, parameters) = candidate_sql_predicate(&parsed, "note", 2);

        assert!(predicate.contains("note.id NOT IN"));
        assert!(predicate.contains("notes_fts MATCH ?2"));
        assert_eq!(parameters.len(), 1);
        assert!(parameters[0].contains("tags : \"archive\"*"));
        assert!(parameters[0].contains("body : \"obsolete\"*"));
    }
}
