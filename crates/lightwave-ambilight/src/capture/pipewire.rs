//! Linux backend: asks the xdg-desktop-portal ScreenCast interface for
//! screen access, then consumes the resulting PipeWire video stream.

use std::os::fd::OwnedFd;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Duration;
use std::{env, fs};

use anyhow::{Context as _, Result, anyhow};
use ashpd::desktop::PersistMode;
use ashpd::desktop::screencast::{CursorMode, Screencast, SelectSourcesOptions, SourceType};
use ashpd::enumflags2::BitFlags;
use pipewire as pw;
use pw::spa;

use super::{Frame, PixelFormat};

/// How long to wait for the compositor to negotiate a video format after
/// the portal hands over the stream.
const FORMAT_TIMEOUT: Duration = Duration::from_secs(10);

pub struct CaptureOptions {
    /// Upper bound on the negotiated capture framerate.
    pub max_fps: u32,
    /// Ignore the saved portal permission and show the picker again.
    pub reselect: bool,
}

/// A running screen capture; frames flow to the callback passed to
/// [`Capture::open`] from a dedicated PipeWire thread until drop.
pub struct Capture {
    /// Wakes the PipeWire loop and asks it to quit.
    quit: pw::channel::Sender<()>,
    thread: Option<JoinHandle<()>>,
    size: (u32, u32),
}

