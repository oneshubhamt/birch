//! Dependency-free, lossless media compatibility engine for Rustnote.
//!
//! This crate intentionally contains only bounded media parsing and passthrough remuxing.
//! Application storage, cache lifecycle, routing, and native/external playback policy remain in
//! the main Rustnote crate. The planner exposes only lossless target profiles that preserve a
//! sensible presentation: AVC/HEVC/AV1 + AAC/AC-3/E-AC-3 to ISO-BMFF, VP8/VP9/AV1 +
//! Opus/Vorbis to WebM, and audio-only FLAC/MP3/AC-3/E-AC-3 extraction. Unsupported layouts
//! fail closed without decoding or re-encoding.

use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
};

const MAX_TRACKS: usize = 64;
const MAX_CLUSTERS: usize = 1_000_000;
const MAX_SAMPLES_PER_TRACK: usize = 5_000_000;
const MAX_CODEC_PRIVATE_BYTES: u64 = 1024 * 1024;
const MAX_BLOCK_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TEXT_BYTES: u64 = 4096;
const DEFAULT_TIMESTAMP_SCALE_NS: u64 = 1_000_000;
const VIDEO_TIMESCALE: u32 = 90_000;
const MOVIE_TIMESCALE: u32 = 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CompatibilityProfile {
    Mp4Video,
    M4aAudio,
    WebmVideo,
    WebmAudio,
    FlacAudio,
    Mp3Audio,
    Ac3Audio,
    Eac3Audio,
}

