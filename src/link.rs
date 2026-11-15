use url::Url;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkValidationError {
    Empty,
    Whitespace,
    IncompleteWebAddress,
    IncompleteEmailAddress,
    UnsupportedScheme,
}

pub fn normalize_safe_href(raw: &str) -> Result<String, LinkValidationError> {
    let value = raw.trim();
    if value.is_empty() {
        return Err(LinkValidationError::Empty);
    }
    if value
        .chars()
        .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err(LinkValidationError::Whitespace);
    }

    let parsed = Url::parse(value).map_err(|_| parse_error_for(value))?;
    match parsed.scheme() {
        "https" | "http" => match parsed.host_str() {
            Some(host) if !host.is_empty() && !matches!(host, "." | "..") => Ok(value.to_string()),
            _ => Err(LinkValidationError::IncompleteWebAddress),
        },
        "mailto" => {
            let address = parsed.path();
            let Some((local, domain)) = address.rsplit_once('@') else {
                return Err(LinkValidationError::IncompleteEmailAddress);
            };
            if local.is_empty() || domain.is_empty() {
                return Err(LinkValidationError::IncompleteEmailAddress);
            }
            Ok(value.to_string())
        }
        _ => Err(LinkValidationError::UnsupportedScheme),
    }
}

pub fn is_safe_href(value: &str) -> bool {
    normalize_safe_href(value).is_ok_and(|normalized| normalized == value)
}

pub fn is_safe_web_href(value: &str) -> bool {
    is_safe_href(value)
        && Url::parse(value).is_ok_and(|parsed| matches!(parsed.scheme(), "https" | "http"))
}

fn parse_error_for(value: &str) -> LinkValidationError {
    let lowercase = value.to_ascii_lowercase();
    if lowercase.starts_with("https://") || lowercase.starts_with("http://") {
        LinkValidationError::IncompleteWebAddress
    } else if lowercase.starts_with("mailto:") {
        LinkValidationError::IncompleteEmailAddress
    } else {
        LinkValidationError::UnsupportedScheme
    }
}