impl Capture {
    pub fn open<F>(options: &CaptureOptions, on_frame: F) -> Result<Self>
    where
        F: FnMut(Frame<'_>) + Send + 'static,
    {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("building tokio runtime")?;
        let portal = runtime.block_on(portal_session(options.reselect))?;

        let (quit_tx, quit_rx) = pw::channel::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let max_fps = options.max_fps;

        let thread = thread::Builder::new()
            .name("ambilight-capture".into())
            .spawn(move || {
                let setup_failed = ready_tx.clone();
                if let Err(err) = run_capture_loop(portal, max_fps, on_frame, ready_tx, quit_rx) {
                    let _ = setup_failed.send(Err(err));
                }
            })
            .context("spawning capture thread")?;

        let size = ready_rx
            .recv_timeout(FORMAT_TIMEOUT)
            .map_err(|_| anyhow!("timed out waiting for the compositor to start the video stream"))?
            .context("setting up the PipeWire stream")?;

        Ok(Self {
            quit: quit_tx,
            thread: Some(thread),
            size: size.tuple(),
        })
    }

    /// Negotiated stream size in pixels.
    pub fn size(&self) -> (u32, u32) {
        self.size
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        let _ = self.quit.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct PortalStream {
    node_id: u32,
    fd: OwnedFd,
}

/// Run the portal handshake: session, source selection, user consent,
/// PipeWire remote. Returns the stream's node id and connection fd.
async fn portal_session(reselect: bool) -> Result<PortalStream> {
    let proxy = Screencast::new()
        .await
        .context("connecting to the ScreenCast portal (is xdg-desktop-portal running?)")?;

    // ashpd keeps the underlying D-Bus connection in a process-wide
    // static, so the cast session outlives these proxy handles and ends
    // when the process exits.
    let session = proxy
        .create_session(Default::default())
        .await
        .context("creating screencast session")?;

    let restore_token = if reselect { None } else { load_restore_token() };

    proxy
        .select_sources(
            &session,
            SelectSourcesOptions::default()
                .set_cursor_mode(CursorMode::Hidden)
                .set_sources(BitFlags::from(SourceType::Monitor))
                .set_multiple(false)
                .set_restore_token(restore_token.as_deref())
                .set_persist_mode(PersistMode::ExplicitlyRevoked),
        )
        .await
        .context("selecting screencast source")?;

    let response = proxy
        .start(&session, None, Default::default())
        .await
        .context("starting screencast session")?
        .response()
        .context("screen share request denied")?;

    if let Some(token) = response.restore_token() {
        save_restore_token(token);
    }

    let stream = response
        .streams()
        .first()
        .ok_or_else(|| anyhow!("portal returned no video streams"))?;
    let node_id = stream.pipe_wire_node_id();

    let fd = proxy
        .open_pipe_wire_remote(&session, Default::default())
        .await
        .context("opening PipeWire remote")?;

    Ok(PortalStream { node_id, fd })
}

/// With `PersistMode::ExplicitlyRevoked` the portal hands out a restore
/// token; presenting it on the next run skips the screen picker dialog.
fn token_path() -> Option<PathBuf> {
    env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))
        .map(|base| base.join("lightwave/screencast-token"))
}

fn load_restore_token() -> Option<String> {
    let token = fs::read_to_string(token_path()?).ok()?;
    let token = token.trim();
    (!token.is_empty()).then(|| token.to_string())
}

fn save_restore_token(token: &str) {
    let Some(path) = token_path() else { return };

    let saved = path
        .parent()
        .map_or(Ok(()), fs::create_dir_all)
        .and_then(|()| fs::write(&path, token));

    if let Err(err) = saved {
        eprintln!("warning: could not save screencast restore token: {err}");
    }
}

struct StreamState<F> {
    on_frame: F,
    info: Option<spa::param::video::VideoInfoRaw>,
    /// Fires once, when the first format negotiation completes.
    ready: Option<mpsc::Sender<Result<Size>>>,
}

struct Size {
    width: u32,
    height: u32,
}

impl Size {
    fn tuple(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

fn run_capture_loop<F>(
    portal: PortalStream,
    max_fps: u32,
    on_frame: F,
    ready: mpsc::Sender<Result<Size>>,
    quit: pw::channel::Receiver<()>,
) -> Result<()>
where
    F: FnMut(Frame<'_>) + Send + 'static,
{
    pw::init();

    let mainloop = pw::main_loop::MainLoopRc::new(None).context("creating PipeWire main loop")?;
    let context =
        pw::context::ContextRc::new(&mainloop, None).context("creating PipeWire context")?;
    let core = context
        .connect_fd_rc(portal.fd, None)
        .context("connecting to the portal's PipeWire remote")?;

    let stream = pw::stream::StreamRc::new(
        core,
        "lightwave-ambilight",
        pw::properties::properties! {
            *pw::keys::MEDIA_TYPE => "Video",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Screen",
        },
    )
    .context("creating PipeWire stream")?;

    let state = StreamState {
        on_frame,
        info: None,
        ready: Some(ready),
    };

    let _listener = stream
        .add_local_listener_with_user_data(state)
        .state_changed(|_, _, _, new| {
            if let pw::stream::StreamState::Error(err) = new {
                eprintln!("video stream error: {err}");
            }
        })
        .param_changed(|_, state, id, param| {
            let Some(param) = param else { return };
            if id != spa::param::ParamType::Format.as_raw() {
                return;
            }

            let Ok((media_type, media_subtype)) = spa::param::format_utils::parse_format(param)
            else {
                return;
            };
            if media_type != spa::param::format::MediaType::Video
                || media_subtype != spa::param::format::MediaSubtype::Raw
            {
                return;
            }

            let mut info = spa::param::video::VideoInfoRaw::default();
            if info.parse(param).is_err() {
                return;
            }

            if let Some(ready) = state.ready.take() {
                let _ = ready.send(Ok(Size {
                    width: info.size().width,
                    height: info.size().height,
                }));
            }
            state.info = Some(info);
        })
        .process(|stream, state| {
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            let Some(info) = &state.info else { return };
            let Some(format) = pixel_format(info.format()) else {
                return;
            };

            let width = info.size().width as usize;
            let height = info.size().height as usize;

            let Some(data) = buffer.datas_mut().first_mut() else {
                return;
            };
            let chunk = data.chunk();
            let offset = chunk.offset() as usize;
            let size = chunk.size() as usize;
            let stride = match usize::try_from(chunk.stride()) {
                Ok(stride) if stride > 0 => stride,
                _ => width * format.bytes_per_pixel(),
            };

            let Some(bytes) = data.data() else { return };
            let end = (offset + size).min(bytes.len());
            let Some(slice) = bytes.get(offset..end) else {
                return;
            };
            if height == 0 || slice.len() < (height - 1) * stride + width * format.bytes_per_pixel()
            {
                return;
            }

            (state.on_frame)(Frame {
                width,
                height,
                stride,
                format,
                data: slice,
            });
        })
        .register()
        .context("registering stream listener")?;

    let pod_bytes = video_format_pod(max_fps);
    let mut params = [spa::pod::Pod::from_bytes(&pod_bytes)
        .ok_or_else(|| anyhow!("building video format pod"))?];

    stream
        .connect(
            spa::utils::Direction::Input,
            Some(portal.node_id),
            pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
            &mut params,
        )
        .context("connecting PipeWire stream")?;

    let _quit_guard = quit.attach(mainloop.loop_(), {
        let mainloop = mainloop.clone();
        move |()| mainloop.quit()
    });

    mainloop.run();

    Ok(())
}

/// What we accept from the compositor: raw RGB-family video in shared
/// memory (MAP_BUFFERS + no DRM modifiers means no dmabuf), any size,
/// framerate capped at `max_fps`.
fn video_format_pod(max_fps: u32) -> Vec<u8> {
    let obj = spa::pod::object!(
        spa::utils::SpaTypes::ObjectParamFormat,
        spa::param::ParamType::EnumFormat,
        spa::pod::property!(
            spa::param::format::FormatProperties::MediaType,
            Id,
            spa::param::format::MediaType::Video
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::MediaSubtype,
            Id,
            spa::param::format::MediaSubtype::Raw
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoFormat,
            Choice,
            Enum,
            Id,
            spa::param::video::VideoFormat::BGRx,
            spa::param::video::VideoFormat::BGRx,
            spa::param::video::VideoFormat::RGBx,
            spa::param::video::VideoFormat::BGRA,
            spa::param::video::VideoFormat::RGBA,
            spa::param::video::VideoFormat::BGR,
            spa::param::video::VideoFormat::RGB,
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoSize,
            Choice,
            Range,
            Rectangle,
            spa::utils::Rectangle {
                width: 1920,
                height: 1080
            },
            spa::utils::Rectangle {
                width: 1,
                height: 1
            },
            spa::utils::Rectangle {
                width: 16384,
                height: 16384
            }
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoFramerate,
            Choice,
            Range,
            Fraction,
            spa::utils::Fraction {
                num: max_fps,
                denom: 1
            },
            spa::utils::Fraction { num: 0, denom: 1 },
            spa::utils::Fraction {
                num: max_fps,
                denom: 1
            }
        ),
    );

    spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &spa::pod::Value::Object(obj),
    )
    .expect("serializing a static format pod cannot fail")
    .0
    .into_inner()
}

fn pixel_format(format: spa::param::video::VideoFormat) -> Option<PixelFormat> {
    use spa::param::video::VideoFormat as Vf;

    match format {
        Vf::BGRx => Some(PixelFormat::Bgrx),
        Vf::RGBx => Some(PixelFormat::Rgbx),
        Vf::BGRA => Some(PixelFormat::Bgra),
        Vf::RGBA => Some(PixelFormat::Rgba),
        Vf::BGR => Some(PixelFormat::Bgr),
        Vf::RGB => Some(PixelFormat::Rgb),
        _ => None,
    }
}
