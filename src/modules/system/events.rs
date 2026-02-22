use crate::module::{IntoModuleEvent, ModuleEvent};

pub enum SystemEvent {
    WindowFocusChanged(String, String), // title, exe
}

impl IntoModuleEvent for SystemEvent {
    fn into_event(self) -> ModuleEvent {
        match self {
            SystemEvent::WindowFocusChanged(title, exe) => ModuleEvent::Stateful {
                source: "system",
                event: "window_focus_changed".to_string(),
                data: serde_json::json!({
                    "title": title,
                    "exe": exe
                }),
                cache_key: "system/window_focus_changed",
            },
        }
    }
}

