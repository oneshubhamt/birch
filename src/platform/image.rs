use std::path::Path;

#[cfg(all(unix, not(any(target_os = "macos", target_os = "android", target_os = "ios"))))]
use std::{
    env,
    os::unix::fs::PermissionsExt,
    sync::OnceLock,
};

#[cfg(any(
    target_os = "macos",
    target_os = "windows",
    all(
        unix,
        not(any(target_os = "macos", target_os = "android", target_os = "ios"))
    )
))]
use std::process::{Command, Output};

#[cfg(target_os = "macos")]
pub(super) fn image_properties(path: &Path, format: Option<&str>) -> super::PlatformImageProperties {
    if !is_heif_format(format) {
        return super::PlatformImageProperties::default();
    }
    let output = match Command::new("/usr/bin/sips")
        .arg("-g")
        .arg("pixelWidth")
        .arg("-g")
        .arg("pixelHeight")
        .arg("-g")
        .arg("hasAlpha")
        .arg(path)
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => return super::PlatformImageProperties::default(),
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut width = None;
    let mut height = None;
    let mut has_alpha = None;
    for line in text.lines().map(str::trim) {
        if let Some(value) = line.strip_prefix("pixelWidth:") {
            width = value.trim().parse::<usize>().ok().filter(|value| *value > 0);
        } else if let Some(value) = line.strip_prefix("pixelHeight:") {
            height = value.trim().parse::<usize>().ok().filter(|value| *value > 0);
        } else if let Some(value) = line.strip_prefix("hasAlpha:") {
            has_alpha = parse_bool(value);
        }
    }
    super::PlatformImageProperties {
        dimensions: width.zip(height),
        has_alpha,
    }
}

#[cfg(target_os = "windows")]
pub(super) fn image_properties(path: &Path, format: Option<&str>) -> super::PlatformImageProperties {
    if !is_heif_format(format) {
        return super::PlatformImageProperties::default();
    }
    const SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName PresentationCore
$sourcePath = [Environment]::GetEnvironmentVariable('RUSTNOTE_IMAGE_SOURCE')
$input = [IO.File]::Open($sourcePath, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
try {
  $decoder = [System.Windows.Media.Imaging.BitmapDecoder]::Create(
    $input,
    [System.Windows.Media.Imaging.BitmapCreateOptions]::PreservePixelFormat,
    [System.Windows.Media.Imaging.BitmapCacheOption]::OnLoad)
  if ($decoder.Frames.Count -lt 1) { throw 'The image contains no decodable frame.' }
  $bitmap = $decoder.Frames[0]
  Write-Output ("{0}x{1}" -f $bitmap.PixelWidth, $bitmap.PixelHeight)
} finally { $input.Dispose() }
"#;
    let output = Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            SCRIPT,
        ])
        .env("RUSTNOTE_IMAGE_SOURCE", path)
        .output();
    let Ok(output) = output else {
        return super::PlatformImageProperties::default();
    };
    if !output.status.success() {
        return super::PlatformImageProperties::default();
    }
    let dimensions = String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(parse_dimensions);
    super::PlatformImageProperties {
        dimensions,
        has_alpha: None,
    }
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "android", target_os = "ios"))))]
pub(super) fn image_properties(path: &Path, format: Option<&str>) -> super::PlatformImageProperties {
    if !is_heif_format(format) {
        return super::PlatformImageProperties::default();
    }

    let tools = linux_image_tools();
    if let Some(executable) = tools.identify {
        let mut command = Command::new(executable);
        if executable == "magick" {
            command.arg("identify");
        }
        if let Ok(output) = command.arg("-format").arg("%wx%h").arg(path).output()
            && output.status.success()
            && let Some(dimensions) = String::from_utf8_lossy(&output.stdout)
                .lines()
                .find_map(parse_dimensions)
        {
            return super::PlatformImageProperties {
                dimensions: Some(dimensions),
                has_alpha: None,
            };
        }
    }

    if tools.heif_info {
        if let Ok(output) = Command::new("heif-info").arg(path).output()
            && output.status.success()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            if let Some(properties) = parse_heif_info_properties(&text) {
                return properties;
            }
        }
    }

    super::PlatformImageProperties::default()
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub(super) fn image_properties(path: &Path, format: Option<&str>) -> super::PlatformImageProperties {
    if !is_heif_format(format) {
        return super::PlatformImageProperties::default();
    }
    super::mobile::adapter()
        .map(|adapter| adapter.image_properties(path))
        .unwrap_or_default()
}