impl CompatibilityProfile {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mp4Video => "mp4-video",
            Self::M4aAudio => "m4a-audio",
            Self::WebmVideo => "webm-video",
            Self::WebmAudio => "webm-audio",
            Self::FlacAudio => "flac-audio",
            Self::Mp3Audio => "mp3-audio",
            Self::Ac3Audio => "ac3-audio",
            Self::Eac3Audio => "eac3-audio",
        }
    }

    pub const fn extension(self) -> &'static str {
        match self {
            Self::Mp4Video => "mp4",
            Self::M4aAudio => "m4a",
            Self::WebmVideo | Self::WebmAudio => "webm",
            Self::FlacAudio => "flac",
            Self::Mp3Audio => "mp3",
            Self::Ac3Audio => "ac3",
            Self::Eac3Audio => "eac3",
        }
    }

    pub const fn base_mime_type(self) -> &'static str {
        match self {
            Self::Mp4Video => "video/mp4",
            Self::M4aAudio => "audio/mp4",
            Self::WebmVideo => "video/webm",
            Self::WebmAudio => "audio/webm",
            Self::FlacAudio => "audio/flac",
            Self::Mp3Audio => "audio/mpeg",
            Self::Ac3Audio => "audio/ac3",
            Self::Eac3Audio => "audio/eac3",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "mp4-video" => Some(Self::Mp4Video),
            "m4a-audio" => Some(Self::M4aAudio),
            "webm-video" => Some(Self::WebmVideo),
            "webm-audio" => Some(Self::WebmAudio),
            "flac-audio" => Some(Self::FlacAudio),
            "mp3-audio" => Some(Self::Mp3Audio),
            "ac3-audio" => Some(Self::Ac3Audio),
            "eac3-audio" => Some(Self::Eac3Audio),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompatibilityCandidate {
    pub profile: CompatibilityProfile,
    pub mime_type: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CompatibilityProbe {
    pub has_video: bool,
    pub has_audio: bool,
    pub candidates: Vec<CompatibilityCandidate>,
}

const ID_EBML: u32 = 0x1A45_DFA3;
const ID_DOCTYPE: u32 = 0x4282;
const ID_SEGMENT: u32 = 0x1853_8067;
const ID_INFO: u32 = 0x1549_A966;
const ID_TIMESTAMP_SCALE: u32 = 0x002A_D7B1;
const ID_TRACKS: u32 = 0x1654_AE6B;
const ID_TRACK_ENTRY: u32 = 0xAE;
const ID_TRACK_NUMBER: u32 = 0xD7;
const ID_TRACK_TYPE: u32 = 0x83;
const ID_FLAG_ENABLED: u32 = 0xB9;
const ID_FLAG_DEFAULT: u32 = 0x88;
const ID_DEFAULT_DURATION: u32 = 0x0023_E383;
const ID_CODEC_ID: u32 = 0x86;
const ID_CODEC_PRIVATE: u32 = 0x63A2;
const ID_VIDEO: u32 = 0xE0;
const ID_PIXEL_WIDTH: u32 = 0xB0;
const ID_PIXEL_HEIGHT: u32 = 0xBA;
const ID_AUDIO: u32 = 0xE1;
const ID_SAMPLING_FREQUENCY: u32 = 0xB5;
const ID_CHANNELS: u32 = 0x9F;
const ID_CONTENT_ENCODINGS: u32 = 0x6D80;
const ID_CLUSTER: u32 = 0x1F43_B675;
const ID_CLUSTER_TIMESTAMP: u32 = 0xE7;
const ID_SIMPLE_BLOCK: u32 = 0xA3;
const ID_BLOCK_GROUP: u32 = 0xA0;
const ID_BLOCK: u32 = 0xA1;
const ID_BLOCK_DURATION: u32 = 0x9B;
const ID_REFERENCE_BLOCK: u32 = 0xFB;
const ID_BLOCK_ADDITIONS: u32 = 0x75A1;
const ID_CODEC_STATE: u32 = 0xA4;
const ID_CUES: u32 = 0x1C53_BB6B;
const ID_CUE_POINT: u32 = 0xBB;
const ID_CUE_TIME: u32 = 0xB3;
const ID_CUE_TRACK_POSITIONS: u32 = 0xB7;
const ID_CUE_TRACK: u32 = 0xF7;
const ID_CUE_CLUSTER_POSITION: u32 = 0xF1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TrackKind {
    Video,
    Audio,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Ac3Config {
    fscod: u8,
    bsid: u8,
    bsmod: u8,
    acmod: u8,
    lfeon: bool,
    bit_rate_code: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Eac3Config {
    fscod: u8,
    bsid: u8,
    bsmod: u8,
    acmod: u8,
    lfeon: bool,
    max_data_rate_kbps: u16,
    complexity_index_type_a: Option<u8>,
}

#[derive(Clone, Debug)]
enum CodecConfig {
    Avc {
        private: Vec<u8>,
        nal_length_bytes: usize,
    },
    Hevc {
        private: Vec<u8>,
        nal_length_bytes: usize,
    },
    Av1 {
        private: Vec<u8>,
    },
    Aac {
        private: Vec<u8>,
        frame_samples: u32,
    },
    Ac3 {
        config: Option<Ac3Config>,
    },
    Eac3 {
        config: Option<Eac3Config>,
    },
}

#[derive(Clone, Debug)]
struct SourceTrack {
    number: u64,
    kind: TrackKind,
    enabled: bool,
    default_track: bool,
    default_duration_ns: Option<u64>,
    codec_id: String,
    codec_private: Vec<u8>,
    has_content_encodings: bool,
    width: Option<u32>,
    height: Option<u32>,
    sample_rate: Option<u32>,
    channels: Option<u16>,
    element_start: u64,
    element_end: u64,
}

#[derive(Clone, Copy, Debug)]
struct ElementSpan {
    data_start: u64,
    data_end: u64,
}

#[derive(Debug)]
struct MatroskaLayout {
    timestamp_scale_ns: u64,
    clusters: Vec<ElementSpan>,
    tracks: Vec<SourceTrack>,
}

#[derive(Clone, Debug)]
struct SampleMeta {
    offset: u64,
    size: u32,
    pts_ns: u64,
    explicit_duration_ns: Option<u64>,
    keyframe: bool,
}

#[derive(Clone, Copy, Debug)]
struct TimedSample {
    offset: u64,
    size: u32,
    duration: u32,
    composition_offset: i32,
    keyframe: bool,
}

#[derive(Debug)]
struct OutputTrack {
    track_id: u32,
    source_number: u64,
    kind: TrackKind,
    codec: CodecConfig,
    width: u16,
    height: u16,
    sample_rate: u32,
    channels: u16,
    default_duration_ns: Option<u64>,
    samples: Vec<SampleMeta>,
    timed_samples: Vec<TimedSample>,
    timescale: u32,
    media_duration: u64,
}

impl OutputTrack {
    fn duration_hint_ns(&self) -> Option<u64> {
        if let Some(value) = self.default_duration_ns.filter(|value| *value > 0) {
            return Some(value);
        }
        match self.codec {
            CodecConfig::Aac { frame_samples, .. } if self.sample_rate > 0 => Some(
                u64::from(frame_samples)
                    .saturating_mul(1_000_000_000)
                    .div_ceil(u64::from(self.sample_rate)),
            ),
            CodecConfig::Ac3 { .. } | CodecConfig::Eac3 { .. } if self.sample_rate > 0 => Some(
                1536_u64
                    .saturating_mul(1_000_000_000)
                    .div_ceil(u64::from(self.sample_rate)),
            ),
            _ => None,
        }
    }

    fn validate_frame(&mut self, data: &[u8]) -> io::Result<()> {
        let sample_rate = self.sample_rate;
        let channels = self.channels;
        match &mut self.codec {
            CodecConfig::Avc {
                nal_length_bytes, ..
            }
            | CodecConfig::Hevc {
                nal_length_bytes, ..
            } => validate_length_prefixed_nals(data, *nal_length_bytes),
            CodecConfig::Av1 { .. } => validate_av1_sample(data),
            CodecConfig::Aac { .. } => {
                if data.is_empty() {
                    Err(invalid_data("empty AAC frame in Matroska track"))
                } else {
                    Ok(())
                }
            }
            CodecConfig::Ac3 { config } => {
                let parsed = parse_ac3_frame(data)?;
                validate_dolby_track_metadata(
                    sample_rate,
                    channels,
                    parsed.sample_rate,
                    parsed.channels,
                )?;
                if let Some(existing) = config {
                    if *existing != parsed.config {
                        return Err(invalid_data(
                            "AC-3 stream configuration changes between samples",
                        ));
                    }
                } else {
                    *config = Some(parsed.config);
                }
                Ok(())
            }
            CodecConfig::Eac3 { config } => {
                let parsed = parse_eac3_frame(data)?;
                validate_dolby_track_metadata(
                    sample_rate,
                    channels,
                    parsed.sample_rate,
                    parsed.channels,
                )?;
                if let Some(existing) = config {
                    if existing.fscod != parsed.config.fscod
                        || existing.bsid != parsed.config.bsid
                        || existing.bsmod != parsed.config.bsmod
                        || existing.acmod != parsed.config.acmod
                        || existing.lfeon != parsed.config.lfeon
                        || existing.complexity_index_type_a != parsed.config.complexity_index_type_a
                    {
                        return Err(invalid_data(
                            "E-AC-3 stream configuration changes between samples",
                        ));
                    }
                    existing.max_data_rate_kbps = existing
                        .max_data_rate_kbps
                        .max(parsed.config.max_data_rate_kbps);
                } else {
                    *config = Some(parsed.config);
                }
                Ok(())
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct ElementHeader {
    id: u32,
    data_start: u64,
    data_len: Option<u64>,
    data_end: u64,
}

pub fn probe_matroska(source_path: &Path) -> io::Result<CompatibilityProbe> {
    let mut source = File::open(source_path)?;
    let source_len = source.metadata()?.len();
    if source_len < 16 {
        return Err(invalid_data("Matroska file is too small"));
    }
    let segment = parse_matroska_segment(&mut source, source_len)?;
    let tracks = parse_first_tracks(&mut source, segment)?;
    Ok(build_compatibility_probe(&tracks))
}

pub fn remux_matroska(
    source_path: &Path,
    destination: &Path,
    profile: CompatibilityProfile,
) -> io::Result<()> {
    match profile {
        CompatibilityProfile::Mp4Video | CompatibilityProfile::M4aAudio => {
            remux_matroska_to_iso_bmff(source_path, destination, profile)
        }
        CompatibilityProfile::WebmVideo | CompatibilityProfile::WebmAudio => {
            remux_matroska_to_webm(source_path, destination, profile)
        }
        CompatibilityProfile::FlacAudio
        | CompatibilityProfile::Mp3Audio
        | CompatibilityProfile::Ac3Audio
        | CompatibilityProfile::Eac3Audio => {
            extract_matroska_audio(source_path, destination, profile)
        }
    }
}

fn remux_matroska_to_iso_bmff(
    source_path: &Path,
    destination: &Path,
    profile: CompatibilityProfile,
) -> io::Result<()> {
    let mut source = File::open(source_path)?;
    let source_len = source.metadata()?.len();
    if source_len < 16 {
        return Err(invalid_data("Matroska file is too small"));
    }
    let layout = parse_matroska_layout(&mut source, source_len)?;
    let mut output_tracks = select_output_tracks(&layout.tracks, profile)?;

    let output = OpenOptions::new()
        .write(true)
        .read(true)
        .create_new(true)
        .open(destination)?;
    let mut output = io::BufWriter::with_capacity(1024 * 1024, output);

    write_ftyp(&mut output, &output_tracks)?;
    let mdat_start = output.stream_position()?;
    write_u32(&mut output, 1)?;
    output.write_all(b"mdat")?;
    write_u64(&mut output, 0)?;

    for cluster in &layout.clusters {
        process_cluster(
            &mut source,
            *cluster,
            layout.timestamp_scale_ns,
            &mut output,
            &mut output_tracks,
        )?;
    }

    if output_tracks.iter().any(|track| track.samples.is_empty()) {
        return Err(invalid_data(
            "selected Matroska track contained no media samples",
        ));
    }

    let global_base_ns = output_tracks
        .iter()
        .flat_map(|track| track.samples.iter().map(|sample| sample.pts_ns))
        .min()
        .ok_or_else(|| invalid_data("Matroska file contained no compatible media samples"))?;

    let mut movie_end_ns = global_base_ns;
    for track in &mut output_tracks {
        let fallback_duration = track_duration_fallback_ns(track)?;
        for sample in &track.samples {
            let duration = sample.explicit_duration_ns.unwrap_or(fallback_duration);
            movie_end_ns = movie_end_ns.max(sample.pts_ns.saturating_add(duration));
        }
        finalize_track_timing(track, global_base_ns, fallback_duration)?;
        track.samples.clear();
        track.samples.shrink_to_fit();
    }

    let mdat_end = output.stream_position()?;
    let mdat_size = mdat_end
        .checked_sub(mdat_start)
        .ok_or_else(|| invalid_data("invalid MP4 media-data size"))?;
    output.seek(SeekFrom::Start(mdat_start + 8))?;
    write_u64(&mut output, mdat_size)?;
    output.seek(SeekFrom::Start(mdat_end))?;

    let movie_duration = ns_to_ticks(movie_end_ns.saturating_sub(global_base_ns), MOVIE_TIMESCALE);
    let moov = build_moov(&output_tracks, movie_duration)?;
    output.write_all(&moov)?;
    output.flush()?;
    output.get_ref().sync_all()?;
    Ok(())
}

fn extract_matroska_audio(
    source_path: &Path,
    destination: &Path,
    profile: CompatibilityProfile,
) -> io::Result<()> {
    if !matches!(
        profile,
        CompatibilityProfile::FlacAudio
            | CompatibilityProfile::Mp3Audio
            | CompatibilityProfile::Ac3Audio
            | CompatibilityProfile::Eac3Audio
    ) {
        return Err(invalid_data(
            "requested compatibility profile is not raw audio",
        ));
    }
    let mut source = File::open(source_path)?;
    let source_len = source.metadata()?.len();
    if source_len < 16 {
        return Err(invalid_data("Matroska file is too small"));
    }
    let layout = parse_matroska_layout(&mut source, source_len)?;
    if layout
        .tracks
        .iter()
        .any(|track| track.enabled && track.kind == TrackKind::Video)
    {
        return Err(invalid_data(
            "raw-audio compatibility profile is audio-only",
        ));
    }
    let enabled_audio: Vec<&SourceTrack> = layout
        .tracks
        .iter()
        .filter(|track| track.enabled && track.kind == TrackKind::Audio)
        .collect();
    let track = match profile {
        CompatibilityProfile::FlacAudio => {
            choose_supported_track(&enabled_audio, supported_flac_audio_track)
        }
        CompatibilityProfile::Mp3Audio => {
            choose_supported_track(&enabled_audio, supported_mp3_audio_track)
        }
        CompatibilityProfile::Ac3Audio => {
            choose_supported_track(&enabled_audio, supported_ac3_audio_track)
        }
        CompatibilityProfile::Eac3Audio => {
            choose_supported_track(&enabled_audio, supported_eac3_audio_track)
        }
        _ => None,
    }
    .ok_or_else(|| invalid_data("Matroska file has no compatible raw-audio track"))?;

    let output = OpenOptions::new()
        .write(true)
        .read(true)
        .create_new(true)
        .open(destination)?;
    let mut output = io::BufWriter::with_capacity(1024 * 1024, output);
    if profile == CompatibilityProfile::FlacAudio {
        output.write_all(&track.codec_private)?;
    }

    let mut frame_count = 0_u64;
    for cluster in &layout.clusters {
        frame_count = frame_count.saturating_add(extract_audio_cluster(
            &mut source,
            *cluster,
            track.number,
            profile,
            &mut output,
        )?);
    }
    if frame_count == 0 {
        return Err(invalid_data(
            "selected Matroska audio track contained no frames",
        ));
    }
    output.flush()?;
    output.get_ref().sync_all()?;
    Ok(())
}

fn extract_audio_cluster<W: Write>(
    source: &mut File,
    cluster: ElementSpan,
    track_number: u64,
    profile: CompatibilityProfile,
    output: &mut W,
) -> io::Result<u64> {
    let mut frame_count = 0_u64;
    source.seek(SeekFrom::Start(cluster.data_start))?;
    while source.stream_position()? < cluster.data_end {
        let Some(child) = read_element_header(source, cluster.data_end)? else {
            break;
        };
        require_known_size(child, "Matroska Cluster child")?;
        match child.id {
            ID_SIMPLE_BLOCK => {
                let bytes = read_bytes(source, child, MAX_BLOCK_BYTES)?;
                frame_count = frame_count.saturating_add(write_raw_audio_block(
                    &bytes,
                    true,
                    track_number,
                    profile,
                    output,
                )?);
            }
            ID_BLOCK_GROUP => {
                if let Some(bytes) = raw_audio_block_group(source, child)? {
                    frame_count = frame_count.saturating_add(write_raw_audio_block(
                        &bytes,
                        false,
                        track_number,
                        profile,
                        output,
                    )?);
                }
            }
            _ => {}
        }
        source.seek(SeekFrom::Start(child.data_end))?;
    }
    Ok(frame_count)
}

fn raw_audio_block_group(source: &mut File, group: ElementHeader) -> io::Result<Option<Vec<u8>>> {
    let mut block = None;
    source.seek(SeekFrom::Start(group.data_start))?;
    while source.stream_position()? < group.data_end {
        let Some(child) = read_element_header(source, group.data_end)? else {
            break;
        };
        require_known_size(child, "Matroska BlockGroup child")?;
        match child.id {
            ID_BLOCK => block = Some(read_bytes(source, child, MAX_BLOCK_BYTES)?),
            ID_BLOCK_ADDITIONS | ID_CODEC_STATE => {
                return Err(invalid_data(
                    "Matroska audio block uses supplemental codec data that cannot be extracted safely",
                ));
            }
            _ => {}
        }
        source.seek(SeekFrom::Start(child.data_end))?;
    }
    Ok(block)
}

fn write_raw_audio_block<W: Write>(
    bytes: &[u8],
    simple: bool,
    track_number: u64,
    profile: CompatibilityProfile,
    output: &mut W,
) -> io::Result<u64> {
    let parsed = parse_block(bytes, simple)?;
    if parsed.track_number != track_number {
        return Ok(0);
    }
    let mut written = 0_u64;
    for frame in &parsed.frames {
        let data = &bytes[frame.start..frame.end];
        match profile {
            CompatibilityProfile::FlacAudio => {
                if data.len() < 2 || data[0] != 0xFF || data[1] & 0xFC != 0xF8 {
                    return Err(invalid_data(
                        "Matroska FLAC block contains an invalid FLAC frame",
                    ));
                }
            }
            CompatibilityProfile::Mp3Audio => {
                if data.len() < 4 || data[0] != 0xFF || data[1] & 0xE0 != 0xE0 {
                    return Err(invalid_data(
                        "Matroska MP3 block contains an invalid MPEG audio frame",
                    ));
                }
            }
            CompatibilityProfile::Ac3Audio | CompatibilityProfile::Eac3Audio => {
                if data.len() < 2 || data[0] != 0x0B || data[1] != 0x77 {
                    return Err(invalid_data(
                        "Matroska Dolby audio block contains an invalid syncframe",
                    ));
                }
            }
            _ => return Err(invalid_data("invalid raw-audio compatibility profile")),
        }
        output.write_all(data)?;
        written = written.saturating_add(1);
    }
    Ok(written)
}

#[derive(Clone, Copy, Debug)]
struct WebmCue {
    time: u64,
    track: u64,
    cluster_position: u64,
}

fn remux_matroska_to_webm(
    source_path: &Path,
    destination: &Path,
    profile: CompatibilityProfile,
) -> io::Result<()> {
    let mut source = File::open(source_path)?;
    let source_len = source.metadata()?.len();
    if source_len < 16 {
        return Err(invalid_data("Matroska file is too small"));
    }
    let layout = parse_matroska_layout(&mut source, source_len)?;
    let selected_tracks = select_webm_tracks(&layout.tracks, profile)?;
    let selected_numbers: Vec<u64> = selected_tracks.iter().map(|track| track.number).collect();
    let video_track = selected_tracks
        .iter()
        .find(|track| track.kind == TrackKind::Video)
        .map(|track| track.number);

    let output = OpenOptions::new()
        .write(true)
        .read(true)
        .create_new(true)
        .open(destination)?;
    let mut output = io::BufWriter::with_capacity(1024 * 1024, output);

    write_webm_ebml_header(&mut output)?;
    write_ebml_id(&mut output, ID_SEGMENT)?;
    write_ebml_unknown_size(&mut output)?;
    let segment_data_start = output.stream_position()?;

    let info = ebml_master(
        ID_INFO,
        &ebml_uint_element(ID_TIMESTAMP_SCALE, layout.timestamp_scale_ns)?,
    )?;
    output.write_all(&info)?;

    let tracks_size = selected_tracks.iter().try_fold(0_u64, |total, track| {
        let len = track
            .element_end
            .checked_sub(track.element_start)
            .ok_or_else(|| invalid_data("invalid Matroska TrackEntry span"))?;
        total
            .checked_add(len)
            .ok_or_else(|| invalid_data("WebM Tracks size overflow"))
    })?;
    write_ebml_id(&mut output, ID_TRACKS)?;
    write_ebml_size(&mut output, tracks_size)?;
    for track in &selected_tracks {
        copy_file_range(
            &mut source,
            &mut output,
            track.element_start,
            track.element_end,
        )?;
    }

    let mut cues = Vec::new();
    for cluster in &layout.clusters {
        let selected = inspect_webm_cluster(&mut source, *cluster, &selected_numbers, video_track)?;
        if selected.elements.is_empty() {
            continue;
        }
        let timestamp_element = ebml_uint_element(ID_CLUSTER_TIMESTAMP, selected.timestamp)?;
        let cluster_payload_size = selected.elements.iter().try_fold(
            u64::try_from(timestamp_element.len())
                .map_err(|_| invalid_data("WebM timestamp element is too large"))?,
            |total, element| {
                let len = element
                    .end
                    .checked_sub(element.start)
                    .ok_or_else(|| invalid_data("invalid WebM cluster element span"))?;
                total
                    .checked_add(len)
                    .ok_or_else(|| invalid_data("WebM Cluster size overflow"))
            },
        )?;
        let cluster_position = output
            .stream_position()?
            .checked_sub(segment_data_start)
            .ok_or_else(|| invalid_data("invalid WebM cluster position"))?;
        write_ebml_id(&mut output, ID_CLUSTER)?;
        write_ebml_size(&mut output, cluster_payload_size)?;
        output.write_all(&timestamp_element)?;
        for element in &selected.elements {
            copy_file_range(&mut source, &mut output, element.start, element.end)?;
        }
        if selected.has_video_keyframe
            && let Some(track) = video_track
        {
            cues.push(WebmCue {
                time: selected.timestamp,
                track,
                cluster_position,
            });
        }
    }

    if cues.is_empty() && video_track.is_some() {
        return Err(invalid_data(
            "WebM video contains no usable keyframe clusters",
        ));
    }
    if !cues.is_empty() {
        let cues_box = build_webm_cues(&cues)?;
        output.write_all(&cues_box)?;
    }
    output.flush()?;
    output.get_ref().sync_all()?;
    Ok(())
}

fn select_webm_tracks(
    source_tracks: &[SourceTrack],
    profile: CompatibilityProfile,
) -> io::Result<Vec<&SourceTrack>> {
    if !matches!(
        profile,
        CompatibilityProfile::WebmVideo | CompatibilityProfile::WebmAudio
    ) {
        return Err(invalid_data("requested compatibility profile is not WebM"));
    }
    let enabled_video: Vec<&SourceTrack> = source_tracks
        .iter()
        .filter(|track| track.enabled && track.kind == TrackKind::Video)
        .collect();
    let enabled_audio: Vec<&SourceTrack> = source_tracks
        .iter()
        .filter(|track| track.enabled && track.kind == TrackKind::Audio)
        .collect();
    let video = choose_primary_video_track(&enabled_video, supported_webm_video_track);
    let audio = choose_supported_track(&enabled_audio, supported_webm_audio_track);

    match profile {
        CompatibilityProfile::WebmVideo => {
            if video.is_none() {
                return Err(invalid_data(
                    "Matroska video codec cannot be losslessly remuxed to WebM by Rustnote",
                ));
            }
            if !enabled_audio.is_empty() && audio.is_none() {
                return Err(invalid_data(
                    "Matroska video has no WebM-compatible audio track; refusing silent compatibility output",
                ));
            }
        }
        CompatibilityProfile::WebmAudio => {
            if !enabled_video.is_empty() {
                return Err(invalid_data(
                    "audio WebM compatibility profile is audio-only",
                ));
            }
            if audio.is_none() {
                return Err(invalid_data(
                    "Matroska audio codec cannot be losslessly remuxed to WebM by Rustnote",
                ));
            }
        }
        CompatibilityProfile::Mp4Video
        | CompatibilityProfile::M4aAudio
        | CompatibilityProfile::FlacAudio
        | CompatibilityProfile::Mp3Audio
        | CompatibilityProfile::Ac3Audio
        | CompatibilityProfile::Eac3Audio => {
            return Err(invalid_data("requested compatibility profile is not WebM"));
        }
    }

    let mut selected = Vec::with_capacity(2);
    if let Some(track) = video.filter(|_| profile == CompatibilityProfile::WebmVideo) {
        selected.push(track);
    }
    if let Some(track) = audio {
        selected.push(track);
    }
    Ok(selected)
}

#[derive(Clone, Copy, Debug)]
struct RawElementSpan {
    start: u64,
    end: u64,
}

#[derive(Debug)]
struct WebmClusterSelection {
    timestamp: u64,
    elements: Vec<RawElementSpan>,
    has_video_keyframe: bool,
}

fn inspect_webm_cluster(
    source: &mut File,
    cluster: ElementSpan,
    selected_tracks: &[u64],
    video_track: Option<u64>,
) -> io::Result<WebmClusterSelection> {
    let mut timestamp = 0_u64;
    let mut elements = Vec::new();
    let mut has_video_keyframe = false;
    source.seek(SeekFrom::Start(cluster.data_start))?;
    while source.stream_position()? < cluster.data_end {
        let element_start = source.stream_position()?;
        let Some(child) = read_element_header(source, cluster.data_end)? else {
            break;
        };
        require_known_size(child, "Matroska Cluster child")?;
        match child.id {
            ID_CLUSTER_TIMESTAMP => timestamp = read_uint(source, child)?,
            ID_SIMPLE_BLOCK => {
                let route = inspect_block_route(source, child, true)?;
                if selected_tracks.contains(&route.track_number) {
                    elements.push(RawElementSpan {
                        start: element_start,
                        end: child.data_end,
                    });
                    if video_track == Some(route.track_number) && route.keyframe {
                        has_video_keyframe = true;
                    }
                }
            }
            ID_BLOCK_GROUP => {
                if let Some(route) = inspect_block_group_route(source, child)?
                    && selected_tracks.contains(&route.track_number)
                {
                    elements.push(RawElementSpan {
                        start: element_start,
                        end: child.data_end,
                    });
                    if video_track == Some(route.track_number) && route.keyframe {
                        has_video_keyframe = true;
                    }
                }
            }
            _ => {}
        }
        source.seek(SeekFrom::Start(child.data_end))?;
    }
    Ok(WebmClusterSelection {
        timestamp,
        elements,
        has_video_keyframe,
    })
}

#[derive(Clone, Copy, Debug)]
struct BlockRoute {
    track_number: u64,
    keyframe: bool,
}

fn inspect_block_route(
    source: &mut File,
    header: ElementHeader,
    simple: bool,
) -> io::Result<BlockRoute> {
    let len = require_known_size(header, "Matroska block")?;
    if len < 4 {
        return Err(invalid_data("Matroska block is too short"));
    }
    let read_len = usize::try_from(len.min(16))
        .map_err(|_| invalid_data("Matroska block header is too large"))?;
    source.seek(SeekFrom::Start(header.data_start))?;
    let mut bytes = vec![0_u8; read_len];
    source.read_exact(&mut bytes)?;
    let (track_number, track_bytes) = parse_vint_from_slice(&bytes, false)?;
    if track_number == 0 || track_bytes + 3 > bytes.len() {
        return Err(invalid_data("invalid Matroska block header"));
    }
    let flags = bytes[track_bytes + 2];
    Ok(BlockRoute {
        track_number,
        keyframe: simple && flags & 0x80 != 0,
    })
}

fn inspect_block_group_route(
    source: &mut File,
    group: ElementHeader,
) -> io::Result<Option<BlockRoute>> {
    let mut block = None;
    let mut has_reference = false;
    source.seek(SeekFrom::Start(group.data_start))?;
    while source.stream_position()? < group.data_end {
        let Some(child) = read_element_header(source, group.data_end)? else {
            break;
        };
        require_known_size(child, "Matroska BlockGroup child")?;
        match child.id {
            ID_BLOCK => block = Some(inspect_block_route(source, child, false)?),
            ID_REFERENCE_BLOCK => has_reference = true,
            ID_BLOCK_ADDITIONS | ID_CODEC_STATE => {
                return Err(invalid_data(
                    "Matroska block uses supplemental codec data that cannot be copied safely to WebM",
                ));
            }
            _ => {}
        }
        source.seek(SeekFrom::Start(child.data_end))?;
    }
    Ok(block.map(|route| BlockRoute {
        track_number: route.track_number,
        keyframe: !has_reference,
    }))
}

fn write_webm_ebml_header<W: Write>(writer: &mut W) -> io::Result<()> {
    let mut body = Vec::new();
    body.extend_from_slice(&ebml_uint_element(0x4286, 1)?); // EBMLVersion
    body.extend_from_slice(&ebml_uint_element(0x42F7, 1)?); // EBMLReadVersion
    body.extend_from_slice(&ebml_uint_element(0x42F2, 4)?); // EBMLMaxIDLength
    body.extend_from_slice(&ebml_uint_element(0x42F3, 8)?); // EBMLMaxSizeLength
    body.extend_from_slice(&ebml_string_element(ID_DOCTYPE, "webm")?);
    body.extend_from_slice(&ebml_uint_element(0x4287, 4)?); // DocTypeVersion
    body.extend_from_slice(&ebml_uint_element(0x4285, 2)?); // DocTypeReadVersion
    let header = ebml_master(ID_EBML, &body)?;
    writer.write_all(&header)
}

fn build_webm_cues(cues: &[WebmCue]) -> io::Result<Vec<u8>> {
    let mut body = Vec::new();
    for cue in cues {
        let mut positions = Vec::new();
        positions.extend_from_slice(&ebml_uint_element(ID_CUE_TRACK, cue.track)?);
        positions.extend_from_slice(&ebml_uint_element(
            ID_CUE_CLUSTER_POSITION,
            cue.cluster_position,
        )?);
        let positions = ebml_master(ID_CUE_TRACK_POSITIONS, &positions)?;
        let mut point = Vec::new();
        point.extend_from_slice(&ebml_uint_element(ID_CUE_TIME, cue.time)?);
        point.extend_from_slice(&positions);
        body.extend_from_slice(&ebml_master(ID_CUE_POINT, &point)?);
    }
    ebml_master(ID_CUES, &body)
}

fn ebml_master(id: u32, body: &[u8]) -> io::Result<Vec<u8>> {
    let mut output = Vec::with_capacity(body.len().saturating_add(12));
    write_ebml_id(&mut output, id)?;
    write_ebml_size(
        &mut output,
        u64::try_from(body.len()).map_err(|_| invalid_data("EBML master is too large"))?,
    )?;
    output.extend_from_slice(body);
    Ok(output)
}

fn ebml_uint_element(id: u32, value: u64) -> io::Result<Vec<u8>> {
    let bytes = value.to_be_bytes();
    let first = bytes
        .iter()
        .position(|byte| *byte != 0)
        .unwrap_or(bytes.len() - 1);
    let payload = &bytes[first..];
    let mut output = Vec::with_capacity(payload.len().saturating_add(12));
    write_ebml_id(&mut output, id)?;
    write_ebml_size(
        &mut output,
        u64::try_from(payload.len()).map_err(|_| invalid_data("EBML integer is too large"))?,
    )?;
    output.extend_from_slice(payload);
    Ok(output)
}

fn ebml_string_element(id: u32, value: &str) -> io::Result<Vec<u8>> {
    let mut output = Vec::with_capacity(value.len().saturating_add(12));
    write_ebml_id(&mut output, id)?;
    write_ebml_size(
        &mut output,
        u64::try_from(value.len()).map_err(|_| invalid_data("EBML string is too large"))?,
    )?;
    output.extend_from_slice(value.as_bytes());
    Ok(output)
}

fn write_ebml_id<W: Write>(writer: &mut W, id: u32) -> io::Result<()> {
    let bytes = id.to_be_bytes();
    let first = bytes
        .iter()
        .position(|byte| *byte != 0)
        .ok_or_else(|| invalid_data("EBML element ID cannot be zero"))?;
    let encoded = &bytes[first..];
    if encoded.len() > 4 || encoded[0].leading_zeros() as usize + 1 != encoded.len() {
        return Err(invalid_data("invalid EBML element ID"));
    }
    writer.write_all(encoded)
}

fn write_ebml_size<W: Write>(writer: &mut W, value: u64) -> io::Result<()> {
    for len in 1..=8 {
        let bits = 7 * len;
        let max_value = (1_u128 << bits) - 2;
        if u128::from(value) <= max_value {
            let marker = 1_u64 << bits;
            let encoded = marker | value;
            let bytes = encoded.to_be_bytes();
            writer.write_all(&bytes[8 - len..])?;
            return Ok(());
        }
    }
    Err(invalid_data("EBML element size exceeds 56-bit VINT limit"))
}

fn write_ebml_unknown_size<W: Write>(writer: &mut W) -> io::Result<()> {
    writer.write_all(&[0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF])
}

fn copy_file_range<W: Write>(
    source: &mut File,
    output: &mut W,
    start: u64,
    end: u64,
) -> io::Result<()> {
    let mut remaining = end
        .checked_sub(start)
        .ok_or_else(|| invalid_data("invalid media copy span"))?;
    source.seek(SeekFrom::Start(start))?;
    let mut buffer = [0_u8; 256 * 1024];
    while remaining > 0 {
        let wanted = usize::try_from(remaining.min(buffer.len() as u64))
            .map_err(|_| invalid_data("media copy span exceeds platform limits"))?;
        source.read_exact(&mut buffer[..wanted])?;
        output.write_all(&buffer[..wanted])?;
        remaining -= wanted as u64;
    }
    Ok(())
}

pub fn validate_webm(path: &Path) -> io::Result<bool> {
    let mut source = match File::open(path) {
        Ok(source) => source,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    let len = source.metadata()?.len();
    if len < 32 {
        return Ok(false);
    }
    let ebml = match read_element_header(&mut source, len) {
        Ok(Some(header)) => header,
        Ok(None) | Err(_) => return Ok(false),
    };
    if ebml.id != ID_EBML || ebml.data_len.is_none() {
        return Ok(false);
    }
    let doctype = match parse_ebml_doctype(&mut source, ebml) {
        Ok(value) => value,
        Err(_) => return Ok(false),
    };
    if !doctype.eq_ignore_ascii_case("webm") {
        return Ok(false);
    }
    source.seek(SeekFrom::Start(ebml.data_end))?;
    let segment = loop {
        if source.stream_position()? >= len {
            return Ok(false);
        }
        let header = match read_element_header(&mut source, len) {
            Ok(Some(header)) => header,
            Ok(None) | Err(_) => return Ok(false),
        };
        if header.id == ID_SEGMENT {
            break header;
        }
        if header.data_len.is_none() {
            return Ok(false);
        }
        source.seek(SeekFrom::Start(header.data_end))?;
    };

    let mut saw_info = false;
    let mut saw_tracks = false;
    let mut saw_cluster = false;
    let mut elements = 0_usize;
    source.seek(SeekFrom::Start(segment.data_start))?;
    while source.stream_position()? < segment.data_end && elements < 1_000_000 {
        let header = match read_element_header(&mut source, segment.data_end) {
            Ok(Some(header)) => header,
            Ok(None) | Err(_) => return Ok(false),
        };
        if header.data_len.is_none() {
            return Ok(false);
        }
        match header.id {
            ID_INFO => saw_info = true,
            ID_TRACKS => saw_tracks = true,
            ID_CLUSTER => saw_cluster = true,
            _ => {}
        }
        if saw_info && saw_tracks && saw_cluster {
            return Ok(true);
        }
        source.seek(SeekFrom::Start(header.data_end))?;
        elements += 1;
    }
    Ok(false)
}

fn parse_matroska_segment(source: &mut File, source_len: u64) -> io::Result<ElementHeader> {
    source.seek(SeekFrom::Start(0))?;
    let ebml = read_element_header(source, source_len)?
        .ok_or_else(|| invalid_data("missing EBML header"))?;
    if ebml.id != ID_EBML || ebml.data_len.is_none() {
        return Err(invalid_data("not a bounded Matroska EBML header"));
    }
    let doctype = parse_ebml_doctype(source, ebml)?;
    if !doctype.eq_ignore_ascii_case("matroska") && !doctype.eq_ignore_ascii_case("webm") {
        return Err(invalid_data("unsupported EBML document type"));
    }

    source.seek(SeekFrom::Start(ebml.data_end))?;
    while source.stream_position()? < source_len {
        let Some(header) = read_element_header(source, source_len)? else {
            break;
        };
        if header.id == ID_SEGMENT {
            return Ok(header);
        }
        if header.data_len.is_none() {
            return Err(invalid_data(
                "unknown-sized element before Matroska Segment",
            ));
        }
        source.seek(SeekFrom::Start(header.data_end))?;
    }
    Err(invalid_data("missing Matroska Segment"))
}

fn parse_first_tracks(source: &mut File, segment: ElementHeader) -> io::Result<Vec<SourceTrack>> {
    source.seek(SeekFrom::Start(segment.data_start))?;
    while source.stream_position()? < segment.data_end {
        let Some(header) = read_element_header(source, segment.data_end)? else {
            break;
        };
        if header.id == ID_TRACKS {
            require_known_size(header, "Matroska Tracks")?;
            let tracks = parse_tracks(source, header)?;
            if tracks.is_empty() {
                return Err(invalid_data("Matroska file has no tracks"));
            }
            return Ok(tracks);
        }
        if header.data_len.is_none() {
            return Err(invalid_data("unknown-sized Matroska element before Tracks"));
        }
        source.seek(SeekFrom::Start(header.data_end))?;
    }
    Err(invalid_data("Matroska file has no tracks"))
}

fn parse_matroska_layout(source: &mut File, source_len: u64) -> io::Result<MatroskaLayout> {
    let segment = parse_matroska_segment(source, source_len)?;
    let mut timestamp_scale_ns = DEFAULT_TIMESTAMP_SCALE_NS;
    let mut tracks = Vec::new();
    let mut clusters = Vec::new();
    source.seek(SeekFrom::Start(segment.data_start))?;
    while source.stream_position()? < segment.data_end {
        let Some(header) = read_element_header(source, segment.data_end)? else {
            break;
        };
        match header.id {
            ID_INFO => {
                require_known_size(header, "Matroska Info")?;
                timestamp_scale_ns =
                    parse_info(source, header)?.unwrap_or(DEFAULT_TIMESTAMP_SCALE_NS);
            }
            ID_TRACKS => {
                require_known_size(header, "Matroska Tracks")?;
                tracks = parse_tracks(source, header)?;
            }
            ID_CLUSTER => {
                require_known_size(header, "Matroska Cluster")?;
                if clusters.len() >= MAX_CLUSTERS {
                    return Err(invalid_data("Matroska file has too many clusters"));
                }
                clusters.push(ElementSpan {
                    data_start: header.data_start,
                    data_end: header.data_end,
                });
            }
            _ => {
                if header.data_len.is_none() {
                    return Err(invalid_data("unsupported unknown-sized Matroska element"));
                }
            }
        }
        source.seek(SeekFrom::Start(header.data_end))?;
    }

    if tracks.is_empty() {
        return Err(invalid_data("Matroska file has no tracks"));
    }
    if clusters.is_empty() {
        return Err(invalid_data("Matroska file has no clusters"));
    }
    if timestamp_scale_ns == 0 || timestamp_scale_ns > 1_000_000_000 {
        return Err(invalid_data("invalid Matroska TimestampScale"));
    }

    Ok(MatroskaLayout {
        timestamp_scale_ns,
        clusters,
        tracks,
    })
}

fn parse_ebml_doctype(source: &mut File, header: ElementHeader) -> io::Result<String> {
    let mut doctype = None;
    source.seek(SeekFrom::Start(header.data_start))?;
    while source.stream_position()? < header.data_end {
        let Some(child) = read_element_header(source, header.data_end)? else {
            break;
        };
        require_known_size(child, "EBML header child")?;
        if child.id == ID_DOCTYPE {
            doctype = Some(read_text(source, child, MAX_TEXT_BYTES)?);
        }
        source.seek(SeekFrom::Start(child.data_end))?;
    }
    doctype.ok_or_else(|| invalid_data("EBML header has no DocType"))
}

fn parse_info(source: &mut File, header: ElementHeader) -> io::Result<Option<u64>> {
    let mut timestamp_scale = None;
    source.seek(SeekFrom::Start(header.data_start))?;
    while source.stream_position()? < header.data_end {
        let Some(child) = read_element_header(source, header.data_end)? else {
            break;
        };
        require_known_size(child, "Matroska Info child")?;
        if child.id == ID_TIMESTAMP_SCALE {
            timestamp_scale = Some(read_uint(source, child)?);
        }
        source.seek(SeekFrom::Start(child.data_end))?;
    }
    Ok(timestamp_scale)
}

fn parse_tracks(source: &mut File, header: ElementHeader) -> io::Result<Vec<SourceTrack>> {
    let mut tracks = Vec::new();
    source.seek(SeekFrom::Start(header.data_start))?;
    while source.stream_position()? < header.data_end {
        let child_start = source.stream_position()?;
        let Some(child) = read_element_header(source, header.data_end)? else {
            break;
        };
        require_known_size(child, "Matroska Tracks child")?;
        if child.id == ID_TRACK_ENTRY {
            if tracks.len() >= MAX_TRACKS {
                return Err(invalid_data("Matroska file has too many tracks"));
            }
            if let Some(track) = parse_track_entry(source, child_start, child)? {
                tracks.push(track);
            }
        }
        source.seek(SeekFrom::Start(child.data_end))?;
    }
    Ok(tracks)
}

fn parse_track_entry(
    source: &mut File,
    element_start: u64,
    header: ElementHeader,
) -> io::Result<Option<SourceTrack>> {
    let mut number = None;
    let mut track_type = None;
    let mut enabled = true;
    let mut default_track = true;
    let mut default_duration_ns = None;
    let mut codec_id = None;
    let mut codec_private = Vec::new();
    let mut has_content_encodings = false;
    let mut width = None;
    let mut height = None;
    let mut sample_rate = None;
    let mut channels = None;

    source.seek(SeekFrom::Start(header.data_start))?;
    while source.stream_position()? < header.data_end {
        let Some(child) = read_element_header(source, header.data_end)? else {
            break;
        };
        require_known_size(child, "Matroska TrackEntry child")?;
        match child.id {
            ID_TRACK_NUMBER => number = Some(read_uint(source, child)?),
            ID_TRACK_TYPE => track_type = Some(read_uint(source, child)?),
            ID_FLAG_ENABLED => enabled = read_uint(source, child)? != 0,
            ID_FLAG_DEFAULT => default_track = read_uint(source, child)? != 0,
            ID_DEFAULT_DURATION => default_duration_ns = Some(read_uint(source, child)?),
            ID_CODEC_ID => codec_id = Some(read_text(source, child, MAX_TEXT_BYTES)?),
            ID_CODEC_PRIVATE => {
                codec_private = read_bytes(source, child, MAX_CODEC_PRIVATE_BYTES)?;
            }
            ID_CONTENT_ENCODINGS => has_content_encodings = true,
            ID_VIDEO => {
                let dimensions = parse_video_settings(source, child)?;
                width = dimensions.0;
                height = dimensions.1;
            }
            ID_AUDIO => {
                let settings = parse_audio_settings(source, child)?;
                sample_rate = settings.0;
                channels = settings.1;
            }
            _ => {}
        }
        source.seek(SeekFrom::Start(child.data_end))?;
    }

    let Some(number) = number else {
        return Ok(None);
    };
    if number == 0 {
        return Err(invalid_data("Matroska TrackNumber must be non-zero"));
    }
    let kind = match track_type {
        Some(1) => TrackKind::Video,
        Some(2) => TrackKind::Audio,
        _ => return Ok(None),
    };
    let codec_id = codec_id.ok_or_else(|| invalid_data("media track is missing CodecID"))?;

    Ok(Some(SourceTrack {
        number,
        kind,
        enabled,
        default_track,
        default_duration_ns: default_duration_ns.filter(|value| *value > 0),
        codec_id,
        codec_private,
        has_content_encodings,
        width,
        height,
        sample_rate,
        channels,
        element_start,
        element_end: header.data_end,
    }))
}

fn parse_video_settings(
    source: &mut File,
    header: ElementHeader,
) -> io::Result<(Option<u32>, Option<u32>)> {
    let mut width = None;
    let mut height = None;
    source.seek(SeekFrom::Start(header.data_start))?;
    while source.stream_position()? < header.data_end {
        let Some(child) = read_element_header(source, header.data_end)? else {
            break;
        };
        require_known_size(child, "Matroska Video child")?;
        match child.id {
            ID_PIXEL_WIDTH => {
                width = Some(
                    u32::try_from(read_uint(source, child)?)
                        .map_err(|_| invalid_data("video width is too large"))?,
                )
            }
            ID_PIXEL_HEIGHT => {
                height = Some(
                    u32::try_from(read_uint(source, child)?)
                        .map_err(|_| invalid_data("video height is too large"))?,
                )
            }
            _ => {}
        }
        source.seek(SeekFrom::Start(child.data_end))?;
    }
    Ok((width, height))
}

fn parse_audio_settings(
    source: &mut File,
    header: ElementHeader,
) -> io::Result<(Option<u32>, Option<u16>)> {
    let mut sample_rate = None;
    let mut channels = None;
    source.seek(SeekFrom::Start(header.data_start))?;
    while source.stream_position()? < header.data_end {
        let Some(child) = read_element_header(source, header.data_end)? else {
            break;
        };
        require_known_size(child, "Matroska Audio child")?;
        match child.id {
            ID_SAMPLING_FREQUENCY => {
                let frequency = read_float(source, child)?;
                if frequency.is_finite() && (1.0..=384_000.0).contains(&frequency) {
                    sample_rate = Some(frequency.round() as u32);
                }
            }
            ID_CHANNELS => {
                channels = Some(
                    u16::try_from(read_uint(source, child)?)
                        .map_err(|_| invalid_data("audio channel count is too large"))?,
                );
            }
            _ => {}
        }
        source.seek(SeekFrom::Start(child.data_end))?;
    }
    Ok((sample_rate, channels))
}

fn build_compatibility_probe(source_tracks: &[SourceTrack]) -> CompatibilityProbe {
    let enabled_video: Vec<&SourceTrack> = source_tracks
        .iter()
        .filter(|track| track.enabled && track.kind == TrackKind::Video)
        .collect();
    let enabled_audio: Vec<&SourceTrack> = source_tracks
        .iter()
        .filter(|track| track.enabled && track.kind == TrackKind::Audio)
        .collect();
    let has_video = !enabled_video.is_empty();
    let has_audio = !enabled_audio.is_empty();
    let mut candidates = Vec::with_capacity(8);

    let mp4_video = choose_primary_video_track(&enabled_video, supported_mp4_video_track);
    let mp4_audio = choose_mp4_audio_track(&enabled_audio);
    if let Some(video) = mp4_video
        && (!has_audio || mp4_audio.is_some())
    {
        candidates.push(CompatibilityCandidate {
            profile: CompatibilityProfile::Mp4Video,
            mime_type: iso_bmff_mime_type(video, mp4_audio),
        });
    } else if !has_video && let Some(audio) = mp4_audio {
        candidates.push(CompatibilityCandidate {
            profile: CompatibilityProfile::M4aAudio,
            mime_type: iso_bmff_mime_type(audio, None),
        });
    }

    let webm_video = choose_primary_video_track(&enabled_video, supported_webm_video_track);
    let webm_audio = choose_supported_track(&enabled_audio, supported_webm_audio_track);
    if let Some(video) = webm_video
        && (!has_audio || webm_audio.is_some())
    {
        candidates.push(CompatibilityCandidate {
            profile: CompatibilityProfile::WebmVideo,
            mime_type: webm_mime_type(Some(video), webm_audio),
        });
    } else if !has_video && let Some(audio) = webm_audio {
        candidates.push(CompatibilityCandidate {
            profile: CompatibilityProfile::WebmAudio,
            mime_type: webm_mime_type(None, Some(audio)),
        });
    }

    if !has_video {
        if choose_supported_track(&enabled_audio, supported_flac_audio_track).is_some() {
            candidates.push(CompatibilityCandidate {
                profile: CompatibilityProfile::FlacAudio,
                mime_type: "audio/flac".to_string(),
            });
        }
        if choose_supported_track(&enabled_audio, supported_mp3_audio_track).is_some() {
            candidates.push(CompatibilityCandidate {
                profile: CompatibilityProfile::Mp3Audio,
                mime_type: "audio/mpeg".to_string(),
            });
        }
        if choose_supported_track(&enabled_audio, supported_ac3_audio_track).is_some() {
            candidates.push(CompatibilityCandidate {
                profile: CompatibilityProfile::Ac3Audio,
                mime_type: "audio/ac3".to_string(),
            });
        }
        if choose_supported_track(&enabled_audio, supported_eac3_audio_track).is_some() {
            candidates.push(CompatibilityCandidate {
                profile: CompatibilityProfile::Eac3Audio,
                mime_type: "audio/eac3".to_string(),
            });
        }
    }

    CompatibilityProbe {
        has_video,
        has_audio,
        candidates,
    }
}

fn select_output_tracks(
    source_tracks: &[SourceTrack],
    profile: CompatibilityProfile,
) -> io::Result<Vec<OutputTrack>> {
    if !matches!(
        profile,
        CompatibilityProfile::Mp4Video | CompatibilityProfile::M4aAudio
    ) {
        return Err(invalid_data(
            "requested compatibility profile is not ISO-BMFF",
        ));
    }
    let enabled_video: Vec<&SourceTrack> = source_tracks
        .iter()
        .filter(|track| track.enabled && track.kind == TrackKind::Video)
        .collect();
    let enabled_audio: Vec<&SourceTrack> = source_tracks
        .iter()
        .filter(|track| track.enabled && track.kind == TrackKind::Audio)
        .collect();

    let video = choose_primary_video_track(&enabled_video, supported_mp4_video_track);
    let audio = choose_mp4_audio_track(&enabled_audio);

    match profile {
        CompatibilityProfile::Mp4Video => {
            if video.is_none() {
                return Err(invalid_data(
                    "Matroska video codec cannot be losslessly remuxed to MP4 by Rustnote",
                ));
            }
            if !enabled_audio.is_empty() && audio.is_none() {
                return Err(invalid_data(
                    "Matroska video has no MP4-compatible audio track; refusing silent compatibility output",
                ));
            }
        }
        CompatibilityProfile::M4aAudio => {
            if !enabled_video.is_empty() {
                return Err(invalid_data("M4A compatibility profile is audio-only"));
            }
            if audio.is_none() {
                return Err(invalid_data(
                    "Matroska audio codec cannot be losslessly remuxed to M4A by Rustnote",
                ));
            }
        }
        CompatibilityProfile::WebmVideo
        | CompatibilityProfile::WebmAudio
        | CompatibilityProfile::FlacAudio
        | CompatibilityProfile::Mp3Audio
        | CompatibilityProfile::Ac3Audio
        | CompatibilityProfile::Eac3Audio => {
            return Err(invalid_data(
                "requested compatibility profile is not ISO-BMFF",
            ));
        }
    }

    let mut result = Vec::with_capacity(2);
    if let Some(track) = video.filter(|_| profile == CompatibilityProfile::Mp4Video) {
        result.push(build_video_output_track(1, track)?);
    }
    if let Some(track) = audio {
        let track_id = if result.is_empty() { 1 } else { 2 };
        result.push(build_audio_output_track(track_id, track)?);
    }
    Ok(result)
}

fn supported_mp4_video_track(track: &SourceTrack) -> bool {
    !track.has_content_encodings
        && matches!(
            track.codec_id.as_str(),
            "V_MPEG4/ISO/AVC" | "V_MPEGH/ISO/HEVC" | "V_AV1"
        )
        && !track.codec_private.is_empty()
        && track.width.is_some()
        && track.height.is_some()
}

fn choose_mp4_audio_track<'a>(tracks: &[&'a SourceTrack]) -> Option<&'a SourceTrack> {
    tracks
        .iter()
        .copied()
        .find(|track| {
            track.default_track && track.codec_id == "A_AAC" && supported_mp4_audio_track(track)
        })
        .or_else(|| {
            tracks
                .iter()
                .copied()
                .find(|track| track.codec_id == "A_AAC" && supported_mp4_audio_track(track))
        })
        .or_else(|| {
            tracks
                .iter()
                .copied()
                .find(|track| track.default_track && supported_mp4_audio_track(track))
        })
        .or_else(|| {
            tracks
                .iter()
                .copied()
                .find(|track| supported_mp4_audio_track(track))
        })
}

fn supported_mp4_audio_track(track: &SourceTrack) -> bool {
    if track.has_content_encodings || track.sample_rate.is_none() || track.channels.is_none() {
        return false;
    }
    match track.codec_id.as_str() {
        "A_AAC" => !track.codec_private.is_empty(),
        "A_AC3" | "A_EAC3" => true,
        _ => false,
    }
}

fn supported_webm_video_track(track: &SourceTrack) -> bool {
    !track.has_content_encodings
        && matches!(track.codec_id.as_str(), "V_VP8" | "V_VP9" | "V_AV1")
        && track.width.is_some()
        && track.height.is_some()
        && (track.codec_id != "V_AV1" || !track.codec_private.is_empty())
}

fn supported_webm_audio_track(track: &SourceTrack) -> bool {
    if track.has_content_encodings || track.sample_rate.is_none() || track.channels.is_none() {
        return false;
    }
    match track.codec_id.as_str() {
        "A_OPUS" => track.codec_private.starts_with(b"OpusHead"),
        "A_VORBIS" => !track.codec_private.is_empty(),
        _ => false,
    }
}

fn supported_flac_audio_track(track: &SourceTrack) -> bool {
    !track.has_content_encodings
        && track.codec_id == "A_FLAC"
        && track.codec_private.starts_with(b"fLaC")
}

fn supported_mp3_audio_track(track: &SourceTrack) -> bool {
    !track.has_content_encodings && track.codec_id == "A_MPEG/L3"
}

fn supported_ac3_audio_track(track: &SourceTrack) -> bool {
    !track.has_content_encodings && track.codec_id == "A_AC3"
}

fn supported_eac3_audio_track(track: &SourceTrack) -> bool {
    !track.has_content_encodings && track.codec_id == "A_EAC3"
}

fn iso_bmff_mime_type(primary: &SourceTrack, audio: Option<&SourceTrack>) -> String {
    let mut codecs = vec![mp4_codec_string(primary)];
    if let Some(audio) = audio {
        codecs.push(mp4_codec_string(audio));
    }
    let base = if primary.kind == TrackKind::Video {
        "video/mp4"
    } else {
        "audio/mp4"
    };
    format!("{base}; codecs=\"{}\"", codecs.join(", "))
}

fn mp4_codec_string(track: &SourceTrack) -> String {
    match track.codec_id.as_str() {
        "V_MPEG4/ISO/AVC" if track.codec_private.len() >= 4 => format!(
            "avc1.{:02X}{:02X}{:02X}",
            track.codec_private[1], track.codec_private[2], track.codec_private[3]
        ),
        "V_MPEGH/ISO/HEVC" => "hvc1".to_string(),
        "V_AV1" => "av01".to_string(),
        "A_AAC" => {
            let object_type = aac_object_type(&track.codec_private).unwrap_or(2);
            format!("mp4a.40.{object_type}")
        }
        "A_AC3" => "ac-3".to_string(),
        "A_EAC3" => "ec-3".to_string(),
        _ => "unknown".to_string(),
    }
}

fn webm_mime_type(video: Option<&SourceTrack>, audio: Option<&SourceTrack>) -> String {
    let mut codecs = Vec::with_capacity(2);
    if let Some(video) = video {
        codecs.push(match video.codec_id.as_str() {
            "V_VP8" => "vp8",
            "V_VP9" => "vp9",
            "V_AV1" => "av01",
            _ => "unknown",
        });
    }
    if let Some(audio) = audio {
        codecs.push(match audio.codec_id.as_str() {
            "A_OPUS" => "opus",
            "A_VORBIS" => "vorbis",
            _ => "unknown",
        });
    }
    let base = if video.is_some() {
        "video/webm"
    } else {
        "audio/webm"
    };
    format!("{base}; codecs=\"{}\"", codecs.join(", "))
}

fn choose_primary_video_track<'a>(
    tracks: &[&'a SourceTrack],
    supported: fn(&SourceTrack) -> bool,
) -> Option<&'a SourceTrack> {
    let has_default = tracks.iter().any(|track| track.default_track);
    if has_default {
        return tracks
            .iter()
            .copied()
            .find(|track| track.default_track && supported(track));
    }
    tracks.iter().copied().find(|track| supported(track))
}

fn choose_supported_track<'a>(
    tracks: &[&'a SourceTrack],
    supported: fn(&SourceTrack) -> bool,
) -> Option<&'a SourceTrack> {
    tracks
        .iter()
        .copied()
        .find(|track| track.default_track && supported(track))
        .or_else(|| tracks.iter().copied().find(|track| supported(track)))
}

fn build_video_output_track(track_id: u32, track: &SourceTrack) -> io::Result<OutputTrack> {
    let width = u16::try_from(
        track
            .width
            .ok_or_else(|| invalid_data("missing video width"))?,
    )
    .map_err(|_| invalid_data("video width exceeds MP4 compatibility limit"))?;
    let height = u16::try_from(
        track
            .height
            .ok_or_else(|| invalid_data("missing video height"))?,
    )
    .map_err(|_| invalid_data("video height exceeds MP4 compatibility limit"))?;
    if width == 0 || height == 0 {
        return Err(invalid_data("video dimensions must be non-zero"));
    }

    let codec = match track.codec_id.as_str() {
        "V_MPEG4/ISO/AVC" => {
            let nal_length_bytes = validate_avcc(&track.codec_private)?;
            CodecConfig::Avc {
                private: track.codec_private.clone(),
                nal_length_bytes,
            }
        }
        "V_MPEGH/ISO/HEVC" => {
            let nal_length_bytes = validate_hvcc(&track.codec_private)?;
            CodecConfig::Hevc {
                private: track.codec_private.clone(),
                nal_length_bytes,
            }
        }
        "V_AV1" => {
            validate_av1_codec_configuration_record(&track.codec_private)?;
            CodecConfig::Av1 {
                private: track.codec_private.clone(),
            }
        }
        _ => return Err(invalid_data("unsupported Matroska video codec")),
    };

    Ok(OutputTrack {
        track_id,
        source_number: track.number,
        kind: TrackKind::Video,
        codec,
        width,
        height,
        sample_rate: 0,
        channels: 0,
        default_duration_ns: track.default_duration_ns,
        samples: Vec::new(),
        timed_samples: Vec::new(),
        timescale: VIDEO_TIMESCALE,
        media_duration: 0,
    })
}

fn build_audio_output_track(track_id: u32, track: &SourceTrack) -> io::Result<OutputTrack> {
    let sample_rate = track
        .sample_rate
        .ok_or_else(|| invalid_data("missing MP4 audio sample rate"))?;
    if sample_rate == 0 || sample_rate > 65_535 {
        return Err(invalid_data(
            "audio sample rate exceeds narrow MP4 compatibility limit",
        ));
    }
    let channels = track
        .channels
        .ok_or_else(|| invalid_data("missing MP4 audio channel count"))?;
    if channels == 0 || channels > 8 {
        return Err(invalid_data(
            "audio channel count exceeds compatibility limit",
        ));
    }

    let codec = match track.codec_id.as_str() {
        "A_AAC" => {
            let aac = parse_aac_specific_config(&track.codec_private)?;
            if let Some(config_rate) = aac.sample_rate
                && config_rate != sample_rate
            {
                return Err(invalid_data(
                    "AAC CodecPrivate sample rate disagrees with Matroska track",
                ));
            }
            if let Some(config_channels) = aac.channels
                && config_channels != channels
            {
                return Err(invalid_data(
                    "AAC CodecPrivate channel count disagrees with Matroska track",
                ));
            }
            CodecConfig::Aac {
                private: track.codec_private.clone(),
                frame_samples: aac.frame_samples,
            }
        }
        "A_AC3" => CodecConfig::Ac3 { config: None },
        "A_EAC3" => CodecConfig::Eac3 { config: None },
        _ => return Err(invalid_data("unsupported Matroska audio codec for MP4")),
    };

    Ok(OutputTrack {
        track_id,
        source_number: track.number,
        kind: TrackKind::Audio,
        codec,
        width: 0,
        height: 0,
        sample_rate,
        channels,
        default_duration_ns: track.default_duration_ns,
        samples: Vec::new(),
        timed_samples: Vec::new(),
        timescale: sample_rate,
        media_duration: 0,
    })
}

fn process_cluster<W: Write + Seek>(
    source: &mut File,
    cluster: ElementSpan,
    timestamp_scale_ns: u64,
    output: &mut W,
    tracks: &mut [OutputTrack],
) -> io::Result<()> {
    #[derive(Clone, Copy)]
    enum ClusterItemKind {
        SimpleBlock,
        BlockGroup,
    }
    #[derive(Clone, Copy)]
    struct ClusterItem {
        kind: ClusterItemKind,
        span: ElementSpan,
    }

    let mut cluster_timestamp = None;
    let mut items = Vec::new();
    source.seek(SeekFrom::Start(cluster.data_start))?;
    while source.stream_position()? < cluster.data_end {
        let Some(child) = read_element_header(source, cluster.data_end)? else {
            break;
        };
        require_known_size(child, "Matroska Cluster child")?;
        match child.id {
            ID_CLUSTER_TIMESTAMP => cluster_timestamp = Some(read_uint(source, child)?),
            ID_SIMPLE_BLOCK => items.push(ClusterItem {
                kind: ClusterItemKind::SimpleBlock,
                span: ElementSpan {
                    data_start: child.data_start,
                    data_end: child.data_end,
                },
            }),
            ID_BLOCK_GROUP => items.push(ClusterItem {
                kind: ClusterItemKind::BlockGroup,
                span: ElementSpan {
                    data_start: child.data_start,
                    data_end: child.data_end,
                },
            }),
            _ => {}
        }
        source.seek(SeekFrom::Start(child.data_end))?;
    }
    let cluster_timestamp = cluster_timestamp.unwrap_or(0);

    for item in items {
        match item.kind {
            ClusterItemKind::SimpleBlock => {
                let bytes = read_span_bytes(source, item.span, MAX_BLOCK_BYTES)?;
                process_block_bytes(
                    &bytes,
                    BlockContext {
                        is_simple_block: true,
                        block_group_keyframe: true,
                        block_duration_ticks: None,
                        cluster_timestamp,
                        timestamp_scale_ns,
                    },
                    output,
                    tracks,
                )?;
            }
            ClusterItemKind::BlockGroup => {
                process_block_group(
                    source,
                    item.span,
                    cluster_timestamp,
                    timestamp_scale_ns,
                    output,
                    tracks,
                )?;
            }
        }
    }
    Ok(())
}

fn process_block_group<W: Write + Seek>(
    source: &mut File,
    group: ElementSpan,
    cluster_timestamp: u64,
    timestamp_scale_ns: u64,
    output: &mut W,
    tracks: &mut [OutputTrack],
) -> io::Result<()> {
    let mut block = None;
    let mut block_duration = None;
    let mut has_reference = false;
    source.seek(SeekFrom::Start(group.data_start))?;
    while source.stream_position()? < group.data_end {
        let Some(child) = read_element_header(source, group.data_end)? else {
            break;
        };
        require_known_size(child, "Matroska BlockGroup child")?;
        match child.id {
            ID_BLOCK => {
                block = Some(read_bytes(source, child, MAX_BLOCK_BYTES)?);
            }
            ID_BLOCK_DURATION => block_duration = Some(read_uint(source, child)?),
            ID_REFERENCE_BLOCK => has_reference = true,
            ID_BLOCK_ADDITIONS | ID_CODEC_STATE => {
                return Err(invalid_data(
                    "Matroska block uses unsupported supplemental codec data",
                ));
            }
            _ => {}
        }
        source.seek(SeekFrom::Start(child.data_end))?;
    }
    let Some(block) = block else {
        return Ok(());
    };
    process_block_bytes(
        &block,
        BlockContext {
            is_simple_block: false,
            block_group_keyframe: !has_reference,
            block_duration_ticks: block_duration,
            cluster_timestamp,
            timestamp_scale_ns,
        },
        output,
        tracks,
    )
}

#[derive(Clone, Copy, Debug)]
struct BlockContext {
    is_simple_block: bool,
    block_group_keyframe: bool,
    block_duration_ticks: Option<u64>,
    cluster_timestamp: u64,
    timestamp_scale_ns: u64,
}

fn process_block_bytes<W: Write + Seek>(
    bytes: &[u8],
    context: BlockContext,
    output: &mut W,
    tracks: &mut [OutputTrack],
) -> io::Result<()> {
    let parsed = parse_block(bytes, context.is_simple_block)?;
    let Some(track_index) = tracks
        .iter()
        .position(|track| track.source_number == parsed.track_number)
    else {
        return Ok(());
    };
    let track = &mut tracks[track_index];
    if track.samples.len().saturating_add(parsed.frames.len()) > MAX_SAMPLES_PER_TRACK {
        return Err(invalid_data("Matroska track has too many samples"));
    }

    let absolute_ticks =
        i128::from(context.cluster_timestamp) + i128::from(parsed.relative_timestamp);
    if absolute_ticks < 0 {
        return Err(invalid_data(
            "Matroska block has a negative absolute timestamp",
        ));
    }
    let base_pts_ns = u64::try_from(absolute_ticks)
        .ok()
        .and_then(|ticks| ticks.checked_mul(context.timestamp_scale_ns))
        .ok_or_else(|| invalid_data("Matroska timestamp overflows nanoseconds"))?;

    let total_block_duration_ns = context
        .block_duration_ticks
        .and_then(|ticks| ticks.checked_mul(context.timestamp_scale_ns))
        .filter(|value| *value > 0);
    let per_frame_block_duration = total_block_duration_ns
        .and_then(|duration| duration.checked_div(parsed.frames.len() as u64))
        .filter(|duration| *duration > 0);
    let per_frame_hint = per_frame_block_duration.or_else(|| track.duration_hint_ns());
    let keyframe = parsed
        .simple_keyframe
        .unwrap_or(context.block_group_keyframe);

    for (index, frame) in parsed.frames.iter().enumerate() {
        let data = &bytes[frame.start..frame.end];
        track.validate_frame(data)?;
        let frame_offset_ns = per_frame_hint
            .and_then(|duration| duration.checked_mul(index as u64))
            .unwrap_or(0);
        let pts_ns = base_pts_ns
            .checked_add(frame_offset_ns)
            .ok_or_else(|| invalid_data("Matroska frame timestamp overflow"))?;
        let offset = output.stream_position()?;
        output.write_all(data)?;
        track.samples.push(SampleMeta {
            offset,
            size: u32::try_from(data.len())
                .map_err(|_| invalid_data("media sample exceeds MP4 size limit"))?,
            pts_ns,
            explicit_duration_ns: per_frame_hint,
            keyframe,
        });
    }
    Ok(())
}

#[derive(Debug)]
struct ParsedBlock {
    track_number: u64,
    relative_timestamp: i16,
    simple_keyframe: Option<bool>,
    frames: Vec<std::ops::Range<usize>>,
}

fn parse_block(bytes: &[u8], is_simple_block: bool) -> io::Result<ParsedBlock> {
    let (track_number, track_bytes) = parse_vint_from_slice(bytes, false)?;
    if track_number == 0 || track_bytes + 3 > bytes.len() {
        return Err(invalid_data("invalid Matroska block header"));
    }
    let relative_timestamp = i16::from_be_bytes([bytes[track_bytes], bytes[track_bytes + 1]]);
    let flags = bytes[track_bytes + 2];
    let data_start = track_bytes + 3;
    let frames = parse_lacing(bytes, data_start, flags)?;
    if frames.is_empty() {
        return Err(invalid_data("Matroska block contains no frames"));
    }
    Ok(ParsedBlock {
        track_number,
        relative_timestamp,
        simple_keyframe: is_simple_block.then_some(flags & 0x80 != 0),
        frames,
    })
}

fn parse_lacing(
    bytes: &[u8],
    data_start: usize,
    flags: u8,
) -> io::Result<Vec<std::ops::Range<usize>>> {
    let lacing = (flags >> 1) & 0x03;
    if lacing == 0 {
        if data_start >= bytes.len() {
            return Err(invalid_data("empty Matroska block payload"));
        }
        return Ok(std::iter::once(data_start..bytes.len()).collect());
    }
    if data_start >= bytes.len() {
        return Err(invalid_data("missing Matroska lace count"));
    }
    let frame_count = usize::from(bytes[data_start]) + 1;
    if !(2..=256).contains(&frame_count) {
        return Err(invalid_data("invalid Matroska lace frame count"));
    }
    let mut cursor = data_start + 1;
    let mut sizes = Vec::with_capacity(frame_count);

    match lacing {
        1 => {
            for _ in 0..frame_count - 1 {
                let mut size = 0usize;
                loop {
                    let byte = *bytes
                        .get(cursor)
                        .ok_or_else(|| invalid_data("truncated Xiph lace size"))?;
                    cursor += 1;
                    size = size
                        .checked_add(usize::from(byte))
                        .ok_or_else(|| invalid_data("Xiph lace size overflow"))?;
                    if byte != 255 {
                        break;
                    }
                }
                sizes.push(size);
            }
        }
        2 => {
            let remaining = bytes.len().saturating_sub(cursor);
            if remaining == 0 || !remaining.is_multiple_of(frame_count) {
                return Err(invalid_data("invalid fixed-size Matroska lacing"));
            }
            let size = remaining / frame_count;
            sizes.resize(frame_count - 1, size);
        }
        3 => {
            let (first, used) = parse_vint_from_slice(&bytes[cursor..], false)?;
            cursor = cursor
                .checked_add(used)
                .ok_or_else(|| invalid_data("EBML lace cursor overflow"))?;
            let mut previous = i128::from(first);
            sizes.push(
                usize::try_from(first).map_err(|_| invalid_data("EBML lace size too large"))?,
            );
            for _ in 1..frame_count - 1 {
                let (encoded, used, payload_bits) = parse_signed_lace_vint(&bytes[cursor..])?;
                cursor = cursor
                    .checked_add(used)
                    .ok_or_else(|| invalid_data("EBML lace cursor overflow"))?;
                let bias = (1_i128 << (payload_bits - 1)) - 1;
                let delta = i128::from(encoded) - bias;
                let current = previous + delta;
                if current <= 0 || current > i128::from(u32::MAX) {
                    return Err(invalid_data("invalid EBML lace size delta"));
                }
                sizes.push(
                    usize::try_from(current)
                        .map_err(|_| invalid_data("EBML lace size too large"))?,
                );
                previous = current;
            }
        }
        _ => return Err(invalid_data("unsupported Matroska lacing mode")),
    }

    let known_total = sizes.iter().try_fold(0usize, |sum, size| {
        sum.checked_add(*size)
            .ok_or_else(|| invalid_data("Matroska lace size overflow"))
    })?;
    let remaining = bytes.len().saturating_sub(cursor);
    if known_total >= remaining {
        return Err(invalid_data("Matroska lace sizes exceed block payload"));
    }
    sizes.push(remaining - known_total);

    let mut frames = Vec::with_capacity(frame_count);
    let mut frame_start = cursor;
    for size in sizes {
        if size == 0 {
            return Err(invalid_data("Matroska lace contains an empty frame"));
        }
        let frame_end = frame_start
            .checked_add(size)
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| invalid_data("Matroska lace frame exceeds block"))?;
        frames.push(frame_start..frame_end);
        frame_start = frame_end;
    }
    if frame_start != bytes.len() {
        return Err(invalid_data("Matroska lace did not consume block payload"));
    }
    Ok(frames)
}

fn parse_signed_lace_vint(bytes: &[u8]) -> io::Result<(u64, usize, u32)> {
    let first = *bytes
        .first()
        .ok_or_else(|| invalid_data("truncated EBML lace VINT"))?;
    let len = vint_length(first, 8)?;
    if bytes.len() < len {
        return Err(invalid_data("truncated EBML lace VINT"));
    }
    let marker = 1_u8 << (8 - len);
    let mut value = u64::from(first & (marker - 1));
    for byte in &bytes[1..len] {
        value = (value << 8) | u64::from(*byte);
    }
    Ok((value, len, (7 * len) as u32))
}

fn finalize_track_timing(
    track: &mut OutputTrack,
    global_base_ns: u64,
    fallback_ns: u64,
) -> io::Result<()> {
    if matches!(track.codec, CodecConfig::Av1 { .. }) {
        return finalize_av1_track_timing(track, global_base_ns, fallback_ns);
    }
    let mut decode_time = 0_u64;
    let mut timed = Vec::with_capacity(track.samples.len());
    let mut any_keyframe = false;

    for sample in &track.samples {
        let duration_ns = sample.explicit_duration_ns.unwrap_or(fallback_ns).max(1);
        let duration = u32::try_from(ns_to_ticks(duration_ns, track.timescale).max(1))
            .map_err(|_| invalid_data("sample duration exceeds MP4 limit"))?;
        let pts_ticks = ns_to_ticks(
            sample.pts_ns.saturating_sub(global_base_ns),
            track.timescale,
        );
        let composition_offset_i128 = i128::from(pts_ticks) - i128::from(decode_time);
        let composition_offset = i32::try_from(composition_offset_i128)
            .map_err(|_| invalid_data("composition offset exceeds MP4 limit"))?;
        any_keyframe |= sample.keyframe;
        timed.push(TimedSample {
            offset: sample.offset,
            size: sample.size,
            duration,
            composition_offset,
            keyframe: sample.keyframe,
        });
        decode_time = decode_time
            .checked_add(u64::from(duration))
            .ok_or_else(|| invalid_data("MP4 decode timeline overflow"))?;
    }

    if track.kind == TrackKind::Video
        && !any_keyframe
        && let Some(first) = timed.first_mut()
    {
        first.keyframe = true;
    }
    track.media_duration = decode_time;
    track.timed_samples = timed;
    Ok(())
}

fn finalize_av1_track_timing(
    track: &mut OutputTrack,
    global_base_ns: u64,
    fallback_ns: u64,
) -> io::Result<()> {
    let pts = track
        .samples
        .iter()
        .map(|sample| {
            ns_to_ticks(
                sample.pts_ns.saturating_sub(global_base_ns),
                track.timescale,
            )
        })
        .collect::<Vec<_>>();
    if pts.first().copied().unwrap_or_default() != 0 {
        return Err(invalid_data(
            "AV1 MP4 compatibility requires the AV1 track to start on the movie timeline origin",
        ));
    }
    if pts.windows(2).any(|pair| pair[1] <= pair[0]) {
        return Err(invalid_data(
            "AV1 MP4 compatibility requires presentation order to match decode order",
        ));
    }

    let mut timed = Vec::with_capacity(track.samples.len());
    let mut media_duration = 0_u64;
    let mut any_keyframe = false;
    for (index, sample) in track.samples.iter().enumerate() {
        let duration = if let Some(next_pts) = pts.get(index + 1) {
            u32::try_from(next_pts.saturating_sub(pts[index]))
                .map_err(|_| invalid_data("AV1 sample duration exceeds MP4 limit"))?
        } else {
            let duration_ns = sample.explicit_duration_ns.unwrap_or(fallback_ns).max(1);
            u32::try_from(ns_to_ticks(duration_ns, track.timescale).max(1))
                .map_err(|_| invalid_data("AV1 sample duration exceeds MP4 limit"))?
        };
        if duration == 0 {
            return Err(invalid_data("AV1 sample duration must be non-zero"));
        }
        any_keyframe |= sample.keyframe;
        timed.push(TimedSample {
            offset: sample.offset,
            size: sample.size,
            duration,
            composition_offset: 0,
            keyframe: sample.keyframe,
        });
        media_duration = media_duration
            .checked_add(u64::from(duration))
            .ok_or_else(|| invalid_data("AV1 MP4 decode timeline overflow"))?;
    }
    if !any_keyframe && let Some(first) = timed.first_mut() {
        first.keyframe = true;
    }
    track.media_duration = media_duration;
    track.timed_samples = timed;
    Ok(())
}

fn track_duration_fallback_ns(track: &OutputTrack) -> io::Result<u64> {
    if let Some(value) = track.duration_hint_ns().filter(|value| *value > 0) {
        return Ok(value);
    }
    if track.kind == TrackKind::Audio {
        return Err(invalid_data(
            "MP4 audio track has no usable frame-duration information",
        ));
    }

    let mut pts: Vec<u64> = track.samples.iter().map(|sample| sample.pts_ns).collect();
    pts.sort_unstable();
    pts.dedup();
    let mut deltas: Vec<u64> = pts
        .windows(2)
        .filter_map(|pair| pair[1].checked_sub(pair[0]))
        .filter(|delta| (1_000_000..=1_000_000_000).contains(delta))
        .collect();
    if deltas.is_empty() {
        return Ok(33_333_333);
    }
    deltas.sort_unstable();
    Ok(deltas[deltas.len() / 2])
}

fn write_ftyp<W: Write>(writer: &mut W, tracks: &[OutputTrack]) -> io::Result<()> {
    let mut body = Vec::new();
    body.extend_from_slice(b"isom");
    body.extend_from_slice(&512_u32.to_be_bytes());
    for brand in [b"isom".as_slice(), b"iso6".as_slice(), b"mp41".as_slice()] {
        body.extend_from_slice(brand);
    }
    if tracks
        .iter()
        .any(|track| matches!(track.codec, CodecConfig::Avc { .. }))
    {
        body.extend_from_slice(b"avc1");
    }
    if tracks
        .iter()
        .any(|track| matches!(track.codec, CodecConfig::Hevc { .. }))
    {
        body.extend_from_slice(b"hvc1");
    }
    if tracks
        .iter()
        .any(|track| matches!(track.codec, CodecConfig::Av1 { .. }))
    {
        body.extend_from_slice(b"av01");
    }
    if tracks
        .iter()
        .any(|track| matches!(track.codec, CodecConfig::Eac3 { .. }))
    {
        body.extend_from_slice(b"dby1");
    }
    write_box_to(writer, b"ftyp", &body)
}

fn build_moov(tracks: &[OutputTrack], movie_duration: u64) -> io::Result<Vec<u8>> {
    let mut body = Vec::new();
    body.extend_from_slice(&build_mvhd(movie_duration, tracks.len())?);
    for track in tracks {
        body.extend_from_slice(&build_trak(track, movie_duration)?);
    }
    boxed(*b"moov", body)
}

fn build_mvhd(movie_duration: u64, track_count: usize) -> io::Result<Vec<u8>> {
    let mut body = Vec::new();
    write_full_box_header(&mut body, 1, 0);
    body.extend_from_slice(&0_u64.to_be_bytes());
    body.extend_from_slice(&0_u64.to_be_bytes());
    body.extend_from_slice(&MOVIE_TIMESCALE.to_be_bytes());
    body.extend_from_slice(&movie_duration.to_be_bytes());
    body.extend_from_slice(&0x0001_0000_u32.to_be_bytes());
    body.extend_from_slice(&0x0100_u16.to_be_bytes());
    body.extend_from_slice(&0_u16.to_be_bytes());
    body.extend_from_slice(&[0_u8; 8]);
    write_identity_matrix(&mut body);
    body.extend_from_slice(&[0_u8; 24]);
    let next_track =
        u32::try_from(track_count + 1).map_err(|_| invalid_data("too many MP4 tracks"))?;
    body.extend_from_slice(&next_track.to_be_bytes());
    boxed(*b"mvhd", body)
}

fn build_trak(track: &OutputTrack, movie_duration: u64) -> io::Result<Vec<u8>> {
    let mut body = Vec::new();
    body.extend_from_slice(&build_tkhd(track, movie_duration)?);
    body.extend_from_slice(&build_mdia(track)?);
    boxed(*b"trak", body)
}

fn build_tkhd(track: &OutputTrack, movie_duration: u64) -> io::Result<Vec<u8>> {
    let mut body = Vec::new();
    write_full_box_header(&mut body, 1, 0x000007);
    body.extend_from_slice(&0_u64.to_be_bytes());
    body.extend_from_slice(&0_u64.to_be_bytes());
    body.extend_from_slice(&track.track_id.to_be_bytes());
    body.extend_from_slice(&0_u32.to_be_bytes());
    body.extend_from_slice(&movie_duration.to_be_bytes());
    body.extend_from_slice(&[0_u8; 8]);
    body.extend_from_slice(&0_i16.to_be_bytes());
    body.extend_from_slice(&0_i16.to_be_bytes());
    let volume = if track.kind == TrackKind::Audio {
        0x0100_u16
    } else {
        0_u16
    };
    body.extend_from_slice(&volume.to_be_bytes());
    body.extend_from_slice(&0_u16.to_be_bytes());
    write_identity_matrix(&mut body);
    body.extend_from_slice(&(u32::from(track.width) << 16).to_be_bytes());
    body.extend_from_slice(&(u32::from(track.height) << 16).to_be_bytes());
    boxed(*b"tkhd", body)
}

fn build_mdia(track: &OutputTrack) -> io::Result<Vec<u8>> {
    let mut body = Vec::new();
    body.extend_from_slice(&build_mdhd(track)?);
    body.extend_from_slice(&build_hdlr(track)?);
    body.extend_from_slice(&build_minf(track)?);
    boxed(*b"mdia", body)
}

fn build_mdhd(track: &OutputTrack) -> io::Result<Vec<u8>> {
    let mut body = Vec::new();
    write_full_box_header(&mut body, 1, 0);
    body.extend_from_slice(&0_u64.to_be_bytes());
    body.extend_from_slice(&0_u64.to_be_bytes());
    body.extend_from_slice(&track.timescale.to_be_bytes());
    body.extend_from_slice(&track.media_duration.to_be_bytes());
    body.extend_from_slice(&0x55C4_u16.to_be_bytes());
    body.extend_from_slice(&0_u16.to_be_bytes());
    boxed(*b"mdhd", body)
}

fn build_hdlr(track: &OutputTrack) -> io::Result<Vec<u8>> {
    let mut body = Vec::new();
    write_full_box_header(&mut body, 0, 0);
    body.extend_from_slice(&0_u32.to_be_bytes());
    body.extend_from_slice(if track.kind == TrackKind::Video {
        b"vide"
    } else {
        b"soun"
    });
    body.extend_from_slice(&[0_u8; 12]);
    body.extend_from_slice(if track.kind == TrackKind::Video {
        b"Rustnote Video\0"
    } else {
        b"Rustnote Audio\0"
    });
    boxed(*b"hdlr", body)
}

fn build_minf(track: &OutputTrack) -> io::Result<Vec<u8>> {
    let mut body = Vec::new();
    if track.kind == TrackKind::Video {
        let mut vmhd = Vec::new();
        write_full_box_header(&mut vmhd, 0, 1);
        vmhd.extend_from_slice(&[0_u8; 8]);
        body.extend_from_slice(&boxed(*b"vmhd", vmhd)?);
    } else {
        let mut smhd = Vec::new();
        write_full_box_header(&mut smhd, 0, 0);
        smhd.extend_from_slice(&[0_u8; 4]);
        body.extend_from_slice(&boxed(*b"smhd", smhd)?);
    }
    body.extend_from_slice(&build_dinf()?);
    body.extend_from_slice(&build_stbl(track)?);
    boxed(*b"minf", body)
}

fn build_dinf() -> io::Result<Vec<u8>> {
    let mut url = Vec::new();
    write_full_box_header(&mut url, 0, 1);
    let url = boxed(*b"url ", url)?;

    let mut dref = Vec::new();
    write_full_box_header(&mut dref, 0, 0);
    dref.extend_from_slice(&1_u32.to_be_bytes());
    dref.extend_from_slice(&url);
    let dref = boxed(*b"dref", dref)?;
    boxed(*b"dinf", dref)
}

fn build_stbl(track: &OutputTrack) -> io::Result<Vec<u8>> {
    let mut body = Vec::new();
    body.extend_from_slice(&build_stsd(track)?);
    body.extend_from_slice(&build_stts(track)?);
    if track
        .timed_samples
        .iter()
        .any(|sample| sample.composition_offset != 0)
    {
        body.extend_from_slice(&build_ctts(track)?);
    }
    body.extend_from_slice(&build_stsc()?);
    body.extend_from_slice(&build_stsz(track)?);
    body.extend_from_slice(&build_co64(track)?);
    if track.kind == TrackKind::Video {
        body.extend_from_slice(&build_stss(track)?);
    }
    boxed(*b"stbl", body)
}

fn build_stsd(track: &OutputTrack) -> io::Result<Vec<u8>> {
    let sample_entry =
        match &track.codec {
            CodecConfig::Avc { private, .. } => {
                build_video_sample_entry(*b"avc1", *b"avcC", private, track.width, track.height)?
            }
            CodecConfig::Hevc { private, .. } => {
                build_video_sample_entry(*b"hvc1", *b"hvcC", private, track.width, track.height)?
            }
            CodecConfig::Av1 { private } => {
                build_video_sample_entry(*b"av01", *b"av1C", private, track.width, track.height)?
            }
            CodecConfig::Aac { private, .. } => {
                build_audio_sample_entry(private, track.channels, track.sample_rate)?
            }
            CodecConfig::Ac3 { config } => {
                build_dolby_audio_sample_entry(
                    *b"ac-3",
                    *b"dac3",
                    build_dac3_body(config.as_ref().copied().ok_or_else(|| {
                        invalid_data("AC-3 track contained no validated samples")
                    })?),
                    track.sample_rate,
                )?
            }
            CodecConfig::Eac3 { config } => {
                build_dolby_audio_sample_entry(
                    *b"ec-3",
                    *b"dec3",
                    build_dec3_body(config.as_ref().copied().ok_or_else(|| {
                        invalid_data("E-AC-3 track contained no validated samples")
                    })?),
                    track.sample_rate,
                )?
            }
        };
    let mut body = Vec::new();
    write_full_box_header(&mut body, 0, 0);
    body.extend_from_slice(&1_u32.to_be_bytes());
    body.extend_from_slice(&sample_entry);
    boxed(*b"stsd", body)
}

fn build_video_sample_entry(
    entry_type: [u8; 4],
    config_type: [u8; 4],
    codec_private: &[u8],
    width: u16,
    height: u16,
) -> io::Result<Vec<u8>> {
    let mut body = Vec::new();
    body.extend_from_slice(&[0_u8; 6]);
    body.extend_from_slice(&1_u16.to_be_bytes());
    body.extend_from_slice(&[0_u8; 16]);
    body.extend_from_slice(&width.to_be_bytes());
    body.extend_from_slice(&height.to_be_bytes());
    body.extend_from_slice(&0x0048_0000_u32.to_be_bytes());
    body.extend_from_slice(&0x0048_0000_u32.to_be_bytes());
    body.extend_from_slice(&0_u32.to_be_bytes());
    body.extend_from_slice(&1_u16.to_be_bytes());
    body.extend_from_slice(&[0_u8; 32]);
    body.extend_from_slice(&0x0018_u16.to_be_bytes());
    body.extend_from_slice(&0xFFFF_u16.to_be_bytes());
    body.extend_from_slice(&boxed(config_type, codec_private.to_vec())?);
    boxed(entry_type, body)
}

fn build_audio_sample_entry(
    codec_private: &[u8],
    channels: u16,
    sample_rate: u32,
) -> io::Result<Vec<u8>> {
    let mut body = Vec::new();
    body.extend_from_slice(&[0_u8; 6]);
    body.extend_from_slice(&1_u16.to_be_bytes());
    body.extend_from_slice(&[0_u8; 8]);
    body.extend_from_slice(&channels.to_be_bytes());
    body.extend_from_slice(&16_u16.to_be_bytes());
    body.extend_from_slice(&0_u16.to_be_bytes());
    body.extend_from_slice(&0_u16.to_be_bytes());
    body.extend_from_slice(&(sample_rate << 16).to_be_bytes());
    body.extend_from_slice(&build_esds(codec_private)?);
    boxed(*b"mp4a", body)
}

fn build_dolby_audio_sample_entry(
    entry_type: [u8; 4],
    config_type: [u8; 4],
    config_body: Vec<u8>,
    sample_rate: u32,
) -> io::Result<Vec<u8>> {
    let mut body = Vec::new();
    body.extend_from_slice(&[0_u8; 6]);
    body.extend_from_slice(&1_u16.to_be_bytes());
    body.extend_from_slice(&[0_u8; 8]);
    // AC-3/EC-3 AudioSampleEntry uses the fixed ChannelCount value 2;
    // channel layout is conveyed by dac3/dec3 and the encoded stream.
    body.extend_from_slice(&2_u16.to_be_bytes());
    body.extend_from_slice(&16_u16.to_be_bytes());
    body.extend_from_slice(&0_u16.to_be_bytes());
    body.extend_from_slice(&0_u16.to_be_bytes());
    body.extend_from_slice(&(sample_rate << 16).to_be_bytes());
    body.extend_from_slice(&boxed(config_type, config_body)?);
    boxed(entry_type, body)
}

fn build_dac3_body(config: Ac3Config) -> Vec<u8> {
    let value = (u32::from(config.fscod) << 22)
        | (u32::from(config.bsid) << 17)
        | (u32::from(config.bsmod) << 14)
        | (u32::from(config.acmod) << 11)
        | (u32::from(u8::from(config.lfeon)) << 10)
        | (u32::from(config.bit_rate_code) << 5);
    value.to_be_bytes()[1..].to_vec()
}

fn build_dec3_body(config: Eac3Config) -> Vec<u8> {
    // EC3SpecificBox: 13-bit data_rate + num_ind_sub=0, followed by one
    // independent-substream descriptor. No dependent substreams are accepted.
    let header = (config.max_data_rate_kbps << 3).to_be_bytes();
    let substream = (u32::from(config.fscod) << 22)
        | (u32::from(config.bsid) << 17)
        | (u32::from(config.bsmod) << 12)
        | (u32::from(config.acmod) << 9)
        | (u32::from(u8::from(config.lfeon)) << 8);
    let substream_bytes = substream.to_be_bytes();
    let mut body = vec![
        header[0],
        header[1],
        substream_bytes[1],
        substream_bytes[2],
        substream_bytes[3],
    ];
    if let Some(complexity) = config.complexity_index_type_a {
        // Seven reserved bits + flag_eac3_extension_type_a, then complexity.
        body.extend_from_slice(&[0x01, complexity]);
    }
    body
}

fn build_esds(audio_specific_config: &[u8]) -> io::Result<Vec<u8>> {
    let mut decoder_specific = vec![0x05];
    write_descriptor_length(&mut decoder_specific, audio_specific_config.len())?;
    decoder_specific.extend_from_slice(audio_specific_config);

    let mut decoder_config_body = Vec::new();
    decoder_config_body.push(0x40);
    decoder_config_body.push(0x15);
    decoder_config_body.extend_from_slice(&[0_u8; 3]);
    decoder_config_body.extend_from_slice(&0_u32.to_be_bytes());
    decoder_config_body.extend_from_slice(&0_u32.to_be_bytes());
    decoder_config_body.extend_from_slice(&decoder_specific);
    decoder_config_body.extend_from_slice(&[0x06, 0x01, 0x02]);
    let mut decoder_config = vec![0x04];
    write_descriptor_length(&mut decoder_config, decoder_config_body.len())?;
    decoder_config.extend_from_slice(&decoder_config_body);

    let mut es_body = Vec::new();
    es_body.extend_from_slice(&1_u16.to_be_bytes());
    es_body.push(0);
    es_body.extend_from_slice(&decoder_config);
    let mut descriptor = vec![0x03];
    write_descriptor_length(&mut descriptor, es_body.len())?;
    descriptor.extend_from_slice(&es_body);

    let mut body = Vec::new();
    write_full_box_header(&mut body, 0, 0);
    body.extend_from_slice(&descriptor);
    boxed(*b"esds", body)
}

fn write_descriptor_length(output: &mut Vec<u8>, length: usize) -> io::Result<()> {
    if length > 0x0FFF_FFFF {
        return Err(invalid_data("MPEG-4 descriptor is too large"));
    }
    let mut bytes = [0_u8; 4];
    let mut value = length as u32;
    let mut index = 3;
    bytes[index] = (value & 0x7F) as u8;
    value >>= 7;
    while value > 0 {
        index -= 1;
        bytes[index] = ((value & 0x7F) as u8) | 0x80;
        value >>= 7;
    }
    output.extend_from_slice(&bytes[index..]);
    Ok(())
}

fn build_stts(track: &OutputTrack) -> io::Result<Vec<u8>> {
    let entries = run_length_u32(track.timed_samples.iter().map(|sample| sample.duration));
    let mut body = Vec::new();
    write_full_box_header(&mut body, 0, 0);
    body.extend_from_slice(
        &u32::try_from(entries.len())
            .map_err(|_| invalid_data("too many stts entries"))?
            .to_be_bytes(),
    );
    for (count, value) in entries {
        body.extend_from_slice(&count.to_be_bytes());
        body.extend_from_slice(&value.to_be_bytes());
    }
    boxed(*b"stts", body)
}

fn build_ctts(track: &OutputTrack) -> io::Result<Vec<u8>> {
    let entries = run_length_i32(
        track
            .timed_samples
            .iter()
            .map(|sample| sample.composition_offset),
    );
    let mut body = Vec::new();
    write_full_box_header(&mut body, 1, 0);
    body.extend_from_slice(
        &u32::try_from(entries.len())
            .map_err(|_| invalid_data("too many ctts entries"))?
            .to_be_bytes(),
    );
    for (count, value) in entries {
        body.extend_from_slice(&count.to_be_bytes());
        body.extend_from_slice(&value.to_be_bytes());
    }
    boxed(*b"ctts", body)
}

fn build_stsc() -> io::Result<Vec<u8>> {
    let mut body = Vec::new();
    write_full_box_header(&mut body, 0, 0);
    body.extend_from_slice(&1_u32.to_be_bytes());
    body.extend_from_slice(&1_u32.to_be_bytes());
    body.extend_from_slice(&1_u32.to_be_bytes());
    body.extend_from_slice(&1_u32.to_be_bytes());
    boxed(*b"stsc", body)
}

fn build_stsz(track: &OutputTrack) -> io::Result<Vec<u8>> {
    let mut body = Vec::new();
    write_full_box_header(&mut body, 0, 0);
    body.extend_from_slice(&0_u32.to_be_bytes());
    body.extend_from_slice(
        &u32::try_from(track.timed_samples.len())
            .map_err(|_| invalid_data("too many MP4 samples"))?
            .to_be_bytes(),
    );
    for sample in &track.timed_samples {
        body.extend_from_slice(&sample.size.to_be_bytes());
    }
    boxed(*b"stsz", body)
}

fn build_co64(track: &OutputTrack) -> io::Result<Vec<u8>> {
    let mut body = Vec::new();
    write_full_box_header(&mut body, 0, 0);
    body.extend_from_slice(
        &u32::try_from(track.timed_samples.len())
            .map_err(|_| invalid_data("too many MP4 chunk offsets"))?
            .to_be_bytes(),
    );
    for sample in &track.timed_samples {
        body.extend_from_slice(&sample.offset.to_be_bytes());
    }
    boxed(*b"co64", body)
}

fn build_stss(track: &OutputTrack) -> io::Result<Vec<u8>> {
    let sync_samples: Vec<u32> = track
        .timed_samples
        .iter()
        .enumerate()
        .filter_map(|(index, sample)| {
            if sample.keyframe {
                u32::try_from(index + 1).ok()
            } else {
                None
            }
        })
        .collect();
    let mut body = Vec::new();
    write_full_box_header(&mut body, 0, 0);
    body.extend_from_slice(
        &u32::try_from(sync_samples.len())
            .map_err(|_| invalid_data("too many sync samples"))?
            .to_be_bytes(),
    );
    for sample in sync_samples {
        body.extend_from_slice(&sample.to_be_bytes());
    }
    boxed(*b"stss", body)
}

fn run_length_u32(values: impl Iterator<Item = u32>) -> Vec<(u32, u32)> {
    let mut result = Vec::new();
    for value in values {
        if let Some((count, previous)) = result.last_mut()
            && *previous == value
            && *count < u32::MAX
        {
            *count += 1;
            continue;
        }
        result.push((1, value));
    }
    result
}

fn run_length_i32(values: impl Iterator<Item = i32>) -> Vec<(u32, i32)> {
    let mut result = Vec::new();
    for value in values {
        if let Some((count, previous)) = result.last_mut()
            && *previous == value
            && *count < u32::MAX
        {
            *count += 1;
            continue;
        }
        result.push((1, value));
    }
    result
}

fn boxed(kind: [u8; 4], body: Vec<u8>) -> io::Result<Vec<u8>> {
    let size = body
        .len()
        .checked_add(8)
        .ok_or_else(|| invalid_data("MP4 box size overflow"))?;
    let size = u32::try_from(size).map_err(|_| invalid_data("MP4 metadata box exceeds 4 GiB"))?;
    let mut output = Vec::with_capacity(size as usize);
    output.extend_from_slice(&size.to_be_bytes());
    output.extend_from_slice(&kind);
    output.extend_from_slice(&body);
    Ok(output)
}

fn write_box_to<W: Write>(writer: &mut W, kind: &[u8; 4], body: &[u8]) -> io::Result<()> {
    let size = body
        .len()
        .checked_add(8)
        .and_then(|size| u32::try_from(size).ok())
        .ok_or_else(|| invalid_data("MP4 box is too large"))?;
    writer.write_all(&size.to_be_bytes())?;
    writer.write_all(kind)?;
    writer.write_all(body)
}

fn write_full_box_header(body: &mut Vec<u8>, version: u8, flags: u32) {
    body.push(version);
    body.push(((flags >> 16) & 0xFF) as u8);
    body.push(((flags >> 8) & 0xFF) as u8);
    body.push((flags & 0xFF) as u8);
}

fn write_identity_matrix(body: &mut Vec<u8>) {
    for value in [0x0001_0000_u32, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000] {
        body.extend_from_slice(&value.to_be_bytes());
    }
}

fn validate_avcc(data: &[u8]) -> io::Result<usize> {
    if data.len() < 7 || data[0] != 1 {
        return Err(invalid_data("invalid AVCDecoderConfigurationRecord"));
    }
    let nal_length_bytes = usize::from((data[4] & 0x03) + 1);
    if !matches!(nal_length_bytes, 1 | 2 | 4) {
        return Err(invalid_data("unsupported AVC NAL length size"));
    }
    let mut cursor = 6usize;
    let sps_count = usize::from(data[5] & 0x1F);
    if sps_count == 0 {
        return Err(invalid_data("AVC configuration has no SPS"));
    }
    for _ in 0..sps_count {
        cursor = skip_u16_sized_blob(data, cursor)?;
    }
    let pps_count = usize::from(
        *data
            .get(cursor)
            .ok_or_else(|| invalid_data("truncated AVC configuration"))?,
    );
    if pps_count == 0 {
        return Err(invalid_data("AVC configuration has no PPS"));
    }
    cursor += 1;
    for _ in 0..pps_count {
        cursor = skip_u16_sized_blob(data, cursor)?;
    }
    if cursor != data.len() {
        return Err(invalid_data(
            "AVC configuration contains unsupported extension data",
        ));
    }
    Ok(nal_length_bytes)
}

fn validate_hvcc(data: &[u8]) -> io::Result<usize> {
    if data.len() < 23 || data[0] != 1 {
        return Err(invalid_data("invalid HEVCDecoderConfigurationRecord"));
    }
    let nal_length_bytes = usize::from((data[21] & 0x03) + 1);
    if !matches!(nal_length_bytes, 1 | 2 | 4) {
        return Err(invalid_data("unsupported HEVC NAL length size"));
    }
    let arrays = usize::from(data[22]);
    let mut cursor = 23usize;
    for _ in 0..arrays {
        cursor = cursor
            .checked_add(1)
            .filter(|value| *value + 2 <= data.len())
            .ok_or_else(|| invalid_data("truncated HEVC configuration array"))?;
        let count = u16::from_be_bytes([data[cursor], data[cursor + 1]]) as usize;
        cursor += 2;
        for _ in 0..count {
            cursor = skip_u16_sized_blob(data, cursor)?;
        }
    }
    if cursor != data.len() {
        return Err(invalid_data("unexpected trailing HEVC configuration data"));
    }
    Ok(nal_length_bytes)
}

fn validate_av1_codec_configuration_record(data: &[u8]) -> io::Result<()> {
    if data.len() < 4 || data[0] != 0x81 {
        return Err(invalid_data("invalid AV1CodecConfigurationRecord"));
    }
    let seq_profile = data[1] >> 5;
    if seq_profile > 2 {
        return Err(invalid_data(
            "unsupported AV1 profile in configuration record",
        ));
    }
    let high_bitdepth = (data[2] & 0x40) != 0;
    let twelve_bit = (data[2] & 0x20) != 0;
    if twelve_bit && !high_bitdepth {
        return Err(invalid_data(
            "invalid AV1 bit-depth flags in configuration record",
        ));
    }
    if (data[3] & 0xE0) != 0 {
        return Err(invalid_data("AV1 configuration reserved bits are non-zero"));
    }
    let initial_presentation_delay_present = (data[3] & 0x10) != 0;
    if !initial_presentation_delay_present && (data[3] & 0x0F) != 0 {
        return Err(invalid_data(
            "AV1 configuration reserved presentation-delay bits are non-zero",
        ));
    }
    if data.len() > 4 {
        validate_av1_obu_sequence(&data[4..], false)?;
    }
    Ok(())
}

fn validate_av1_sample(data: &[u8]) -> io::Result<()> {
    if data.is_empty() {
        return Err(invalid_data("empty AV1 sample in Matroska track"));
    }
    validate_av1_obu_sequence(data, true)
}

fn validate_av1_obu_sequence(data: &[u8], allow_last_without_size: bool) -> io::Result<()> {
    let mut cursor = 0usize;
    let mut obu_count = 0usize;
    while cursor < data.len() {
        let header = data[cursor];
        cursor += 1;
        if (header & 0x81) != 0 {
            return Err(invalid_data(
                "AV1 OBU header contains forbidden or reserved bits",
            ));
        }
        let obu_type = (header >> 3) & 0x0F;
        if obu_type == 0 {
            return Err(invalid_data("AV1 OBU uses reserved type zero"));
        }
        if obu_type == 8 {
            return Err(invalid_data("AV1 tile-list OBUs are not supported in MP4"));
        }
        let extension_flag = (header & 0x04) != 0;
        let has_size_field = (header & 0x02) != 0;
        if extension_flag {
            let extension = *data
                .get(cursor)
                .ok_or_else(|| invalid_data("truncated AV1 OBU extension header"))?;
            cursor += 1;
            if (extension & 0x07) != 0 {
                return Err(invalid_data("AV1 OBU extension reserved bits are non-zero"));
            }
        }
        if has_size_field {
            let (payload_size, leb_len) = read_av1_leb128(&data[cursor..])?;
            cursor = cursor
                .checked_add(leb_len)
                .and_then(|value| value.checked_add(payload_size))
                .filter(|value| *value <= data.len())
                .ok_or_else(|| invalid_data("AV1 OBU payload exceeds sample bounds"))?;
        } else {
            if !allow_last_without_size {
                return Err(invalid_data(
                    "AV1 configuration OBUs must include explicit size fields",
                ));
            }
            cursor = data.len();
        }
        obu_count += 1;
    }
    if obu_count == 0 {
        return Err(invalid_data("AV1 data contains no OBUs"));
    }
    Ok(())
}

fn read_av1_leb128(data: &[u8]) -> io::Result<(usize, usize)> {
    let mut value = 0u64;
    for (index, byte) in data.iter().copied().take(8).enumerate() {
        let shift =
            u32::try_from(index * 7).map_err(|_| invalid_data("AV1 OBU size shift overflow"))?;
        value |= u64::from(byte & 0x7F) << shift;
        if (byte & 0x80) == 0 {
            let size = usize::try_from(value)
                .map_err(|_| invalid_data("AV1 OBU size exceeds this platform"))?;
            return Ok((size, index + 1));
        }
    }
    Err(invalid_data("invalid or oversized AV1 OBU size field"))
}

fn skip_u16_sized_blob(data: &[u8], cursor: usize) -> io::Result<usize> {
    if cursor + 2 > data.len() {
        return Err(invalid_data("truncated codec configuration"));
    }
    let size = u16::from_be_bytes([data[cursor], data[cursor + 1]]) as usize;
    let end = cursor
        .checked_add(2)
        .and_then(|value| value.checked_add(size))
        .filter(|end| *end <= data.len())
        .ok_or_else(|| invalid_data("codec configuration blob exceeds bounds"))?;
    Ok(end)
}

fn validate_length_prefixed_nals(data: &[u8], length_bytes: usize) -> io::Result<()> {
    if data.is_empty() {
        return Err(invalid_data("empty video sample"));
    }
    let mut cursor = 0usize;
    let mut nal_count = 0usize;
    while cursor < data.len() {
        if cursor + length_bytes > data.len() {
            return Err(invalid_data("truncated length-prefixed video sample"));
        }
        let mut length = 0usize;
        for byte in &data[cursor..cursor + length_bytes] {
            length = length
                .checked_mul(256)
                .and_then(|value| value.checked_add(usize::from(*byte)))
                .ok_or_else(|| invalid_data("video NAL length overflow"))?;
        }
        cursor += length_bytes;
        if length == 0 || cursor + length > data.len() {
            return Err(invalid_data("invalid video NAL length"));
        }
        cursor += length;
        nal_count += 1;
    }
    if nal_count == 0 {
        return Err(invalid_data("video sample contains no NAL units"));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug)]
struct ParsedAc3Frame {
    config: Ac3Config,
    sample_rate: u32,
    channels: u16,
}

#[derive(Clone, Copy, Debug)]
struct ParsedEac3Frame {
    config: Eac3Config,
    sample_rate: u32,
    channels: u16,
}

fn validate_dolby_track_metadata(
    track_sample_rate: u32,
    track_channels: u16,
    frame_sample_rate: u32,
    frame_channels: u16,
) -> io::Result<()> {
    if track_sample_rate != frame_sample_rate {
        return Err(invalid_data(
            "Dolby audio frame sample rate disagrees with Matroska track",
        ));
    }
    if track_channels != frame_channels {
        return Err(invalid_data(
            "Dolby audio frame channel count disagrees with Matroska track",
        ));
    }
    Ok(())
}

fn dolby_channels(acmod: u8, lfeon: bool) -> io::Result<u16> {
    const FULL_BAND_CHANNELS: [u16; 8] = [2, 1, 2, 3, 3, 4, 4, 5];
    let base = FULL_BAND_CHANNELS
        .get(usize::from(acmod))
        .copied()
        .ok_or_else(|| invalid_data("invalid Dolby audio coding mode"))?;
    Ok(base + u16::from(lfeon))
}

fn parse_ac3_frame(data: &[u8]) -> io::Result<ParsedAc3Frame> {
    const SAMPLE_RATES: [u32; 3] = [48_000, 44_100, 32_000];
    const BIT_RATES_KBPS: [u32; 19] = [
        32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384, 448, 512, 576, 640,
    ];

    if data.len() < 7 || data[0..2] != [0x0B, 0x77] {
        return Err(invalid_data("invalid AC-3 syncframe"));
    }
    let mut bits = BitReader::new(&data[4..]);
    let fscod = u8::try_from(bits.read(2)?).map_err(|_| invalid_data("invalid AC-3 fscod"))?;
    let sample_rate = SAMPLE_RATES
        .get(usize::from(fscod))
        .copied()
        .ok_or_else(|| invalid_data("reserved AC-3 sample-rate code"))?;
    let frmsizecod =
        u8::try_from(bits.read(6)?).map_err(|_| invalid_data("invalid AC-3 frame-size code"))?;
    let bit_rate_code = frmsizecod >> 1;
    let bit_rate_kbps = BIT_RATES_KBPS
        .get(usize::from(bit_rate_code))
        .copied()
        .ok_or_else(|| invalid_data("reserved AC-3 frame-size code"))?;
    let bsid = u8::try_from(bits.read(5)?).map_err(|_| invalid_data("invalid AC-3 bsid"))?;
    if bsid > 8 {
        return Err(invalid_data("unsupported AC-3 bitstream identification"));
    }
    let bsmod = u8::try_from(bits.read(3)?).map_err(|_| invalid_data("invalid AC-3 bsmod"))?;
    let acmod = u8::try_from(bits.read(3)?).map_err(|_| invalid_data("invalid AC-3 acmod"))?;
    if acmod == 2 {
        bits.skip(2)?;
    } else {
        if (acmod & 1) != 0 && acmod != 1 {
            bits.skip(2)?;
        }
        if (acmod & 4) != 0 {
            bits.skip(2)?;
        }
    }
    let lfeon = bits.read(1)? != 0;

    let numerator = u64::from(bit_rate_kbps)
        .saturating_mul(1000)
        .saturating_mul(1536);
    let denominator = u64::from(sample_rate).saturating_mul(16);
    let mut frame_words = numerator / denominator;
    if numerator % denominator != 0 && (frmsizecod & 1) != 0 {
        frame_words = frame_words.saturating_add(1);
    }
    let frame_size = usize::try_from(frame_words.saturating_mul(2))
        .map_err(|_| invalid_data("AC-3 frame size exceeds platform limit"))?;
    if frame_size != data.len() {
        return Err(invalid_data(
            "AC-3 Matroska sample must contain exactly one complete syncframe",
        ));
    }

    Ok(ParsedAc3Frame {
        config: Ac3Config {
            fscod,
            bsid,
            bsmod,
            acmod,
            lfeon,
            bit_rate_code,
        },
        sample_rate,
        channels: dolby_channels(acmod, lfeon)?,
    })
}

fn parse_eac3_frame(data: &[u8]) -> io::Result<ParsedEac3Frame> {
    const SAMPLE_RATES: [u32; 3] = [48_000, 44_100, 32_000];

    if data.len() < 7 || data[0..2] != [0x0B, 0x77] {
        return Err(invalid_data("invalid E-AC-3 syncframe"));
    }
    let mut bits = BitReader::new(&data[2..]);
    let strmtyp = bits.read(2)?;
    if strmtyp != 0 {
        return Err(invalid_data(
            "E-AC-3 MP4 compatibility supports only independent substreams",
        ));
    }
    let substream_id = bits.read(3)?;
    if substream_id != 0 {
        return Err(invalid_data(
            "E-AC-3 MP4 compatibility supports only the primary independent substream",
        ));
    }
    let frame_size = usize::try_from((bits.read(11)? + 1).saturating_mul(2))
        .map_err(|_| invalid_data("E-AC-3 frame size exceeds platform limit"))?;
    if frame_size != data.len() {
        return Err(invalid_data(
            "E-AC-3 Matroska sample must contain exactly one complete syncframe",
        ));
    }

    let fscod =
        u8::try_from(bits.read(2)?).map_err(|_| invalid_data("invalid E-AC-3 sample-rate code"))?;
    if fscod == 3 {
        return Err(invalid_data(
            "reduced-rate E-AC-3 is left to native or external playback",
        ));
    }
    let sample_rate = SAMPLE_RATES
        .get(usize::from(fscod))
        .copied()
        .ok_or_else(|| invalid_data("reserved E-AC-3 sample-rate code"))?;
    let numblkscod = bits.read(2)?;
    let blocks_per_frame = match numblkscod {
        0 => 1,
        1 => 2,
        2 => 3,
        3 => 6,
        _ => return Err(invalid_data("invalid E-AC-3 block-count code")),
    };
    if blocks_per_frame != 6 {
        return Err(invalid_data(
            "E-AC-3 short-block frames require sample aggregation and are not remuxed",
        ));
    }

    let acmod = u8::try_from(bits.read(3)?).map_err(|_| invalid_data("invalid E-AC-3 acmod"))?;
    let lfeon = bits.read(1)? != 0;
    let bsid = u8::try_from(bits.read(5)?).map_err(|_| invalid_data("invalid E-AC-3 bsid"))?;
    if bsid != 16 {
        return Err(invalid_data("unsupported E-AC-3 bitstream identification"));
    }
    let (bsmod, complexity_index_type_a) =
        parse_eac3_optional_metadata(&mut bits, acmod, lfeon, blocks_per_frame)?;

    let frame_len = u64::try_from(data.len())
        .map_err(|_| invalid_data("E-AC-3 frame size exceeds platform limit"))?;
    let bitrate_numerator = frame_len
        .saturating_mul(8)
        .saturating_mul(u64::from(sample_rate));
    let bitrate_denominator = 1536_u64.saturating_mul(1000);
    let data_rate_kbps = bitrate_numerator / bitrate_denominator;
    let max_data_rate_kbps = u16::try_from(data_rate_kbps)
        .ok()
        .filter(|value| *value <= 8191)
        .ok_or_else(|| invalid_data("E-AC-3 data rate exceeds EC3SpecificBox limit"))?;

    Ok(ParsedEac3Frame {
        config: Eac3Config {
            fscod,
            bsid,
            bsmod,
            acmod,
            lfeon,
            max_data_rate_kbps,
            complexity_index_type_a,
        },
        sample_rate,
        channels: dolby_channels(acmod, lfeon)?,
    })
}

fn parse_eac3_optional_metadata(
    bits: &mut BitReader<'_>,
    acmod: u8,
    lfeon: bool,
    blocks_per_frame: u32,
) -> io::Result<(u8, Option<u8>)> {
    let programs = if acmod == 0 { 2 } else { 1 };
    for _ in 0..programs {
        bits.skip(5)?; // dialnorm
        if bits.read(1)? != 0 {
            bits.skip(8)?; // compr
        }
    }

    if bits.read(1)? != 0 {
        // Mixing metadata. This follows the independent-substream E-AC-3
        // syntax closely enough to reach informational/additional metadata
        // without interpreting fields Rustnote does not need.
        if acmod > 2 {
            bits.skip(2)?; // dmixmod
            if (acmod & 1) != 0 {
                bits.skip(6)?; // ltrtcmixlev + lorocmixlev
            }
            if (acmod & 4) != 0 {
                bits.skip(6)?; // ltrtsurmixlev + lorosurmixlev
            }
        }
        if lfeon && bits.read(1)? != 0 {
            bits.skip(5)?; // lfemixlevcod
        }
        for _ in 0..programs {
            if bits.read(1)? != 0 {
                bits.skip(6)?; // pgmscl
            }
        }
        if bits.read(1)? != 0 {
            bits.skip(6)?; // extpgmscl
        }
        match bits.read(2)? {
            0 => {}
            1 => bits.skip(5)?,
            2 => bits.skip(12)?,
            3 => {
                let bytes = usize::try_from(bits.read(5)? + 2)
                    .map_err(|_| invalid_data("E-AC-3 mixing metadata is too large"))?;
                bits.skip(
                    bytes
                        .checked_mul(8)
                        .ok_or_else(|| invalid_data("E-AC-3 mixing metadata is too large"))?,
                )?;
            }
            _ => return Err(invalid_data("invalid E-AC-3 mixing definition")),
        }
        if acmod < 2 {
            for _ in 0..programs {
                if bits.read(1)? != 0 {
                    bits.skip(14)?; // panmean + reserved paninfo bits
                }
            }
        }
        if bits.read(1)? != 0 {
            for _ in 0..blocks_per_frame {
                if blocks_per_frame == 1 || bits.read(1)? != 0 {
                    bits.skip(5)?; // blkmixcfginfo
                }
            }
        }
    }

    let bsmod = if bits.read(1)? != 0 {
        let value = u8::try_from(bits.read(3)?)
            .map_err(|_| invalid_data("invalid E-AC-3 bitstream mode"))?;
        bits.skip(2)?; // copyrightb + origbs
        if acmod == 2 {
            bits.skip(4)?; // dsurmod + dheadphonmod
        }
        if acmod >= 6 {
            bits.skip(2)?; // dsurexmod
        }
        for _ in 0..programs {
            if bits.read(1)? != 0 {
                bits.skip(8)?; // audprodie metadata
            }
        }
        bits.skip(1)?; // sourcefscod; reduced-rate streams are rejected earlier
        value
    } else {
        0
    };

    // Six-block independent frames have neither converter-sync nor original
    // AC-3 frame-size fields here. Preserve E-AC-3 extension-type-A signalling
    // in dec3 when present; other addbsi bytes stay in the elementary stream.
    let complexity_index_type_a = if bits.read(1)? != 0 {
        let additional_bytes = usize::try_from(bits.read(6)? + 1)
            .map_err(|_| invalid_data("E-AC-3 additional metadata is too large"))?;
        let first = u8::try_from(bits.read(8)?)
            .map_err(|_| invalid_data("invalid E-AC-3 additional metadata"))?;
        let extension_type_a = (first & 1) != 0;
        let complexity = if extension_type_a {
            if additional_bytes < 2 {
                return Err(invalid_data(
                    "E-AC-3 extension type A is missing its complexity index",
                ));
            }
            Some(
                u8::try_from(bits.read(8)?)
                    .map_err(|_| invalid_data("invalid E-AC-3 complexity index"))?,
            )
        } else {
            None
        };
        let consumed = if extension_type_a { 2 } else { 1 };
        if additional_bytes > consumed {
            bits.skip(
                (additional_bytes - consumed)
                    .checked_mul(8)
                    .ok_or_else(|| invalid_data("E-AC-3 additional metadata is too large"))?,
            )?;
        }
        complexity
    } else {
        None
    };

    Ok((bsmod, complexity_index_type_a))
}

fn aac_object_type(data: &[u8]) -> io::Result<u32> {
    let mut bits = BitReader::new(data);
    read_aac_object_type(&mut bits)
}

#[derive(Clone, Copy, Debug)]
struct AacConfigInfo {
    sample_rate: Option<u32>,
    channels: Option<u16>,
    frame_samples: u32,
}

fn parse_aac_specific_config(data: &[u8]) -> io::Result<AacConfigInfo> {
    if data.len() < 2 {
        return Err(invalid_data("AAC AudioSpecificConfig is too short"));
    }
    let mut bits = BitReader::new(data);
    let mut object_type = read_aac_object_type(&mut bits)?;
    let mut sample_rate = read_aac_sample_rate(&mut bits)?;
    let channels = bits.read(4).map(|value| value as u16)?;

    if matches!(object_type, 5 | 29) {
        sample_rate = read_aac_sample_rate(&mut bits)?;
        object_type = read_aac_object_type(&mut bits)?;
    }
    let frame_samples = if matches!(
        object_type,
        1 | 2 | 3 | 4 | 6 | 7 | 17 | 19 | 20 | 21 | 22 | 23
    ) && bits.remaining() >= 1
        && bits.read(1)? != 0
    {
        960
    } else {
        1024
    };

    Ok(AacConfigInfo {
        sample_rate,
        channels: (channels != 0).then_some(channels),
        frame_samples,
    })
}

fn read_aac_object_type(bits: &mut BitReader<'_>) -> io::Result<u32> {
    let value = bits.read(5)?;
    if value == 31 {
        Ok(32 + bits.read(6)?)
    } else {
        Ok(value)
    }
}

fn read_aac_sample_rate(bits: &mut BitReader<'_>) -> io::Result<Option<u32>> {
    const RATES: [u32; 13] = [
        96_000, 88_200, 64_000, 48_000, 44_100, 32_000, 24_000, 22_050, 16_000, 12_000, 11_025,
        8_000, 7_350,
    ];
    let index = bits.read(4)?;
    if index == 15 {
        let value = bits.read(24)?;
        return Ok((value > 0).then_some(value));
    }
    Ok(RATES.get(index as usize).copied())
}

struct BitReader<'a> {
    data: &'a [u8],
    bit: usize,
}

impl<'a> BitReader<'a> {
    const fn new(data: &'a [u8]) -> Self {
        Self { data, bit: 0 }
    }

    fn remaining(&self) -> usize {
        self.data.len().saturating_mul(8).saturating_sub(self.bit)
    }

    fn read(&mut self, count: usize) -> io::Result<u32> {
        if count > 32 || self.remaining() < count {
            return Err(invalid_data("truncated codec bitstream"));
        }
        let mut value = 0_u32;
        for _ in 0..count {
            let byte = self.data[self.bit / 8];
            let shift = 7 - (self.bit % 8);
            value = (value << 1) | u32::from((byte >> shift) & 1);
            self.bit += 1;
        }
        Ok(value)
    }

    fn skip(&mut self, count: usize) -> io::Result<()> {
        if self.remaining() < count {
            return Err(invalid_data("truncated codec bitstream"));
        }
        self.bit += count;
        Ok(())
    }
}

fn read_element_header(source: &mut File, parent_end: u64) -> io::Result<Option<ElementHeader>> {
    let start = source.stream_position()?;
    if start >= parent_end {
        return Ok(None);
    }
    let id = read_ebml_id_stream(source)?;
    let data_len = read_ebml_size_stream(source)?;
    let data_start = source.stream_position()?;
    if data_start > parent_end {
        return Err(invalid_data("EBML element header exceeds parent bounds"));
    }
    let data_end = match data_len {
        Some(length) => data_start
            .checked_add(length)
            .filter(|end| *end <= parent_end)
            .ok_or_else(|| invalid_data("EBML element exceeds parent bounds"))?,
        None => parent_end,
    };
    Ok(Some(ElementHeader {
        id,
        data_start,
        data_len,
        data_end,
    }))
}

fn read_ebml_id_stream(source: &mut File) -> io::Result<u32> {
    let mut first = [0_u8; 1];
    source.read_exact(&mut first)?;
    let len = vint_length(first[0], 4)?;
    let mut value = u64::from(first[0]);
    for _ in 1..len {
        let mut byte = [0_u8; 1];
        source.read_exact(&mut byte)?;
        value = (value << 8) | u64::from(byte[0]);
    }
    u32::try_from(value).map_err(|_| invalid_data("EBML element ID is too large"))
}

fn read_ebml_size_stream(source: &mut File) -> io::Result<Option<u64>> {
    let mut first = [0_u8; 1];
    source.read_exact(&mut first)?;
    let len = vint_length(first[0], 8)?;
    let marker = 1_u8 << (8 - len);
    let mut value = u64::from(first[0] & (marker - 1));
    for _ in 1..len {
        let mut byte = [0_u8; 1];
        source.read_exact(&mut byte)?;
        value = (value << 8) | u64::from(byte[0]);
    }
    let bits = 7 * len;
    let unknown = (1_u64 << bits) - 1;
    Ok((value != unknown).then_some(value))
}

fn parse_vint_from_slice(bytes: &[u8], is_id: bool) -> io::Result<(u64, usize)> {
    let first = *bytes
        .first()
        .ok_or_else(|| invalid_data("truncated EBML VINT"))?;
    let len = vint_length(first, if is_id { 4 } else { 8 })?;
    if bytes.len() < len {
        return Err(invalid_data("truncated EBML VINT"));
    }
    let mut value = if is_id {
        u64::from(first)
    } else {
        let marker = 1_u8 << (8 - len);
        u64::from(first & (marker - 1))
    };
    for byte in &bytes[1..len] {
        value = (value << 8) | u64::from(*byte);
    }
    Ok((value, len))
}

fn vint_length(first: u8, max_len: usize) -> io::Result<usize> {
    if first == 0 {
        return Err(invalid_data("invalid zero-prefixed EBML VINT"));
    }
    let len = first.leading_zeros() as usize + 1;
    if len == 0 || len > max_len {
        return Err(invalid_data("EBML VINT length exceeds supported limit"));
    }
    Ok(len)
}

fn require_known_size(header: ElementHeader, label: &str) -> io::Result<u64> {
    header
        .data_len
        .ok_or_else(|| invalid_data(format!("{label} has unsupported unknown size")))
}

fn read_uint(source: &mut File, header: ElementHeader) -> io::Result<u64> {
    let len = require_known_size(header, "integer element")?;
    if len == 0 || len > 8 {
        return Err(invalid_data("invalid EBML integer width"));
    }
    source.seek(SeekFrom::Start(header.data_start))?;
    let mut bytes = [0_u8; 8];
    let start = 8 - len as usize;
    source.read_exact(&mut bytes[start..])?;
    Ok(u64::from_be_bytes(bytes))
}

fn read_float(source: &mut File, header: ElementHeader) -> io::Result<f64> {
    let len = require_known_size(header, "float element")?;
    source.seek(SeekFrom::Start(header.data_start))?;
    match len {
        4 => {
            let mut bytes = [0_u8; 4];
            source.read_exact(&mut bytes)?;
            Ok(f32::from_bits(u32::from_be_bytes(bytes)) as f64)
        }
        8 => {
            let mut bytes = [0_u8; 8];
            source.read_exact(&mut bytes)?;
            Ok(f64::from_bits(u64::from_be_bytes(bytes)))
        }
        _ => Err(invalid_data("invalid EBML float width")),
    }
}

fn read_text(source: &mut File, header: ElementHeader, max: u64) -> io::Result<String> {
    let bytes = read_bytes(source, header, max)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| invalid_data("invalid UTF-8 EBML text"))?;
    Ok(text.trim_end_matches('\0').to_string())
}

fn read_bytes(source: &mut File, header: ElementHeader, max: u64) -> io::Result<Vec<u8>> {
    let len = require_known_size(header, "binary element")?;
    if len > max {
        return Err(invalid_data("EBML binary element exceeds safety limit"));
    }
    source.seek(SeekFrom::Start(header.data_start))?;
    let mut bytes =
        vec![0_u8; usize::try_from(len).map_err(|_| invalid_data("EBML element is too large"))?];
    source.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn read_span_bytes(source: &mut File, span: ElementSpan, max: u64) -> io::Result<Vec<u8>> {
    let len = span
        .data_end
        .checked_sub(span.data_start)
        .ok_or_else(|| invalid_data("invalid media span"))?;
    if len > max {
        return Err(invalid_data("media block exceeds safety limit"));
    }
    source.seek(SeekFrom::Start(span.data_start))?;
    let mut bytes =
        vec![0_u8; usize::try_from(len).map_err(|_| invalid_data("media block is too large"))?];
    source.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn ns_to_ticks(ns: u64, timescale: u32) -> u64 {
    let numerator = u128::from(ns) * u128::from(timescale);
    ((numerator + 500_000_000) / 1_000_000_000).min(u128::from(u64::MAX)) as u64
}

fn write_u32<W: Write>(writer: &mut W, value: u32) -> io::Result<()> {
    writer.write_all(&value.to_be_bytes())
}

fn write_u64<W: Write>(writer: &mut W, value: u64) -> io::Result<()> {
    writer.write_all(&value.to_be_bytes())
}

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matroska_unlaced_block_is_one_frame_range() {
        let frames = parse_lacing(&[0, 1, 2, 3, 4], 2, 0).expect("unlaced block should parse");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames.first(), Some(&(2..5)));
    }

    #[test]
    fn validates_av1_mp4_passthrough_records_and_samples() {
        assert!(validate_av1_codec_configuration_record(&[0x81, 0x00, 0x00, 0x00]).is_ok());
        assert!(
            validate_av1_codec_configuration_record(&[
                0x81, 0x00, 0x0C, 0x00, 0x0A, 0x0A, 0x00, 0x00, 0x00, 0x02, 0xAF, 0xFF, 0x9B, 0x5F,
                0x30, 0x08,
            ])
            .is_ok()
        );
        assert!(validate_av1_sample(&[0x30, 0x00]).is_ok());
        assert!(validate_av1_sample(&[0x32, 0x01, 0x00]).is_ok());

        let tile_list = validate_av1_sample(&[0x40, 0x00]);
        assert!(tile_list.is_err());

        let config_obu_without_size =
            validate_av1_codec_configuration_record(&[0x81, 0x00, 0x00, 0x00, 0x08, 0x00]);
        assert!(config_obu_without_size.is_err());
    }

    #[test]
    fn av1_mp4_rejects_reordered_presentation_timestamps() {
        let mut track = OutputTrack {
            track_id: 1,
            source_number: 1,
            kind: TrackKind::Video,
            codec: CodecConfig::Av1 {
                private: vec![0x81, 0x00, 0x00, 0x00],
            },
            width: 1920,
            height: 1080,
            sample_rate: 0,
            channels: 0,
            default_duration_ns: None,
            samples: vec![
                SampleMeta {
                    offset: 0,
                    size: 2,
                    pts_ns: 0,
                    explicit_duration_ns: None,
                    keyframe: true,
                },
                SampleMeta {
                    offset: 2,
                    size: 2,
                    pts_ns: 0,
                    explicit_duration_ns: None,
                    keyframe: false,
                },
            ],
            timed_samples: Vec::new(),
            timescale: VIDEO_TIMESCALE,
            media_duration: 0,
        };
        let result = finalize_track_timing(&mut track, 0, 33_333_333);
        assert!(result.is_err());
    }

    #[test]
    fn parses_conservative_ac3_mp4_sample() {
        let mut frame = vec![0_u8; 768];
        frame[..7].copy_from_slice(&[0x0B, 0x77, 0x00, 0x00, 0x14, 0x40, 0x40]);
        let parsed = match parse_ac3_frame(&frame) {
            Ok(parsed) => parsed,
            Err(error) => panic!("valid AC-3 frame rejected: {error}"),
        };
        assert_eq!(parsed.sample_rate, 48_000);
        assert_eq!(parsed.channels, 2);
        assert_eq!(parsed.config.bsid, 8);
        assert_eq!(parsed.config.bit_rate_code, 10);
        assert_eq!(build_dac3_body(parsed.config), vec![0x10, 0x11, 0x40]);
    }

    #[test]
    fn ac3_specific_box_matches_a_real_mono_48khz_192kbps_frame() {
        let mut frame = vec![0_u8; 768];
        frame[..7].copy_from_slice(&[0x0B, 0x77, 0x32, 0xB5, 0x14, 0x40, 0x2F]);
        let parsed = match parse_ac3_frame(&frame) {
            Ok(parsed) => parsed,
            Err(error) => panic!("valid AC-3 frame rejected: {error}"),
        };
        assert_eq!(parsed.sample_rate, 48_000);
        assert_eq!(parsed.channels, 1);
        assert_eq!(build_dac3_body(parsed.config), vec![0x10, 0x09, 0x40]);
    }

    #[test]
    fn parses_conservative_eac3_mp4_sample() {
        let mut frame = vec![0_u8; 768];
        frame[..6].copy_from_slice(&[0x0B, 0x77, 0x01, 0x7F, 0x34, 0x80]);
        let parsed = match parse_eac3_frame(&frame) {
            Ok(parsed) => parsed,
            Err(error) => panic!("valid E-AC-3 frame rejected: {error}"),
        };
        assert_eq!(parsed.sample_rate, 48_000);
        assert_eq!(parsed.channels, 2);
        assert_eq!(parsed.config.bsid, 16);
        assert_eq!(parsed.config.max_data_rate_kbps, 192);
        assert_eq!(
            build_dec3_body(parsed.config),
            vec![0x06, 0x00, 0x20, 0x04, 0x00]
        );
    }

    #[test]
    fn eac3_specific_box_matches_a_real_mono_48khz_192kbps_frame() {
        let mut frame = vec![0_u8; 768];
        frame[..16].copy_from_slice(&[
            0x0B, 0x77, 0x01, 0x7F, 0x32, 0x87, 0xC0, 0x00, 0x21, 0x07, 0xC8, 0x60, 0xE4, 0xEE,
            0xC3, 0x86,
        ]);
        let parsed = match parse_eac3_frame(&frame) {
            Ok(parsed) => parsed,
            Err(error) => panic!("valid E-AC-3 frame rejected: {error}"),
        };
        assert_eq!(parsed.sample_rate, 48_000);
        assert_eq!(parsed.channels, 1);
        assert_eq!(
            build_dec3_body(parsed.config),
            vec![0x06, 0x00, 0x20, 0x02, 0x00]
        );
    }

    #[test]
    fn eac3_dec3_preserves_bitstream_mode_and_extension_type_a() {
        let body = build_dec3_body(Eac3Config {
            fscod: 0,
            bsid: 16,
            bsmod: 5,
            acmod: 2,
            lfeon: false,
            max_data_rate_kbps: 192,
            complexity_index_type_a: Some(42),
        });
        assert_eq!(body, vec![0x06, 0x00, 0x20, 0x54, 0x00, 0x01, 0x2A]);
    }

    #[test]
    fn dolby_audio_sample_entry_uses_fixed_channel_count() {
        let entry = match build_dolby_audio_sample_entry(
            *b"ec-3",
            *b"dec3",
            vec![0x06, 0x00, 0x20, 0x04, 0x00],
            48_000,
        ) {
            Ok(entry) => entry,
            Err(error) => panic!("valid EC-3 sample entry rejected: {error}"),
        };
        assert_eq!(&entry[24..26], &2_u16.to_be_bytes());
        assert_eq!(&entry[26..28], &16_u16.to_be_bytes());
    }

    #[test]
    fn rejects_short_block_eac3_for_mp4_passthrough() {
        let mut frame = vec![0_u8; 768];
        frame[..6].copy_from_slice(&[0x0B, 0x77, 0x01, 0x7F, 0x04, 0x80]);
        match parse_eac3_frame(&frame) {
            Ok(_) => panic!("short-block E-AC-3 was accepted"),
            Err(error) => assert!(error.to_string().contains("short-block")),
        }
    }
}
