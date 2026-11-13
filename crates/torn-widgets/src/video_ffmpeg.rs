//! `FFmpeg` command-line decoding for [`crate::Video`].
//!
//! The backend drives the `ffmpeg` and `ffprobe` executables rather than
//! linking against `libav*`. That keeps the toolkit free of native build
//! dependencies while still supporting every container and codec the installed
//! `FFmpeg` build understands. Frames are decoded to RGBA on a worker thread and
//! handed to the widget as [`RasterImage`] values; the newest frame wins so a
//! slow UI thread never builds an unbounded backlog.

use std::ffi::{OsStr, OsString};
use std::io::{ErrorKind, Read};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use torn_render::RasterImage;

const DEFAULT_FRAME_RATE: u32 = 30;
const DEFAULT_MAX_DIMENSION: u32 = 1280;
const FRAME_POLL_INTERVAL: Duration = Duration::from_millis(8);

/// Settings for the `FFmpeg` command-line decoder.
///
/// The defaults read `TORN_FFMPEG` and `TORN_FFPROBE` from the environment and
/// fall back to `ffmpeg` and a matching `ffprobe` next to it. Applications can
/// ship `FFmpeg` alongside their executable and point [`Self::with_ffmpeg`] at
/// that copy instead of relying on `PATH`.
#[derive(Clone, Debug)]
pub struct FfmpegConfig {
    ffmpeg: OsString,
    ffprobe: OsString,
    frame_rate: u32,
    max_dimension: Option<u32>,
}

impl FfmpegConfig {
    /// Creates settings from the environment, with the usual defaults.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the `ffmpeg` executable used for decoding.
    ///
    /// Unless [`Self::with_ffprobe`] is called afterwards, the matching
    /// `ffprobe` executable is derived from the same path.
    #[must_use]
    pub fn with_ffmpeg(mut self, program: impl Into<OsString>) -> Self {
        self.ffmpeg = program.into();
        self.ffprobe = default_ffprobe(&self.ffmpeg);
        self
    }

    /// Sets the `ffprobe` executable used to read media metadata.
    #[must_use]
    pub fn with_ffprobe(mut self, program: impl Into<OsString>) -> Self {
        self.ffprobe = program.into();
        self
    }

    /// Sets the output frame rate, clamped to at least one frame per second.
    ///
    /// The default is 30. Lower rates reduce decoding and allocation cost.
    #[must_use]
    pub fn with_frame_rate(mut self, frames_per_second: u32) -> Self {
        self.frame_rate = frames_per_second.max(1);
        self
    }

    /// Limits the longest decoded frame dimension.
    ///
    /// Frames larger than this are scaled down proportionally before they are
    /// transferred to the UI thread. `None` keeps the source resolution. The
    /// default is 1280 pixels.
    #[must_use]
    pub fn with_max_dimension(mut self, max_dimension: Option<u32>) -> Self {
        self.max_dimension = max_dimension.filter(|dimension| *dimension > 0);
        self
    }

    /// Returns the configured `ffmpeg` executable.
    #[must_use]
    pub fn ffmpeg(&self) -> &OsStr {
        &self.ffmpeg
    }

    /// Returns the configured `ffprobe` executable.
    #[must_use]
    pub fn ffprobe(&self) -> &OsStr {
        &self.ffprobe
    }

    /// Returns the configured output frame rate.
    #[must_use]
    pub const fn frame_rate(&self) -> u32 {
        self.frame_rate
    }

    /// Returns the configured maximum decoded frame dimension.
    #[must_use]
    pub const fn max_dimension(&self) -> Option<u32> {
        self.max_dimension
    }
}

impl Default for FfmpegConfig {
    fn default() -> Self {
        let ffmpeg = std::env::var_os("TORN_FFMPEG").unwrap_or_else(|| OsString::from("ffmpeg"));
        let ffprobe = std::env::var_os("TORN_FFPROBE").unwrap_or_else(|| default_ffprobe(&ffmpeg));
        Self {
            ffmpeg,
            ffprobe,
            frame_rate: DEFAULT_FRAME_RATE,
            max_dimension: Some(DEFAULT_MAX_DIMENSION),
        }
    }
}

