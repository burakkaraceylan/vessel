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
            // Both TrackChanged and PlaybackStopped share "media/now_playing" as their
            // cache key so they occupy a single cache slot and overwrite each other.
            // A snapshot never sends both — only the most recent one wins.
            MediaEvent::TrackChanged(track) => ModuleEvent::Stateful {
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
                cache_key: "media/now_playing".to_owned(),
            },
            MediaEvent::PlaybackStopped => ModuleEvent::Stateful {
                source: "media",
                event: "playback_stopped".to_string(),
                data: serde_json::Value::Null,
                cache_key: "media/now_playing".to_owned(),
            },
        }
    }
}
