use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttachmentMediaKind {
    Image,
    Pdf,
    Audio,
    Video,
    File,
}

impl AttachmentMediaKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Pdf => "pdf",
            Self::Audio => "audio",
            Self::Video => "video",
            Self::File => "file",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Image => "Image",
            Self::Pdf => "PDF",
            Self::Audio => "Audio",
            Self::Video => "Video",
            Self::File => "File",
        }
    }

    pub const fn filter_label(self) -> &'static str {
        match self {
            Self::Image => "Images",
            Self::Pdf => "PDF",
            Self::Audio => "Audio",
            Self::Video => "Video",
            Self::File => "Files",
        }
    }
}

struct AttachmentFormat {
    kind: AttachmentMediaKind,
    mime_types: &'static [&'static str],
    extensions: &'static [&'static str],
}

static FORMATS: &[AttachmentFormat] = &[
    AttachmentFormat {
        kind: AttachmentMediaKind::File,
        mime_types: &["text/plain"],
        extensions: &["txt", "md"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::File,
        mime_types: &["text/html"],
        extensions: &["html", "htm"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::Pdf,
        mime_types: &["application/pdf"],
        extensions: &["pdf"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::File,
        mime_types: &["application/json"],
        extensions: &["json"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::Image,
        mime_types: &["image/png"],
        extensions: &["png"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::Image,
        mime_types: &["image/jpeg"],
        extensions: &["jpg", "jpeg"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::Image,
        mime_types: &["image/gif"],
        extensions: &["gif"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::Image,
        mime_types: &["image/webp"],
        extensions: &["webp"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::Image,
        mime_types: &["image/svg+xml"],
        extensions: &["svg"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::Image,
        mime_types: &["image/heic", "image/heic-sequence"],
        extensions: &["heic"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::Image,
        mime_types: &["image/heif", "image/heif-sequence"],
        extensions: &["heif"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::Audio,
        mime_types: &["audio/mpeg"],
        extensions: &["mp3"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::Audio,
        mime_types: &["audio/mp4"],
        extensions: &["m4a"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::Audio,
        mime_types: &["audio/aac"],
        extensions: &["aac"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::Audio,
        mime_types: &["audio/wav"],
        extensions: &["wav"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::Audio,
        mime_types: &["audio/flac"],
        extensions: &["flac"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::Audio,
        mime_types: &["audio/ogg"],
        extensions: &["ogg", "oga", "opus"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::Audio,
        mime_types: &["audio/webm"],
        extensions: &["weba"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::Audio,
        mime_types: &["audio/matroska", "audio/x-matroska"],
        extensions: &["mka"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::Audio,
        mime_types: &["audio/amr"],
        extensions: &["amr"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::Audio,
        mime_types: &["audio/amr-wb"],
        extensions: &["awb"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::Audio,
        mime_types: &["audio/ac3"],
        extensions: &["ac3"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::Audio,
        mime_types: &["audio/eac3"],
        extensions: &["eac3", "ec3"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::Audio,
        mime_types: &["audio/aiff", "audio/x-aiff"],
        extensions: &["aif", "aiff", "aifc"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::Audio,
        mime_types: &["audio/x-caf"],
        extensions: &["caf"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::Audio,
        mime_types: &["audio/x-ms-wma"],
        extensions: &["wma"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::Video,
        mime_types: &["video/mp4"],
        extensions: &["mp4", "m4v"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::Video,
        mime_types: &["video/quicktime"],
        extensions: &["mov"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::Video,
        mime_types: &["video/webm"],
        extensions: &["webm"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::Video,
        mime_types: &["video/matroska", "video/x-matroska"],
        extensions: &["mkv"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::Video,
        mime_types: &["video/h265", "video/hevc"],
        extensions: &["h265", "hevc"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::Video,
        mime_types: &["video/mpeg"],
        extensions: &["mpeg", "mpg", "mpe"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::Video,
        mime_types: &["video/mp2t"],
        extensions: &["ts", "mts", "m2ts"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::Video,
        mime_types: &["video/3gpp"],
        extensions: &["3gp", "3gpp"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::Video,
        mime_types: &["video/3gpp2"],
        extensions: &["3g2", "3gpp2"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::Video,
        mime_types: &["video/ogg"],
        extensions: &["ogv"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::Video,
        mime_types: &["video/x-msvideo"],
        extensions: &["avi"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::Video,
        mime_types: &["video/x-ms-wmv"],
        extensions: &["wmv"],
    },
    AttachmentFormat {
        kind: AttachmentMediaKind::File,
        mime_types: &["application/zip"],
        extensions: &["zip"],
    },
];

pub fn extensions_for_kind(kind: AttachmentMediaKind) -> Vec<&'static str> {
    FORMATS
        .iter()
        .filter(|format| format.kind == kind)
        .flat_map(|format| format.extensions.iter().copied())
        .collect()
}

pub fn mime_type_from_path(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?;
    Some(mime_type_from_extension(extension).unwrap_or("application/octet-stream"))
}

pub fn mime_type_from_file_name(file_name: &str) -> Option<&'static str> {
    let extension = Path::new(file_name).extension()?.to_str()?;
    mime_type_from_extension(extension)
}

pub fn resolved_mime_type(file_name: &str, supplied: Option<&str>) -> Option<String> {
    let supplied = supplied.map(str::trim).filter(|value| !value.is_empty());
    if let Some(value) = supplied
        && !value.eq_ignore_ascii_case("application/octet-stream")
    {
        return Some(value.to_ascii_lowercase());
    }
    mime_type_from_file_name(file_name)
        .map(str::to_string)
        .or_else(|| supplied.map(str::to_ascii_lowercase))
}

pub fn kind(file_name: &str, mime_type: Option<&str>) -> AttachmentMediaKind {
    if let Some(mime_type) = mime_type.map(str::trim).filter(|value| !value.is_empty())
        && let Some(format) = format_for_mime_type(mime_type)
    {
        return format.kind;
    }

    mime_type_from_file_name(file_name)
        .and_then(format_for_mime_type)
        .map(|format| format.kind)
        .unwrap_or(AttachmentMediaKind::File)
}

pub fn kind_from_path(path: &Path) -> AttachmentMediaKind {
    let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
        return AttachmentMediaKind::File;
    };
    kind(file_name, mime_type_from_path(path))
}

pub fn preferred_extension(mime_type: Option<&str>) -> Option<&'static str> {
    mime_type
        .and_then(format_for_mime_type)
        .and_then(|format| format.extensions.first().copied())
}

pub fn preview_extension(file_name: &str, mime_type: Option<&str>) -> Option<&'static str> {
    let media_kind = kind(file_name, mime_type);
    if !matches!(
        media_kind,
        AttachmentMediaKind::Image | AttachmentMediaKind::Audio | AttachmentMediaKind::Video
    ) {
        return None;
    }

    let file_extension = Path::new(file_name).extension().and_then(|value| value.to_str());
    if let Some((format, extension)) = file_extension.and_then(format_for_extension)
        && format.kind == media_kind
    {
        return Some(extension);
    }

    mime_type
        .and_then(format_for_mime_type)
        .and_then(|format| format.extensions.first().copied())
}

fn format_for_mime_type(mime_type: &str) -> Option<&'static AttachmentFormat> {
    let mime_type = mime_type.trim();
    FORMATS.iter().find(|format| {
        format
            .mime_types
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(mime_type))
    })
}

fn mime_type_from_extension(extension: &str) -> Option<&'static str> {
    format_for_extension(extension).map(|(format, _)| format.mime_types[0])
}

fn format_for_extension(
    extension: &str,
) -> Option<(&'static AttachmentFormat, &'static str)> {
    let extension = extension.trim_start_matches('.');
    FORMATS.iter().find_map(|format| {
        format.extensions.iter().find_map(|candidate| {
            candidate
                .eq_ignore_ascii_case(extension)
                .then_some((format, *candidate))
        })
    })
}
