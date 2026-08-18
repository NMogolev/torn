use std::{
    fmt,
    sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    thread::{self, JoinHandle},
};

use torn_render::DisplayList;

use crate::{PixelBuffer, PixelBufferError, RenderError, SoftwareRenderer};

const FRAME_QUEUE_CAPACITY: usize = 1;

/// A completed frame returned by [`SoftwareRenderWorker`].
#[derive(Debug)]
pub struct SoftwareRenderResult {
    frame_id: u64,
    result: Result<PixelBuffer, SoftwareRenderError>,
}

impl SoftwareRenderResult {
    /// Returns the caller-supplied identifier of the completed frame.
    #[must_use]
    pub const fn frame_id(&self) -> u64 {
        self.frame_id
    }

    /// Returns the rendered pixels or the error that prevented rendering them.
    ///
    /// # Errors
    ///
    /// Returns the target-allocation or display-list rendering error reported by
    /// the worker.
    pub fn into_result(self) -> Result<PixelBuffer, SoftwareRenderError> {
        self.result
    }
}

/// Why a submitted frame could not be rendered.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SoftwareRenderError {
    /// Allocating the requested target buffer failed.
    PixelBuffer(PixelBufferError),
    /// The display list could not be executed.
    Render(RenderError),
}

impl fmt::Display for SoftwareRenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PixelBuffer(error) => {
                write!(formatter, "could not allocate render target: {error}")
            }
            Self::Render(error) => write!(formatter, "could not render display list: {error}"),
        }
    }
}

impl std::error::Error for SoftwareRenderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::PixelBuffer(error) => Some(error),
            Self::Render(error) => Some(error),
        }
    }
}

/// Why [`SoftwareRenderWorker::try_submit`] could not accept a frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubmitError {
    /// The worker is rendering a frame and one newer frame is already queued.
    QueueFull,
    /// The worker has stopped and cannot accept more work.
    Stopped,
}

impl fmt::Display for SubmitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::QueueFull => "software render queue is full",
            Self::Stopped => "software render worker has stopped",
        })
    }
}

impl std::error::Error for SubmitError {}

/// A failure to receive a completed frame from [`SoftwareRenderWorker`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReceiveError {
    /// The worker stopped before it could return another frame.
    Stopped,
}

impl fmt::Display for ReceiveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("software render worker has stopped")
    }
}

impl std::error::Error for ReceiveError {}

/// Executes complete display lists on a dedicated software-rendering thread.
///
/// The worker accepts at most one queued frame in addition to the frame being
/// rendered. Callers should treat [`SubmitError::QueueFull`] as a signal to drop
/// the stale frame instead of blocking the UI thread. Widget state, window
/// handles, and frame presentation remain owned by their originating threads;
/// only the transferable [`DisplayList`] and [`PixelBuffer`] cross this boundary.
pub struct SoftwareRenderWorker {
    requests: Option<SyncSender<RenderRequest>>,
    results: Option<Receiver<SoftwareRenderResult>>,
    thread: Option<JoinHandle<()>>,
}

impl SoftwareRenderWorker {
    /// Starts a dedicated worker thread.
    ///
    /// # Errors
    ///
    /// Returns the operating-system error if the worker thread cannot be
    /// created.
    pub fn spawn() -> std::io::Result<Self> {
        Self::spawn_with_result_notifier(|| {})
    }

    /// Starts a dedicated worker thread and notifies `on_result` after each
    /// completed frame is available to receive.
    ///
    /// The callback runs on the rendering thread. It should therefore only
    /// perform a lightweight wake-up operation, such as sending a native event
    /// to an event loop. Completed frames must still be retrieved with
    /// [`Self::try_receive`] or [`Self::receive`].
    ///
    /// # Errors
    ///
    /// Returns the operating-system error if the worker thread cannot be
    /// created.
    pub fn spawn_with_result_notifier(
        on_result: impl Fn() + Send + 'static,
    ) -> std::io::Result<Self> {
        let (request_sender, request_receiver) = mpsc::sync_channel(FRAME_QUEUE_CAPACITY);
        let (result_sender, result_receiver) = mpsc::sync_channel(FRAME_QUEUE_CAPACITY);
        let thread = thread::Builder::new()
            .name("torn-software-render".into())
            .spawn(move || render_frames(request_receiver, result_sender, on_result))?;

        Ok(Self {
            requests: Some(request_sender),
            results: Some(result_receiver),
            thread: Some(thread),
        })
    }