#[cfg(not(any(unix, windows)))]
pub(super) fn image_properties(
    _path: &Path,
    _format: Option<&str>,
) -> super::PlatformImageProperties {
    super::PlatformImageProperties::default()
}

#[cfg(any(unix, windows))]
fn is_heif_format(format: Option<&str>) -> bool {
    matches!(format, Some("HEIC") | Some("HEIF"))
}

#[cfg(any(
    target_os = "windows",
    all(
        unix,
        not(any(target_os = "macos", target_os = "android", target_os = "ios"))
    )
))]
fn parse_dimensions(value: &str) -> Option<(usize, usize)> {
    let (width, height) = value.trim().split_once('x')?;
    let width = width.trim().parse::<usize>().ok().filter(|value| *value > 0)?;
    let height = height.trim().parse::<usize>().ok().filter(|value| *value > 0)?;
    Some((width, height))
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "android", target_os = "ios"))))]
fn parse_heif_info_properties(text: &str) -> Option<super::PlatformImageProperties> {
    let primary = text
        .lines()
        .enumerate()
        .find(|(_, line)| line.trim_start().starts_with("image:") && line.contains("primary"))
        .or_else(|| {
            text.lines()
                .enumerate()
                .find(|(_, line)| line.trim_start().starts_with("image:"))
        })?;
    let value = primary.1.trim().strip_prefix("image:")?.trim();
    let dimensions = parse_dimensions(value.split_whitespace().next()?)?;
    let has_alpha = text
        .lines()
        .skip(primary.0 + 1)
        .take_while(|line| !line.trim_start().starts_with("image:"))
        .find_map(|line| {
            line.trim()
                .strip_prefix("alpha channel:")
                .and_then(parse_bool)
        });
    Some(super::PlatformImageProperties {
        dimensions: Some(dimensions),
        has_alpha,
    })
}

#[cfg(any(
    target_os = "macos",
    all(
        unix,
        not(any(target_os = "macos", target_os = "android", target_os = "ios"))
    )
))]
fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "yes" | "true" | "1" => Some(true),
        "no" | "false" | "0" => Some(false),
        _ => None,
    }
}

#[cfg(target_os = "macos")]
pub(super) fn render_image_to_png(
    source: &Path,
    destination: &Path,
    max_long_edge: Option<u32>,
) -> super::PlatformResult<()> {
    let mut command = Command::new("/usr/bin/sips");
    command.arg("-s").arg("format").arg("png");
    if let Some(max_long_edge) = max_long_edge {
        command
            .arg("--resampleHeightWidthMax")
            .arg(max_long_edge.to_string());
    }
    let output = command
        .arg(source)
        .arg("--out")
        .arg(destination)
        .output()
        .map_err(|error| format!("Could not start the macOS image converter: {error}"))?;
    command_result(output, "macOS image converter")
}

