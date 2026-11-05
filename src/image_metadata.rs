use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

use crate::attachment_media::{AttachmentMediaKind, kind};

const MAX_EXIF_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ImageAttachmentMetadata {
    pub pixel_width: Option<i64>,
    pub pixel_height: Option<i64>,
    pub format: Option<String>,
    pub orientation: Option<i64>,
    pub has_alpha: Option<bool>,
    pub animated: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ImageContainerTraits {
    orientation: Option<i64>,
    has_alpha: Option<bool>,
    animated: Option<bool>,
}

pub(crate) fn probe_image_attachment(
    path: &Path,
    file_name: &str,
    mime_type: Option<&str>,
) -> ImageAttachmentMetadata {
    if kind(file_name, mime_type) != AttachmentMediaKind::Image {
        return ImageAttachmentMetadata::default();
    }

    let format = image_format(file_name, mime_type).map(str::to_string);
    let platform = crate::platform::image_properties(path, format.as_deref());
    let dimensions = imagesize::size(path)
        .ok()
        .map(|size| (size.width, size.height))
        .or(platform.dimensions);
    let traits = image_container_traits(path, format.as_deref(), mime_type);

    ImageAttachmentMetadata {
        pixel_width: dimensions
            .and_then(|(width, _)| i64::try_from(width).ok())
            .filter(|value| *value > 0),
        pixel_height: dimensions
            .and_then(|(_, height)| i64::try_from(height).ok())
            .filter(|value| *value > 0),
        format,
        orientation: traits.orientation,
        has_alpha: traits.has_alpha.or(platform.has_alpha),
        animated: traits.animated,
    }
}

fn image_format(file_name: &str, mime_type: Option<&str>) -> Option<&'static str> {
    let extension = Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase);
    match extension.as_deref() {
        Some("png") => return Some("PNG"),
        Some("jpg") | Some("jpeg") => return Some("JPEG"),
        Some("gif") => return Some("GIF"),
        Some("webp") => return Some("WebP"),
        Some("svg") => return Some("SVG"),
        Some("heic") => return Some("HEIC"),
        Some("heif") => return Some("HEIF"),
        _ => {}
    }

    match mime_type.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
        Some("image/png") => Some("PNG"),
        Some("image/jpeg") => Some("JPEG"),
        Some("image/gif") => Some("GIF"),
        Some("image/webp") => Some("WebP"),
        Some("image/svg+xml") => Some("SVG"),
        Some("image/heic") | Some("image/heic-sequence") => Some("HEIC"),
        Some("image/heif") | Some("image/heif-sequence") => Some("HEIF"),
        _ => None,
    }
}

fn image_container_traits(
    path: &Path,
    format: Option<&str>,
    mime_type: Option<&str>,
) -> ImageContainerTraits {
    match format {
        Some("JPEG") => ImageContainerTraits {
            orientation: jpeg_exif_orientation(path),
            has_alpha: Some(false),
            animated: Some(false),
        },
        Some("PNG") => png_traits(path).unwrap_or_default(),
        Some("GIF") => gif_traits(path).unwrap_or(ImageContainerTraits {
            orientation: None,
            has_alpha: None,
            animated: Some(true),
        }),
        Some("WebP") => webp_traits(path).unwrap_or_default(),
        Some("HEIC") | Some("HEIF") => ImageContainerTraits {
            orientation: None,
            has_alpha: None,
            animated: Some(matches!(
                mime_type.map(str::trim).map(str::to_ascii_lowercase).as_deref(),
                Some("image/heic-sequence") | Some("image/heif-sequence")
            )),
        },
        _ => ImageContainerTraits::default(),
    }
}

fn jpeg_exif_orientation(path: &Path) -> Option<i64> {
    let mut file = File::open(path).ok()?;
    let mut soi = [0_u8; 2];
    file.read_exact(&mut soi).ok()?;
    if soi != [0xff, 0xd8] {
        return None;
    }

    loop {
        let marker = read_jpeg_marker(&mut file)?;
        if marker == 0xd9 || marker == 0xda {
            return None;
        }
        if marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
            continue;
        }
        let mut length_bytes = [0_u8; 2];
        file.read_exact(&mut length_bytes).ok()?;
        let length = usize::from(u16::from_be_bytes(length_bytes));
        if length < 2 {
            return None;
        }
        let payload_len = length - 2;
        if marker == 0xe1 {
            let mut payload = vec![0_u8; payload_len];
            file.read_exact(&mut payload).ok()?;
            if let Some(orientation) = exif_orientation(&payload) {
                return Some(orientation);
            }
        } else {
            skip_bytes(&mut file, u64::try_from(payload_len).ok()?).ok()?;
        }
    }
}