fn default_ffprobe(ffmpeg: &OsStr) -> OsString {
    let path = Path::new(ffmpeg);
    let Some(name) = path.file_name().and_then(OsStr::to_str) else {
        return OsString::from("ffprobe");
    };
    let lower = name.to_ascii_lowercase();
    let Some(index) = lower.find("ffmpeg") else {
        return OsString::from("ffprobe");
    };
    let mut probe = name.to_owned();
    probe.replace_range(index..index + "ffmpeg".len(), "ffprobe");
    path.with_file_name(probe).into_os_string()
}

/// Metadata read from the media container before decoding starts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VideoMetadata {
    width: u32,
    height: u32,
    duration: Duration,
}

impl VideoMetadata {
    /// Returns the source video width in pixels.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Returns the source video height in pixels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Returns the container duration, or zero when it is unknown.
    #[must_use]
    pub const fn duration(&self) -> Duration {
        self.duration
    }
}

/// Why opening or decoding a video failed.
#[derive(Debug)]
pub enum VideoError {
    /// The `ffmpeg` executable could not be started.
    FfmpegNotFound {
        /// The executable that was requested.
        program: OsString,
    },
    /// The `ffprobe` executable could not be started.
    FfprobeNotFound {
        /// The executable that was requested.
        program: OsString,
    },
    /// `ffprobe` ran but rejected the file.
    ProbeFailed {
        /// The executable that was requested.
        program: OsString,
        /// Diagnostic text written by `ffprobe`.
        details: String,
    },
    /// `ffprobe` succeeded but did not report a usable video stream.
    InvalidMetadata(String),
    /// Starting `FFmpeg`, probing the file, or creating a worker failed.
    Io(std::io::Error),
}

impl std::fmt::Display for VideoError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FfmpegNotFound { program } => write!(
                formatter,
                "the ffmpeg executable {} was not found; install FFmpeg or set TORN_FFMPEG",
                program.to_string_lossy()
            ),
            Self::FfprobeNotFound { program } => write!(
                formatter,
                "the ffprobe executable {} was not found; install FFmpeg or set TORN_FFPROBE",
                program.to_string_lossy()
            ),
            Self::ProbeFailed { program, details } => {
                write!(
                    formatter,
                    "ffprobe {} failed: {details}",
                    program.to_string_lossy()
                )
            }
            Self::InvalidMetadata(message) => {
                write!(formatter, "unsupported video metadata: {message}")
            }
            Self::Io(error) => write!(formatter, "video backend I/O failed: {error}"),
        }
    }
}

impl std::error::Error for VideoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