#[cfg(target_os = "windows")]
pub(super) fn render_image_to_png(
    source: &Path,
    destination: &Path,
    max_long_edge: Option<u32>,
) -> super::PlatformResult<()> {
    // Windows PresentationCore decoders are backed by Windows Imaging Component.
    // Consequently HEIF/HEIC works whenever the system HEIF/HEVC codec is installed,
    // while JPEG/PNG/WebP and other WIC formats use the same narrow adapter.
    const SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName PresentationCore
$sourcePath = [Environment]::GetEnvironmentVariable('RUSTNOTE_IMAGE_SOURCE')
$destinationPath = [Environment]::GetEnvironmentVariable('RUSTNOTE_IMAGE_DESTINATION')
$maxText = [Environment]::GetEnvironmentVariable('RUSTNOTE_IMAGE_MAX_EDGE')
$input = [IO.File]::Open($sourcePath, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
try {
  $decoder = [System.Windows.Media.Imaging.BitmapDecoder]::Create(
    $input,
    [System.Windows.Media.Imaging.BitmapCreateOptions]::PreservePixelFormat,
    [System.Windows.Media.Imaging.BitmapCacheOption]::OnLoad)
  if ($decoder.Frames.Count -lt 1) { throw 'The image contains no decodable frame.' }
  [System.Windows.Media.Imaging.BitmapSource]$bitmap = $decoder.Frames[0]
  [int]$orientation = 1
  $metadata = $bitmap.Metadata -as [System.Windows.Media.Imaging.BitmapMetadata]
  if ($null -ne $metadata) {
    try {
      $value = $metadata.GetQuery('/app1/ifd/{ushort=274}')
      if ($null -ne $value) { $orientation = [int]$value }
    } catch { $orientation = 1 }
  }
  $matrix = $null
  switch ($orientation) {
    2 { $matrix = [System.Windows.Media.Matrix]::new(-1, 0, 0, 1, $bitmap.PixelWidth, 0); break }
    3 { $matrix = [System.Windows.Media.Matrix]::new(-1, 0, 0, -1, $bitmap.PixelWidth, $bitmap.PixelHeight); break }
    4 { $matrix = [System.Windows.Media.Matrix]::new(1, 0, 0, -1, 0, $bitmap.PixelHeight); break }
    5 { $matrix = [System.Windows.Media.Matrix]::new(0, 1, 1, 0, 0, 0); break }
    6 { $matrix = [System.Windows.Media.Matrix]::new(0, 1, -1, 0, $bitmap.PixelHeight, 0); break }
    7 { $matrix = [System.Windows.Media.Matrix]::new(0, -1, -1, 0, $bitmap.PixelHeight, $bitmap.PixelWidth); break }
    8 { $matrix = [System.Windows.Media.Matrix]::new(0, -1, 1, 0, 0, $bitmap.PixelWidth); break }
  }
  if ($null -ne $matrix) {
    $orientationTransform = [System.Windows.Media.MatrixTransform]::new($matrix)
    $bitmap = [System.Windows.Media.Imaging.TransformedBitmap]::new($bitmap, $orientationTransform)
  }
  [int]$maxEdge = 0
  if ([int]::TryParse($maxText, [ref]$maxEdge) -and $maxEdge -gt 0) {
    $longEdge = [Math]::Max($bitmap.PixelWidth, $bitmap.PixelHeight)
    if ($longEdge -gt $maxEdge) {
      $scale = $maxEdge / [double]$longEdge
      $transform = [System.Windows.Media.ScaleTransform]::new($scale, $scale)
      $bitmap = [System.Windows.Media.Imaging.TransformedBitmap]::new($bitmap, $transform)
    }
  }
  $encoder = [System.Windows.Media.Imaging.PngBitmapEncoder]::new()
  $encoder.Frames.Add([System.Windows.Media.Imaging.BitmapFrame]::Create($bitmap))
  $output = [IO.File]::Open($destinationPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
  try { $encoder.Save($output); $output.Flush() } finally { $output.Dispose() }
} finally { $input.Dispose() }
"#;

    let mut command = Command::new("powershell.exe");
    command
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", SCRIPT])
        .env("RUSTNOTE_IMAGE_SOURCE", source)
        .env("RUSTNOTE_IMAGE_DESTINATION", destination)
        .env(
            "RUSTNOTE_IMAGE_MAX_EDGE",
            max_long_edge.map(|value| value.to_string()).unwrap_or_default(),
        );
    let output = command
        .output()
        .map_err(|error| format!("Could not start the Windows image converter: {error}"))?;
    command_result(output, "Windows image converter").map_err(|error| {
        if source.extension().and_then(|value| value.to_str()).is_some_and(|value| {
            value.eq_ignore_ascii_case("heic") || value.eq_ignore_ascii_case("heif")
        }) {
            super::PlatformError::operation(format!(
                "{error}. HEIC/HEIF on Windows requires the system HEIF/HEVC codec"
            ))
        } else {
            error
        }
    })
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "android", target_os = "ios"))))]
pub(super) fn render_image_to_png(
    source: &Path,
    destination: &Path,
    max_long_edge: Option<u32>,
) -> super::PlatformResult<()> {
    // Linux intentionally uses host codec tools rather than bundling an HEVC decoder.
    // ImageMagick handles ordinary formats and HEIF when its delegate is installed.
    // If ImageMagick lacks HEIF support, libheif's small `heif-convert` utility can
    // decode first; ImageMagick may then resize the resulting PNG if needed.
    let tools = linux_image_tools();
    let imagemagick = tools.convert;
    let is_heif = source
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| {
            value.eq_ignore_ascii_case("heic") || value.eq_ignore_ascii_case("heif")
        });

    if let Some(executable) = imagemagick {
        match run_imagemagick(executable, source, destination, max_long_edge) {
            Ok(()) => return Ok(()),
            Err(error) if !is_heif => return Err(error),
            Err(_) => {
                let _ = std::fs::remove_file(destination);
            }
        }
    }

    if is_heif && tools.heif_convert {
        if max_long_edge.is_none() {
            return run_heif_convert(source, destination);
        }
        if let Some(executable) = imagemagick {
            let intermediate = destination.with_extension("heif-source.png");
            let result = (|| {
                run_heif_convert(source, &intermediate)?;
                run_imagemagick(executable, &intermediate, destination, max_long_edge)
            })();
            let _ = std::fs::remove_file(&intermediate);
            return result;
        }
    }

    Err(super::PlatformError::operation(if is_heif {
        concat!(
            "HEIC/HEIF rendition requires a host HEIF decoder (ImageMagick with HEIF support ",
            "or heif-convert); screen-sized renditions additionally require ImageMagick"
        )
        .to_string()
    } else {
        "Image rendition requires ImageMagick on this Linux system".to_string()
    }))
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub(super) fn render_image_to_png(
    source: &Path,
    destination: &Path,
    max_long_edge: Option<u32>,
) -> super::PlatformResult<()> {
    let adapter = super::mobile::adapter().ok_or_else(|| {
        "Rustnote's mobile platform adapter has not been registered".to_string()
    })?;
    adapter.render_image_to_png(source, destination, max_long_edge)
}

