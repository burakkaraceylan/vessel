use std::time::Duration;

use windows::{
    Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
    core::PWSTR,
};

use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};

use crate::{
    module::{EventPublisher, IntoModuleEvent},
    modules::system::events::SystemEvent,
};

pub struct WindowModule {
    pub event_tx: EventPublisher,
}

impl WindowModule {
    pub fn new(event_tx: EventPublisher) -> Self {
        Self { event_tx }
    }

    pub async fn run(&mut self) {
        let mut interval = tokio::time::interval(Duration::from_millis(250));
        let mut last_hwnd: usize = 0;
        let mut pid: u32 = 0;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    unsafe {
                        let hwnd = GetForegroundWindow();

                        if hwnd.0 as usize != last_hwnd {
                            last_hwnd = hwnd.0 as usize;
                            GetWindowThreadProcessId(hwnd, Some(&mut pid));

                            if let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                               let mut buf = [0u16; 260];
                               let mut size = 260u32;

                                 if QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut size).is_ok() {
                                        let exe = String::from_utf16_lossy(&buf[..size as usize]);
                                        let exe = std::path::Path::new(&exe)
                                            .file_name()
                                            .map(|n| n.to_string_lossy().into_owned())
                                            .unwrap_or(exe);

                                        let title = windows::Win32::UI::WindowsAndMessaging::GetWindowTextLengthW(hwnd)
                                         .checked_add(1)
                                         .and_then(|len| {
                                              let mut buf = vec![0u16; len as usize];
                                              let read_len = windows::Win32::UI::WindowsAndMessaging::GetWindowTextW(hwnd, &mut buf);
                                              if read_len > 0 {
                                                    Some(String::from_utf16_lossy(&buf[..read_len as usize]))
                                              } else {
                                                    None
                                              }
                                         })
                                         .unwrap_or_default();

                                        let _ = self.event_tx.send(SystemEvent::WindowFocusChanged(title, exe).into_event());
                                  };
                            }
                        } else {
                            continue;
                        };
                    };
                }

            }
        }
    }
}