fn probe_metadata(path: &Path, config: &FfmpegConfig) -> Result<VideoMetadata, VideoError> {
    let output = Command::new(&config.ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-show_entries",
            "stream_tags=rotate",
            "-show_entries",
            "stream_side_data=rotation",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1",
        ])
        .arg(path)
        .output()
        .map_err(|error| {
            if error.kind() == ErrorKind::NotFound {
                VideoError::FfprobeNotFound {
                    program: config.ffprobe.clone(),
                }
            } else {
                VideoError::Io(error)
            }
        })?;
    if !output.status.success() {
        return Err(VideoError::ProbeFailed {
            program: config.ffprobe.clone(),
            details: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    parse_probe_output(&String::from_utf8_lossy(&output.stdout))
}

fn check_ffmpeg(config: &FfmpegConfig) -> Result<(), VideoError> {
    let status = Command::new(&config.ffmpeg)
        .args(["-hide_banner", "-version"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| {
            if error.kind() == ErrorKind::NotFound {
                VideoError::FfmpegNotFound {
                    program: config.ffmpeg.clone(),
                }
            } else {
                VideoError::Io(error)
            }
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(VideoError::Io(std::io::Error::other(format!(
            "ffmpeg {} exited with {status}",
            config.ffmpeg.to_string_lossy()
        ))))
    }
}

fn parse_probe_output(text: &str) -> Result<VideoMetadata, VideoError> {
    let mut width = None;
    let mut height = None;
    let mut rotation = 0_i32;
    let mut duration = Duration::ZERO;
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "width" => width = value.parse::<u32>().ok(),
            "height" => height = value.parse::<u32>().ok(),
            "rotate" | "rotation" => rotation = value.parse::<i32>().unwrap_or(0),
            "duration" => {
                if let Ok(seconds) = value.parse::<f64>()
                    && seconds.is_finite()
                    && seconds >= 0.0
                {
                    duration = Duration::from_secs_f64(seconds);
                }
            }
            _ => {}
        }
    }
    let width = width.filter(|value| *value > 0).ok_or_else(|| {
        VideoError::InvalidMetadata("the file has no decodable video stream".to_owned())
    })?;
    let height = height
        .filter(|value| *value > 0)
        .ok_or_else(|| VideoError::InvalidMetadata("the video stream has no height".to_owned()))?;
    if rotation.rem_euclid(180) != 0 {
        // FFmpeg applies the display matrix before user filters, so scale and
        // decode using the rotated display dimensions.
        return Ok(VideoMetadata {
            width: height,
            height: width,
            duration,
        });
    }
    Ok(VideoMetadata {
        width,
        height,
        duration,
    })
}

fn scaled_size(width: u32, height: u32, max_dimension: Option<u32>) -> (u32, u32) {
    let Some(max) = max_dimension else {
        return (width, height);
    };
    let longest = width.max(height);
    if longest <= max {
        return (width, height);
    }
    (
        scaled_dimension(width, longest, max),
        scaled_dimension(height, longest, max),
    )
}

fn scaled_dimension(value: u32, longest: u32, max: u32) -> u32 {
    let rounded = (u64::from(value) * u64::from(max) + u64::from(longest) / 2) / u64::from(longest);
    u32::try_from(rounded.max(1)).unwrap_or(value).max(1)
}

fn frame_command(path: &Path, config: &FfmpegConfig, size: (u32, u32), start: Duration) -> Command {
    let (width, height) = size;
    let mut command = Command::new(&config.ffmpeg);
    command
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-nostdin")
        .arg("-ss")
        .arg(format!("{:.3}", start.as_secs_f64()))
        .arg("-i")
        .arg(path)
        .arg("-map")
        .arg("0:v:0")
        .arg("-an")
        .arg("-sn")
        .arg("-vf")
        .arg(format!("scale={width}:{height},fps={}", config.frame_rate))
        .arg("-f")
        .arg("rawvideo")
        .arg("-pix_fmt")
        .arg("rgba")
        .arg("pipe:1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    command
}

#[derive(Debug)]
enum DecoderCommand {
    PlayAt(Duration),
    Pause,
    Seek { position: Duration, playing: bool },
    Preview(Duration),
    Stop,
}

struct Shared {
    frame: Mutex<Option<RasterImage>>,
    eof: AtomicBool,
    error: Mutex<Option<String>>,
    busy: AtomicBool,
}

impl Shared {
    fn new() -> Self {
        Self {
            frame: Mutex::new(None),
            eof: AtomicBool::new(false),
            error: Mutex::new(None),
            busy: AtomicBool::new(false),
        }
    }

    fn store_frame(&self, frame: RasterImage) {
        if let Ok(mut slot) = self.frame.lock() {
            *slot = Some(frame);
        }
    }

    fn take_frame(&self) -> Option<RasterImage> {
        self.frame.lock().ok().and_then(|mut slot| slot.take())
    }

    fn store_error(&self, message: impl Into<String>) {
        if let Ok(mut slot) = self.error.lock() {
            *slot = Some(message.into());
        }
    }

    fn take_error(&self) -> Option<String> {
        self.error.lock().ok().and_then(|mut slot| slot.take())
    }
}

struct Session {
    child: Child,
    frames: Receiver<Result<RasterImage, String>>,
    eof: Arc<AtomicBool>,
    reader: JoinHandle<()>,
    preview: bool,
}

impl Session {
    fn finished(&self) -> bool {
        self.eof.load(Ordering::Acquire)
    }

    fn stop(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = self.reader.join();
    }
}

/// A running `FFmpeg` decoder with its own worker thread.
pub(crate) struct FfmpegDecoder {
    commands: Sender<DecoderCommand>,
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}

impl FfmpegDecoder {
    pub(crate) fn open(
        path: &Path,
        config: FfmpegConfig,
    ) -> Result<(Self, VideoMetadata), VideoError> {
        let metadata = probe_metadata(path, &config)?;
        check_ffmpeg(&config)?;
        let size = scaled_size(metadata.width, metadata.height, config.max_dimension);
        let shared = Arc::new(Shared::new());
        let (commands, receiver) = mpsc::channel();
        let thread_shared = Arc::clone(&shared);
        let thread_path = path.to_path_buf();
        let thread_config = config;
        let worker = thread::Builder::new()
            .name("torn-video-decoder".to_owned())
            .spawn(move || {
                run_decoder(
                    &receiver,
                    &thread_shared,
                    &thread_path,
                    &thread_config,
                    size,
                );
            })
            .map_err(VideoError::Io)?;
        let decoder = Self {
            commands,
            shared,
            worker: Some(worker),
        };
        decoder.preview(Duration::ZERO);
        Ok((decoder, metadata))
    }

    pub(crate) fn play_at(&self, position: Duration) {
        self.send(DecoderCommand::PlayAt(position));
    }

    pub(crate) fn pause(&self) {
        self.send(DecoderCommand::Pause);
    }

    pub(crate) fn seek(&self, position: Duration, playing: bool) {
        self.send(DecoderCommand::Seek { position, playing });
    }

    pub(crate) fn preview(&self, position: Duration) {
        self.send(DecoderCommand::Preview(position));
    }

    pub(crate) fn is_busy(&self) -> bool {
        self.shared.busy.load(Ordering::Acquire)
    }

    pub(crate) fn has_pending_frame(&self) -> bool {
        self.shared.frame.lock().is_ok_and(|slot| slot.is_some())
    }

    pub(crate) fn take_frame(&self) -> Option<RasterImage> {
        self.shared.take_frame()
    }

    pub(crate) fn take_eof(&self) -> bool {
        self.shared.eof.swap(false, Ordering::AcqRel)
    }

    pub(crate) fn take_error(&self) -> Option<String> {
        self.shared.take_error()
    }

    fn send(&self, command: DecoderCommand) {
        let busy = !matches!(command, DecoderCommand::Pause | DecoderCommand::Stop);
        if self.commands.send(command).is_err() {
            self.shared
                .store_error("the video decoder stopped unexpectedly");
            self.shared.busy.store(false, Ordering::Release);
            self.shared.eof.store(true, Ordering::Release);
        } else if busy {
            self.shared.busy.store(true, Ordering::Release);
        } else {
            self.shared.busy.store(false, Ordering::Release);
        }
    }
}

impl Drop for FfmpegDecoder {
    fn drop(&mut self) {
        let _ = self.commands.send(DecoderCommand::Stop);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn run_decoder(
    commands: &Receiver<DecoderCommand>,
    shared: &Shared,
    path: &Path,
    config: &FfmpegConfig,
    size: (u32, u32),
) {
    let mut session: Option<Session> = None;
    loop {
        if let Some(active) = session.as_mut() {
            let received = drain_frames(active, shared);
            if active.preview && received {
                if let Some(active) = session.take() {
                    active.stop();
                }
                shared.busy.store(false, Ordering::Release);
            } else if active.finished() {
                let preview = active.preview;
                if let Some(active) = session.take() {
                    active.stop();
                }
                if !preview {
                    shared.eof.store(true, Ordering::Release);
                }
                shared.busy.store(false, Ordering::Release);
            }
        }

        let command = if session.is_some() {
            match commands.recv_timeout(FRAME_POLL_INTERVAL) {
                Ok(command) => Some(command),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => break,
            }
        } else {
            match commands.recv() {
                Ok(command) => Some(command),
                Err(_) => break,
            }
        };
        let Some(command) = command else {
            continue;
        };
        match command {
            DecoderCommand::Stop => break,
            DecoderCommand::Pause => {
                stop_session(&mut session);
                shared.busy.store(false, Ordering::Release);
            }
            DecoderCommand::PlayAt(position) => {
                stop_session(&mut session);
                start_or_report(&mut session, shared, path, config, size, position, false);
            }
            DecoderCommand::Seek { position, playing } => {
                stop_session(&mut session);
                start_or_report(&mut session, shared, path, config, size, position, !playing);
            }
            DecoderCommand::Preview(position) => {
                stop_session(&mut session);
                start_or_report(&mut session, shared, path, config, size, position, true);
            }
        }
    }
    stop_session(&mut session);
    shared.busy.store(false, Ordering::Release);
}

fn drain_frames(session: &mut Session, shared: &Shared) -> bool {
    let mut received = false;
    loop {
        match session.frames.try_recv() {
            Ok(Ok(frame)) => {
                shared.store_frame(frame);
                received = true;
            }
            Ok(Err(message)) => shared.store_error(message),
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
        }
    }
    received
}

fn stop_session(session: &mut Option<Session>) {
    if let Some(active) = session.take() {
        active.stop();
    }
}

fn start_or_report(
    session: &mut Option<Session>,
    shared: &Shared,
    path: &Path,
    config: &FfmpegConfig,
    size: (u32, u32),
    position: Duration,
    preview: bool,
) {
    match start_session(path, config, size, position, preview) {
        Ok(started) => {
            shared.busy.store(true, Ordering::Release);
            *session = Some(started);
        }
        Err(message) => {
            shared.store_error(message);
            shared.busy.store(false, Ordering::Release);
            shared.eof.store(true, Ordering::Release);
        }
    }
}

fn start_session(
    path: &Path,
    config: &FfmpegConfig,
    size: (u32, u32),
    position: Duration,
    preview: bool,
) -> Result<Session, String> {
    let mut child = frame_command(path, config, size, position)
        .spawn()
        .map_err(|error| {
            if error.kind() == ErrorKind::NotFound {
                format!(
                    "the ffmpeg executable {} was not found; install FFmpeg or set TORN_FFMPEG",
                    config.ffmpeg.to_string_lossy()
                )
            } else {
                format!("could not start ffmpeg: {error}")
            }
        })?;
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err("ffmpeg did not expose a frame pipe".to_owned());
    };
    let frame_bytes = frame_byte_count(size)?;
    let (frames, sender_eof, reader) = spawn_frame_reader(stdout, size, frame_bytes);
    Ok(Session {
        child,
        frames,
        eof: sender_eof,
        reader,
        preview,
    })
}

fn frame_byte_count(size: (u32, u32)) -> Result<usize, String> {
    let pixels = u64::from(size.0)
        .checked_mul(u64::from(size.1))
        .and_then(|count| count.checked_mul(4))
        .ok_or_else(|| "decoded frame is too large".to_owned())?;
    usize::try_from(pixels).map_err(|_| "decoded frame is too large".to_owned())
}

fn spawn_frame_reader(
    mut stdout: impl Read + Send + 'static,
    size: (u32, u32),
    frame_bytes: usize,
) -> (
    Receiver<Result<RasterImage, String>>,
    Arc<AtomicBool>,
    JoinHandle<()>,
) {
    let (sender, receiver) = mpsc::channel();
    let eof = Arc::new(AtomicBool::new(false));
    let reader_eof = Arc::clone(&eof);
    let reader = thread::spawn(move || {
        loop {
            let mut pixels = vec![0; frame_bytes];
            match stdout.read_exact(&mut pixels) {
                Ok(()) => match RasterImage::new(size.0, size.1, pixels) {
                    Ok(frame) => {
                        if sender.send(Ok(frame)).is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = sender.send(Err(format!("decoded frame is invalid: {error}")));
                        reader_eof.store(true, Ordering::Release);
                        break;
                    }
                },
                Err(error) if error.kind() == ErrorKind::UnexpectedEof => {
                    reader_eof.store(true, Ordering::Release);
                    break;
                }
                Err(error) => {
                    let _ = sender.send(Err(format!("reading ffmpeg output failed: {error}")));
                    reader_eof.store(true, Ordering::Release);
                    break;
                }
            }
        }
    });
    (receiver, eof, reader)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_probe_output() {
        let metadata = parse_probe_output("width=640\nheight=360\nduration=12.500000\n")
            .expect("metadata is valid");
        assert_eq!(metadata.width(), 640);
        assert_eq!(metadata.height(), 360);
        assert_eq!(metadata.duration(), Duration::from_millis(12_500));
    }

    #[test]
    fn swaps_dimensions_for_rotated_videos() {
        let metadata = parse_probe_output("width=1920\nheight=1080\nrotation=-90\n")
            .expect("metadata is valid");
        assert_eq!(metadata.width(), 1080);
        assert_eq!(metadata.height(), 1920);
    }

    #[test]
    fn treats_unknown_duration_as_zero() {
        let metadata =
            parse_probe_output("width=2\nheight=2\nduration=N/A\n").expect("metadata is valid");
        assert_eq!(metadata.duration(), Duration::ZERO);
    }

    #[test]
    fn rejects_files_without_a_video_stream() {
        assert!(matches!(
            parse_probe_output("duration=3.0\n"),
            Err(VideoError::InvalidMetadata(_))
        ));
    }

    #[test]
    fn scales_down_preserving_aspect_ratio() {
        assert_eq!(scaled_size(1920, 1080, Some(1280)), (1280, 720));
        assert_eq!(scaled_size(1080, 1920, Some(1280)), (720, 1280));
        assert_eq!(scaled_size(320, 240, Some(1280)), (320, 240));
        assert_eq!(scaled_size(1920, 1080, None), (1920, 1080));
    }

    #[test]
    fn derives_ffprobe_next_to_ffmpeg() {
        assert_eq!(
            default_ffprobe(OsStr::new("ffmpeg")),
            OsString::from("ffprobe")
        );
        assert_eq!(
            Path::new(&default_ffprobe(OsStr::new("C:/tools/ffmpeg.exe"))),
            Path::new("C:/tools/ffprobe.exe")
        );
    }

    #[test]
    fn requests_rgba_raw_frames_from_the_seek_position() {
        let config = FfmpegConfig::new()
            .with_ffmpeg("ffmpeg")
            .with_frame_rate(24);
        let command = frame_command(
            Path::new("clip.mp4"),
            &config,
            (640, 360),
            Duration::from_millis(1500),
        );
        let args: Vec<String> = command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect();
        assert!(
            args.windows(2)
                .any(|pair| pair[0] == "-ss" && pair[1] == "1.500")
        );
        assert!(
            args.windows(2)
                .any(|pair| pair[0] == "-vf" && pair[1] == "scale=640:360,fps=24")
        );
        assert!(
            args.windows(2)
                .any(|pair| pair[0] == "-pix_fmt" && pair[1] == "rgba")
        );
        assert_eq!(args.last().map(String::as_str), Some("pipe:1"));
    }
}