fn read_jpeg_marker(file: &mut File) -> Option<u8> {
    let mut byte = [0_u8; 1];
    loop {
        file.read_exact(&mut byte).ok()?;
        if byte[0] != 0xff {
            continue;
        }
        loop {
            file.read_exact(&mut byte).ok()?;
            match byte[0] {
                0xff => continue,
                0x00 => break,
                marker => return Some(marker),
            }
        }
    }
}

fn png_traits(path: &Path) -> Option<ImageContainerTraits> {
    let mut file = File::open(path).ok()?;
    let mut signature = [0_u8; 8];
    file.read_exact(&mut signature).ok()?;
    if &signature != b"\x89PNG\r\n\x1a\n" {
        return None;
    }

    let mut orientation = None;
    let mut has_alpha = None;
    let mut animated = false;
    loop {
        let mut length_bytes = [0_u8; 4];
        file.read_exact(&mut length_bytes).ok()?;
        let length = u64::from(u32::from_be_bytes(length_bytes));
        let mut chunk_type = [0_u8; 4];
        file.read_exact(&mut chunk_type).ok()?;

        match &chunk_type {
            b"IHDR" if (10..=64).contains(&length) => {
                let mut data = vec![0_u8; usize::try_from(length).ok()?];
                file.read_exact(&mut data).ok()?;
                let color_type = data[9];
                has_alpha = Some(matches!(color_type, 4 | 6));
                skip_bytes(&mut file, 4).ok()?;
            }
            b"tRNS" => {
                has_alpha = Some(true);
                skip_bytes(&mut file, length.saturating_add(4)).ok()?;
            }
            b"acTL" => {
                animated = true;
                skip_bytes(&mut file, length.saturating_add(4)).ok()?;
            }
            b"eXIf" if length <= MAX_EXIF_BYTES => {
                let mut data = vec![0_u8; usize::try_from(length).ok()?];
                file.read_exact(&mut data).ok()?;
                orientation = orientation.or_else(|| exif_orientation(&data));
                skip_bytes(&mut file, 4).ok()?;
            }
            b"IDAT" | b"IEND" => {
                return Some(ImageContainerTraits {
                    orientation,
                    has_alpha,
                    animated: Some(animated),
                });
            }
            _ => skip_bytes(&mut file, length.saturating_add(4)).ok()?,
        }
    }
}

fn gif_traits(path: &Path) -> Option<ImageContainerTraits> {
    let mut file = File::open(path).ok()?;
    let mut header = [0_u8; 6];
    file.read_exact(&mut header).ok()?;
    if &header != b"GIF87a" && &header != b"GIF89a" {
        return None;
    }
    let mut logical_screen = [0_u8; 7];
    file.read_exact(&mut logical_screen).ok()?;
    if logical_screen[4] & 0x80 != 0 {
        let table_entries = 1_u64 << ((logical_screen[4] & 0x07) + 1);
        skip_bytes(&mut file, table_entries.saturating_mul(3)).ok()?;
    }

    let mut frames = 0_u32;
    let mut has_alpha = false;
    loop {
        let mut introducer = [0_u8; 1];
        if file.read_exact(&mut introducer).is_err() {
            break;
        }
        match introducer[0] {
            0x2c => {
                frames = frames.saturating_add(1);
                let mut descriptor = [0_u8; 9];
                file.read_exact(&mut descriptor).ok()?;
                if descriptor[8] & 0x80 != 0 {
                    let table_entries = 1_u64 << ((descriptor[8] & 0x07) + 1);
                    skip_bytes(&mut file, table_entries.saturating_mul(3)).ok()?;
                }
                skip_bytes(&mut file, 1).ok()?;
                skip_gif_sub_blocks(&mut file).ok()?;
            }
            0x21 => {
                let mut label = [0_u8; 1];
                file.read_exact(&mut label).ok()?;
                if label[0] == 0xf9 {
                    let mut block_size = [0_u8; 1];
                    file.read_exact(&mut block_size).ok()?;
                    let size = usize::from(block_size[0]);
                    let mut data = vec![0_u8; size];
                    file.read_exact(&mut data).ok()?;
                    if data.first().is_some_and(|packed| packed & 0x01 != 0) {
                        has_alpha = true;
                    }
                    let mut terminator = [0_u8; 1];
                    file.read_exact(&mut terminator).ok()?;
                    if terminator[0] != 0 {
                        return None;
                    }
                } else {
                    skip_gif_sub_blocks(&mut file).ok()?;
                }
            }
            0x3b => break,
            _ => return None,
        }
    }
    Some(ImageContainerTraits {
        orientation: None,
        has_alpha: Some(has_alpha),
        animated: Some(frames > 1),
    })
}

fn skip_gif_sub_blocks(file: &mut File) -> std::io::Result<()> {
    loop {
        let mut size = [0_u8; 1];
        file.read_exact(&mut size)?;
        if size[0] == 0 {
            return Ok(());
        }
        skip_bytes(file, u64::from(size[0]))?;
    }
}