#[cfg(not(any(unix, windows)))]
pub(super) fn render_image_to_png(
    _source: &Path,
    _destination: &Path,
    _max_long_edge: Option<u32>,
) -> super::PlatformResult<()> {
    Err(super::PlatformError::Unsupported("Image rendition conversion is not supported on this platform"))
}

#[cfg(any(
    target_os = "macos",
    target_os = "windows",
    all(
        unix,
        not(any(target_os = "macos", target_os = "android", target_os = "ios"))
    )
))]
fn command_result(output: Output, label: &str) -> super::PlatformResult<()> {
    if output.status.success() {
        return Ok(());
    }
    let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(super::PlatformError::operation(if detail.is_empty() {
        format!("The {label} exited with status {}", output.status)
    } else {
        format!("The {label} failed: {detail}")
    }))
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "android", target_os = "ios"))))]
#[derive(Clone, Copy, Debug)]
struct LinuxImageTools {
    identify: Option<&'static str>,
    convert: Option<&'static str>,
    heif_convert: bool,
    heif_info: bool,
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "android", target_os = "ios"))))]
fn linux_image_tools() -> LinuxImageTools {
    static TOOLS: OnceLock<LinuxImageTools> = OnceLock::new();
    *TOOLS.get_or_init(|| {
        let magick = executable_on_path("magick");
        LinuxImageTools {
            identify: magick.then_some("magick").or_else(|| {
                executable_on_path("identify").then_some("identify")
            }),
            convert: magick.then_some("magick").or_else(|| {
                executable_on_path("convert").then_some("convert")
            }),
            heif_convert: executable_on_path("heif-convert"),
            heif_info: executable_on_path("heif-info"),
        }
    })
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "android", target_os = "ios"))))]
fn executable_on_path(command: &str) -> bool {
    let Some(path) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&path).any(|directory| {
        let Ok(metadata) = std::fs::metadata(directory.join(command)) else {
            return false;
        };
        metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
    })
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "android", target_os = "ios"))))]
fn run_heif_convert(source: &Path, destination: &Path) -> super::PlatformResult<()> {
    let output = Command::new("heif-convert")
        .arg(source)
        .arg(destination)
        .output()
        .map_err(|error| format!("Could not start the Linux HEIF converter: {error}"))?;
    command_result(output, "Linux HEIF converter")
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "android", target_os = "ios"))))]
fn run_imagemagick(
    executable: &str,
    source: &Path,
    destination: &Path,
    max_long_edge: Option<u32>,
) -> super::PlatformResult<()> {
    let mut command = Command::new(executable);
    command.arg(source).arg("-auto-orient");
    if let Some(max_long_edge) = max_long_edge {
        command.arg("-resize").arg(format!("{max_long_edge}x{max_long_edge}>"));
    }
    let output = command
        .arg(destination)
        .output()
        .map_err(|error| format!("Could not start ImageMagick: {error}"))?;
    command_result(output, "Linux image converter")
}