    /// Queues a display list for asynchronous rendering without blocking.
    ///
    /// `frame_id` is returned unchanged with the corresponding
    /// [`SoftwareRenderResult`]. When the queue is full, ownership of
    /// `display_list` is intentionally released so callers can discard an
    /// obsolete frame cheaply.
    ///
    /// # Errors
    ///
    /// Returns [`SubmitError::QueueFull`] if the worker cannot begin this frame
    /// soon, or [`SubmitError::Stopped`] after the worker has exited.
    pub fn try_submit(
        &self,
        frame_id: u64,
        display_list: DisplayList,
        width: u32,
        height: u32,
    ) -> Result<(), SubmitError> {
        self.try_submit_with_scale_factor(frame_id, display_list, width, height, 1.0)
    }

    /// Queues a display list for rendering into a physical-pixel target.
    ///
    /// Display-list geometry remains in logical pixels; `scale_factor` maps it
    /// to the supplied physical `width` and `height` in the worker thread.
    ///
    /// # Errors
    ///
    /// Returns [`SubmitError::QueueFull`] if the worker cannot begin this frame
    /// soon, or [`SubmitError::Stopped`] after the worker has exited.
    pub fn try_submit_with_scale_factor(
        &self,
        frame_id: u64,
        display_list: DisplayList,
        width: u32,
        height: u32,
        scale_factor: f32,
    ) -> Result<(), SubmitError> {
        let request = RenderRequest {
            frame_id,
            display_list,
            width,
            height,
            scale_factor,
        };
        let Some(requests) = &self.requests else {
            return Err(SubmitError::Stopped);
        };

        match requests.try_send(request) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err(SubmitError::QueueFull),
            Err(TrySendError::Disconnected(_)) => Err(SubmitError::Stopped),
        }
    }

    /// Returns a completed frame if one is immediately available.
    ///
    /// # Errors
    ///
    /// Returns [`ReceiveError::Stopped`] when no further completed frames can
    /// arrive because the worker has exited.
    pub fn try_receive(&self) -> Result<Option<SoftwareRenderResult>, ReceiveError> {
        let Some(results) = &self.results else {
            return Err(ReceiveError::Stopped);
        };

        match results.try_recv() {
            Ok(result) => Ok(Some(result)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(ReceiveError::Stopped),
        }
    }

    /// Waits for the next completed frame.
    ///
    /// This is appropriate for headless work and tests. Native event-loop code
    /// should use [`Self::try_receive`] to keep the UI thread responsive.
    ///
    /// # Errors
    ///
    /// Returns [`ReceiveError::Stopped`] when the worker exits before returning
    /// a frame.
    pub fn receive(&self) -> Result<SoftwareRenderResult, ReceiveError> {
        self.results
            .as_ref()
            .ok_or(ReceiveError::Stopped)?
            .recv()
            .map_err(|_| ReceiveError::Stopped)
    }
}