fn webp_traits(path: &Path) -> Option<ImageContainerTraits> {
    let mut file = File::open(path).ok()?;
    let mut header = [0_u8; 12];
    file.read_exact(&mut header).ok()?;
    if &header[..4] != b"RIFF" || &header[8..] != b"WEBP" {
        return None;
    }

    let mut orientation = None;
    let mut has_alpha = false;
    let mut animated = false;
    loop {
        let mut chunk_header = [0_u8; 8];
        if file.read_exact(&mut chunk_header).is_err() {
            break;
        }
        let chunk_type = &chunk_header[..4];
        let length = u64::from(u32::from_le_bytes(chunk_header[4..8].try_into().ok()?));
        let padding = length & 1;
        if chunk_type == b"VP8X" && length > 0 {
            let mut flags = [0_u8; 1];
            file.read_exact(&mut flags).ok()?;
            has_alpha |= flags[0] & 0x10 != 0;
            animated |= flags[0] & 0x02 != 0;
            skip_bytes(&mut file, length.saturating_sub(1).saturating_add(padding)).ok()?;
        } else if chunk_type == b"VP8L" && length >= 5 {
            let mut lossless_header = [0_u8; 5];
            file.read_exact(&mut lossless_header).ok()?;
            if lossless_header[0] == 0x2f {
                let bits = u32::from_le_bytes(lossless_header[1..5].try_into().ok()?);
                has_alpha |= (bits >> 28) & 1 != 0;
            }
            skip_bytes(&mut file, length.saturating_sub(5).saturating_add(padding)).ok()?;
        } else if chunk_type == b"ALPH" {
            has_alpha = true;
            skip_bytes(&mut file, length.saturating_add(padding)).ok()?;
        } else if chunk_type == b"ANIM" || chunk_type == b"ANMF" {
            animated = true;
            skip_bytes(&mut file, length.saturating_add(padding)).ok()?;
        } else if chunk_type == b"EXIF" && length <= MAX_EXIF_BYTES {
            let mut data = vec![0_u8; usize::try_from(length).ok()?];
            file.read_exact(&mut data).ok()?;
            orientation = orientation.or_else(|| exif_orientation(&data));
            skip_bytes(&mut file, padding).ok()?;
        } else {
            skip_bytes(&mut file, length.saturating_add(padding)).ok()?;
        }
    }
    Some(ImageContainerTraits {
        orientation,
        has_alpha: Some(has_alpha),
        animated: Some(animated),
    })
}

fn exif_orientation(bytes: &[u8]) -> Option<i64> {
    let tiff = bytes.strip_prefix(b"Exif\0\0").unwrap_or(bytes);
    if tiff.len() < 8 {
        return None;
    }
    let little_endian = match &tiff[..2] {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    if read_u16(tiff, 2, little_endian)? != 42 {
        return None;
    }
    let ifd_offset = usize::try_from(read_u32(tiff, 4, little_endian)?).ok()?;
    let entry_count = usize::from(read_u16(tiff, ifd_offset, little_endian)?);
    let entries_start = ifd_offset.checked_add(2)?;
    for index in 0..entry_count {
        let entry = entries_start.checked_add(index.checked_mul(12)?)?;
        let entry_end = entry.checked_add(12)?;
        if entry_end > tiff.len() {
            return None;
        }
        if read_u16(tiff, entry, little_endian)? != 0x0112 {
            continue;
        }
        let field_type = read_u16(tiff, entry + 2, little_endian)?;
        let count = read_u32(tiff, entry + 4, little_endian)?;
        if field_type != 3 || count == 0 {
            return None;
        }
        let value = if count == 1 {
            read_u16(tiff, entry + 8, little_endian)?
        } else {
            let offset = usize::try_from(read_u32(tiff, entry + 8, little_endian)?).ok()?;
            read_u16(tiff, offset, little_endian)?
        };
        return (1..=8).contains(&value).then_some(i64::from(value));
    }
    None
}

fn read_u16(bytes: &[u8], offset: usize, little_endian: bool) -> Option<u16> {
    let raw: [u8; 2] = bytes.get(offset..offset.checked_add(2)?)?.try_into().ok()?;
    Some(if little_endian {
        u16::from_le_bytes(raw)
    } else {
        u16::from_be_bytes(raw)
    })
}

fn read_u32(bytes: &[u8], offset: usize, little_endian: bool) -> Option<u32> {
    let raw: [u8; 4] = bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?;
    Some(if little_endian {
        u32::from_le_bytes(raw)
    } else {
        u32::from_be_bytes(raw)
    })
}

fn skip_bytes(file: &mut File, bytes: u64) -> std::io::Result<()> {
    let offset = i64::try_from(bytes)
        .map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "image chunk is too large")
        })?;
    file.seek(SeekFrom::Current(offset))?;
    Ok(())
}
