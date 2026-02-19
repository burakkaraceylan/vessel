use anyhow::Context;
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use windows::{
    core::Interface,
    Foundation::TypedEventHandler,
    Media::Control::{
        GlobalSystemMediaTransportControlsSession,
        GlobalSystemMediaTransportControlsSessionManager,
        GlobalSystemMediaTransportControlsSessionMediaProperties,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
    },
    Storage::Streams::{DataReader, IInputStream, IRandomAccessStream},
};

// ---------------------------------------------------------------------------
// Public API — all Send types
// ---------------------------------------------------------------------------

pub struct SmtcTrack {
    pub title: String,
    pub artist: String,
    pub album_artist: Option<String>,
    pub album: Option<String>,
    pub subtitle: Option<String>,
    pub playback_status: String,
    /// Key into the shared assets store, e.g. `"media_cover_art"`.
    /// `None` if no cover art was available for this track.
    pub cover_art_key: Option<String>,
}

pub enum SmtcOutbound {
    TrackChanged(SmtcTrack),
    PlaybackStopped,
}

pub enum SmtcCommand {
    Play,
    Pause,
    Stop,
    Next,
    Previous,
}

/// A `Send`-safe handle to the SMTC background thread.
/// All `!Send` WinRT objects live on the dedicated thread.
pub struct SmtcModule {
    pub event_rx: mpsc::Receiver<SmtcOutbound>,
    pub command_tx: mpsc::Sender<SmtcCommand>,
    // Keeps the thread alive for the module's lifetime.
    _thread: std::thread::JoinHandle<()>,
}

impl SmtcModule {
    /// Spawns a dedicated thread with a single-threaded tokio runtime that owns
    /// all the `!Send` WinRT state. Returns once the SMTC manager is initialised.
    pub async fn new(
        cancel_token: CancellationToken,
        assets: Arc<DashMap<String, (Vec<u8>, String)>>,
    ) -> anyhow::Result<Self> {
        let (event_tx, event_rx) = mpsc::channel::<SmtcOutbound>(32);
        let (command_tx, command_rx) = mpsc::channel::<SmtcCommand>(32);
        let (init_tx, init_rx) = oneshot::channel::<anyhow::Result<()>>();

        let thread = std::thread::spawn(move || {
            // A single-threaded runtime — !Send futures are fine inside block_on.
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to build SMTC tokio runtime");

            rt.block_on(async move {
                match SmtcInner::new(event_tx, command_rx, cancel_token, assets).await {
                    Ok(mut inner) => {
                        let _ = init_tx.send(Ok(()));
                        inner.run().await;
                    }
                    Err(e) => {
                        let _ = init_tx.send(Err(e));
                    }
                }
            });
        });

        init_rx
            .await
            .context("SMTC thread died before initialisation")??;

        Ok(SmtcModule {
            event_rx,
            command_tx,
            _thread: thread,
        })
    }
}

// ---------------------------------------------------------------------------
// Private inner module — runs on the dedicated thread, may use !Send WinRT types
// ---------------------------------------------------------------------------

/// Holds the two event registrations for the active session.
/// Dropping it calls `Remove*` on both, preventing handler accumulation.
struct SessionSubscription(Option<Box<dyn FnOnce()>>);

impl SessionSubscription {
    /// Registers `MediaPropertiesChanged` and `PlaybackInfoChanged` on `session`.
    /// Returns `None` if either registration fails.
    fn new(session: GlobalSystemMediaTransportControlsSession, tx: mpsc::Sender<()>) -> Option<Self> {
        let tx2 = tx.clone();
        let props_handler = TypedEventHandler::new(move |_, _| {
            let _ = tx2.try_send(());
            Ok(())
        });
        let props_token = session.MediaPropertiesChanged(&props_handler).ok()?;

        let playback_handler = TypedEventHandler::new(move |_, _| {
            let _ = tx.try_send(());
            Ok(())
        });
        let playback_token = session.PlaybackInfoChanged(&playback_handler).ok()?;

        // Clone the session COM pointer into the closure so we can remove
        // the handlers later without holding a reference in the struct.
        let s = session.clone();
        Some(SessionSubscription(Some(Box::new(move || {
            let _ = s.RemoveMediaPropertiesChanged(props_token);
            let _ = s.RemovePlaybackInfoChanged(playback_token);
        }))))
    }
}

impl Drop for SessionSubscription {
    fn drop(&mut self) {
        if let Some(f) = self.0.take() { f(); }
    }
}

struct SmtcInner {
    global_manager: GlobalSystemMediaTransportControlsSessionManager,
    event_tx: mpsc::Sender<SmtcOutbound>,
    command_rx: mpsc::Receiver<SmtcCommand>,
    cancel_token: CancellationToken,
    session_changed_rx: mpsc::Receiver<()>,
    track_changed_rx: mpsc::Receiver<()>,
    track_changed_tx: mpsc::Sender<()>,
    /// Keeps the active-session subscriptions alive (and removes them on replace/drop).
    current_subscription: Option<SessionSubscription>,
    assets: Arc<DashMap<String, (Vec<u8>, String)>>,
}

