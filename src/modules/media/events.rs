use crate::module::{IntoModuleEvent, ModuleEvent};
use crate::modules::media::smtc::{SmtcOutbound, SmtcTrack};

pub enum MediaEvent {
    TrackChanged(SmtcTrack),
    PlaybackStopped,
}

impl From<SmtcOutbound> for MediaEvent {
    fn from(outbound: SmtcOutbound) -> Self {
        match outbound {
            SmtcOutbound::TrackChanged(track) => MediaEvent::TrackChanged(track),
            SmtcOutbound::PlaybackStopped => MediaEvent::PlaybackStopped,
        }
    }
}

impl IntoModuleEvent for MediaEvent {
    fn into_event(self) -> ModuleEvent {
        match self {
            MediaEvent::TrackChanged(track) => ModuleEvent {
                source: "media",
                event: "track_changed".to_string(),
                data: serde_json::json!({
                    "title": track.title,
                    "artist": track.artist,
                    "album_artist": track.album_artist,
                    "album": track.album,
                    "subtitle": track.subtitle,
                    "playback_status": track.playback_status,
                    "cover_art_url": track.cover_art_key.as_deref()
                        .map(|k| format!("/api/assets/{k}")),
                }),
            },
            MediaEvent::PlaybackStopped => ModuleEvent {
                source: "media",
                event: "playback_stopped".to_string(),
                data: serde_json::Value::Null,
            },
        }
    }
}