impl Drop for SoftwareRenderWorker {
    fn drop(&mut self) {
        self.requests.take();
        self.results.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct RenderRequest {
    frame_id: u64,
    display_list: DisplayList,
    width: u32,
    height: u32,
    scale_factor: f32,
}

fn render_frames(
    requests: Receiver<RenderRequest>,
    results: SyncSender<SoftwareRenderResult>,
    on_result: impl Fn(),
) {
    for request in requests {
        let result = PixelBuffer::new(request.width, request.height)
            .map_err(SoftwareRenderError::PixelBuffer)
            .and_then(|mut target| {
                SoftwareRenderer
                    .render_with_scale_factor(
                        &request.display_list,
                        &mut target,
                        request.scale_factor,
                    )
                    .map_err(SoftwareRenderError::Render)
                    .map(|()| target)
            });
        if results
            .send(SoftwareRenderResult {
                frame_id: request.frame_id,
                result,
            })
            .is_err()
        {
            break;
        }
        on_result();
    }
    drop(results);
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use torn_core::{Color, Point, Rect, Size};
    use torn_render::{DisplayList, PaintContext};

    use super::{SoftwareRenderError, SoftwareRenderWorker, SubmitError};
    use crate::{Pixel, RenderError};

    fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
        Rect::new(
            Point::new(x, y),
            Size::new(width, height).expect("valid test size"),
        )
    }

    #[test]
    fn renders_a_display_list_on_its_worker_thread() {
        let worker = SoftwareRenderWorker::spawn().expect("worker thread starts");
        let mut display_list = DisplayList::new();
        PaintContext::new(&mut display_list)
            .fill_rect(rect(0.0, 0.0, 2.0, 1.0), Color::rgba8(20, 40, 60, 255));

        worker
            .try_submit(17, display_list, 2, 1)
            .expect("empty queue accepts a frame");
        let rendered = worker.receive().expect("worker returns a frame");

        assert_eq!(rendered.frame_id(), 17);
        let pixels = rendered.into_result().expect("valid display list renders");
        assert_eq!(pixels.get(0, 0), Some(Pixel::rgba(20, 40, 60, 255)));
        assert_eq!(pixels.get(1, 0), Some(Pixel::rgba(20, 40, 60, 255)));
    }

    #[test]
    fn returns_render_errors_with_their_frame_identifier() {
        let worker = SoftwareRenderWorker::spawn().expect("worker thread starts");
        let mut display_list = DisplayList::new();
        PaintContext::new(&mut display_list).pop_clip();

        worker
            .try_submit(23, display_list, 1, 1)
            .expect("empty queue accepts a frame");
        let rendered = worker.receive().expect("worker returns a frame");

        assert_eq!(rendered.frame_id(), 23);
        assert_eq!(
            rendered.into_result(),
            Err(SoftwareRenderError::Render(RenderError::UnmatchedRestore))
        );
    }

    #[test]
    fn notifies_after_a_frame_is_available() {
        let (sender, receiver) = mpsc::channel();
        let worker = SoftwareRenderWorker::spawn_with_result_notifier(move || {
            sender.send(()).expect("test receiver remains available");
        })
        .expect("worker thread starts");

        worker
            .try_submit(1, DisplayList::new(), 1, 1)
            .expect("empty queue accepts a frame");
        receiver
            .recv()
            .expect("worker notifies after producing a frame");
        assert_eq!(
            worker
                .try_receive()
                .expect("worker remains connected")
                .expect("notified frame is available")
                .frame_id(),
            1
        );
    }

    #[test]
    fn limits_the_number_of_queued_frames() {
        let worker = SoftwareRenderWorker::spawn().expect("worker thread starts");
        let mut display_list = DisplayList::new();
        PaintContext::new(&mut display_list).pop_clip();

        worker
            .try_submit(1, display_list.clone(), 1, 1)
            .expect("first frame is accepted");

        let mut queue_filled = false;
        for frame_id in 2..=10_000 {
            match worker.try_submit(frame_id, display_list.clone(), 1, 1) {
                Ok(()) => {}
                Err(SubmitError::QueueFull) => {
                    queue_filled = true;
                    break;
                }
                Err(SubmitError::Stopped) => panic!("worker stopped unexpectedly"),
            }
        }

        assert!(
            queue_filled,
            "the worker must bound its pending-frame queue"
        );
    }
}