impl SmtcInner {
    async fn new(
        event_tx: mpsc::Sender<SmtcOutbound>,
        command_rx: mpsc::Receiver<SmtcCommand>,
        cancel_token: CancellationToken,
        assets: Arc<DashMap<String, (Vec<u8>, String)>>,
    ) -> anyhow::Result<Self> {
        let (session_tx, session_rx) = mpsc::channel::<()>(8);
        let (track_tx, track_rx) = mpsc::channel::<()>(8);

        let global_manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
            .context("Failed to request SMTC session manager")?
            .await
            .context("SMTC session manager unavailable")?;

        let session_changed_handler = TypedEventHandler::new(move |_, _| {
            let _ = session_tx.try_send(());
            Ok(())
        });
        global_manager.CurrentSessionChanged(&session_changed_handler)?;

        let current_subscription = global_manager
            .GetCurrentSession()
            .ok()
            .and_then(|s| SessionSubscription::new(s, track_tx.clone()));

        Ok(SmtcInner {
            global_manager,
            event_tx,
            command_rx,
            cancel_token,
            session_changed_rx: session_rx,
            track_changed_rx: track_rx,
            track_changed_tx: track_tx,
            current_subscription,
            assets,
        })
    }

    async fn run(&mut self) {
        self.emit_current().await;

        use std::pin::pin;
        use tokio::time::{Duration, Instant, sleep_until};
        const DEBOUNCE: Duration = Duration::from_millis(150);

        // Starts already elapsed but is gated by `pending`, so it won't fire
        // until a notification arrives and resets the deadline.
        let mut debounce = pin!(sleep_until(Instant::now()));
        let mut pending = false;
        let mut session_dirty = false;

        loop {
            tokio::select! {
                _ = self.cancel_token.cancelled() => break,

                Some(cmd) = self.command_rx.recv() => {
                    self.dispatch_command(cmd).await;
                }

                result = self.session_changed_rx.recv() => {
                    if result.is_none() { break; }
                    session_dirty = true;
                    pending = true;
                    debounce.as_mut().reset(Instant::now() + DEBOUNCE);
                }

                result = self.track_changed_rx.recv() => {
                    if result.is_none() { break; }
                    pending = true;
                    debounce.as_mut().reset(Instant::now() + DEBOUNCE);
                }

                // Only fires when `pending` — collapses all rapid SMTC events
                // into a single read + emit once things settle.
                _ = &mut debounce, if pending => {
                    if session_dirty {
                        // Assigning drops the old SessionSubscription, which
                        // calls RemoveMediaPropertiesChanged / RemovePlaybackInfoChanged.
                        self.current_subscription = self.global_manager
                            .GetCurrentSession()
                            .ok()
                            .and_then(|s| SessionSubscription::new(s, self.track_changed_tx.clone()));
                        session_dirty = false;
                    }
                    self.emit_current().await;
                    pending = false;
                }
            }
        }
    }

    async fn emit_current(&self) {
        let outbound = match self.read_current().await {
            Some(track) => SmtcOutbound::TrackChanged(track),
            None => SmtcOutbound::PlaybackStopped,
        };
        let _ = self.event_tx.send(outbound).await;
    }

    async fn read_current(&self) -> Option<SmtcTrack> {
        let session = self.global_manager.GetCurrentSession().ok()?;
        let props = session.TryGetMediaPropertiesAsync().ok()?.await.ok()?;

        let playback_status = session
            .GetPlaybackInfo()
            .and_then(|info| info.PlaybackStatus())
            .map(|s| {
                if s == PlaybackStatus::Playing { "playing" }
                else if s == PlaybackStatus::Paused { "paused" }
                else if s == PlaybackStatus::Stopped { "stopped" }
                else { "unknown" }
            })
            .unwrap_or("unknown")
            .to_string();

        let cover_art_key = if let Some((bytes, content_type)) = try_read_cover_art(&props).await {
            const KEY: &str = "media_cover_art";
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();
            self.assets.insert(KEY.to_string(), (bytes, content_type));
            // Append timestamp as a cache-busting query param; the path extractor
            // in the handler sees only the key, so the map entry stays stable.
            Some(format!("{KEY}?t={ts}"))
        } else {
            None
        };

        Some(SmtcTrack {
            title: props.Title().ok()?.to_string(),
            artist: props.Artist().ok()?.to_string(),
            album_artist: nonempty(props.AlbumArtist().ok()?.to_string()),
            album: nonempty(props.AlbumTitle().ok()?.to_string()),
            subtitle: nonempty(props.Subtitle().ok()?.to_string()),
            playback_status,
            cover_art_key,
        })
    }

    async fn dispatch_command(&self, cmd: SmtcCommand) {
        let Ok(session) = self.global_manager.GetCurrentSession() else { return };
        let result: anyhow::Result<()> = async {
            match cmd {
                SmtcCommand::Play     => { session.TryPlayAsync()?.await?; }
                SmtcCommand::Pause    => { session.TryPauseAsync()?.await?; }
                SmtcCommand::Stop     => { session.TryStopAsync()?.await?; }
                SmtcCommand::Next     => { session.TrySkipNextAsync()?.await?; }
                SmtcCommand::Previous => { session.TrySkipPreviousAsync()?.await?; }
            }
            Ok(())
        }
        .await;
        if let Err(e) = result {
            eprintln!("SMTC command error: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn nonempty(s: String) -> Option<String> {
    if s.is_empty() { None } else { Some(s) }
}

async fn try_read_cover_art(
    props: &GlobalSystemMediaTransportControlsSessionMediaProperties,
) -> Option<(Vec<u8>, String)> {
    let thumbnail = props.Thumbnail().ok()?;
    let stream = thumbnail.OpenReadAsync().ok()?.await.ok()?;

    let as_ras: IRandomAccessStream = stream.cast().ok()?;
    let size = as_ras.Size().ok()? as u32;
    if size == 0 {
        return None;
    }

    let input_stream: IInputStream = as_ras.cast().ok()?;
    let reader = DataReader::CreateDataReader(&input_stream).ok()?;
    reader.LoadAsync(size).ok()?.await.ok()?;

    let mut buf = vec![0u8; size as usize];
    reader.ReadBytes(&mut buf).ok()?;

    Some((buf, "image/jpeg".to_string()))
}
