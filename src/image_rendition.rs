//! Shared policy for disposable image renditions used by the editor runtime.

pub const SCREEN_LONG_EDGE: u32 = 2_560;
pub const SCREEN_PIXEL_THRESHOLD: i64 = 8_000_000;
pub const SCREEN_LONG_EDGE_THRESHOLD: i64 = 4_096;
pub const CACHE_BUDGET_BYTES: u64 = 512 * 1024 * 1024;

pub fn needs_screen_rendition(
    pixel_width: Option<i64>,
    pixel_height: Option<i64>,
    format: Option<&str>,
    animated: Option<bool>,
) -> bool {
    if animated == Some(true) {
        return false;
    }

    let (Some(width), Some(height)) = (pixel_width, pixel_height) else {
        return false;
    };
    if width <= 0 || height <= 0 {
        return false;
    }

    let static_raster = format.is_some_and(|value| {
        if value.eq_ignore_ascii_case("jpeg") {
            return true;
        }
        // Formats that can contain animation are eligible only when the
        // lightweight container/MIME probe positively established they are static.
        (value.eq_ignore_ascii_case("png")
            || value.eq_ignore_ascii_case("webp")
            || value.eq_ignore_ascii_case("heic")
            || value.eq_ignore_ascii_case("heif"))
            && animated == Some(false)
    });
    if !static_raster {
        return false;
    }

    width.saturating_mul(height) > SCREEN_PIXEL_THRESHOLD
        || width.max(height) > SCREEN_LONG_EDGE_THRESHOLD
}
