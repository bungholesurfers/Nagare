//! What the system's media sessions say, for the native app's autoMusic: Windows' Global System Media Transport
//! Controls, the sessions its media flyout (and the lock screen's) shows controls for - any app that plays media
//! through the system's transport controls: Spotify, a browser's tab, the Media Player. A thread of its own asks every
//! second, so the frame never waits on it; `main.rs` reads the last answer.

use std::sync::{Arc, Mutex};
#[cfg(windows)]
use std::time::Duration;

/// The sessions as last read: whether any is playing, how many there are, and the app of the one playing (or of the
/// first, when none is).
#[derive(Clone, Debug, Default)]
pub struct MediaStatus {
    pub playing: bool,
    pub sessions: usize,
    pub app: String,
}

pub struct MediaWatch {
    last: Arc<Mutex<Result<MediaStatus, String>>>,
}

impl MediaWatch {
    /// Starts the thread that reads the sessions.
    pub fn start() -> Self {
        let last = Arc::new(Mutex::new(Err("not read yet".to_string())));
        let out = last.clone();
        std::thread::Builder::new()
            .name("media sessions".into())
            .spawn(move || watch(out))
            .expect("media thread");
        MediaWatch { last }
    }

    pub fn status(&self) -> Result<MediaStatus, String> {
        self.last.lock().unwrap().clone()
    }
}

#[cfg(windows)]
fn watch(out: Arc<Mutex<Result<MediaStatus, String>>>) {
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSessionManager as Manager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
    };
    use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

    let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
    let manager = match Manager::RequestAsync().and_then(|op| op.join()) {
        Ok(m) => m,
        Err(e) => {
            *out.lock().unwrap() = Err(format!("media sessions unavailable: {e}"));
            return;
        }
    };
    loop {
        let read = || -> windows::core::Result<MediaStatus> {
            let sessions = manager.GetSessions()?;
            let mut status = MediaStatus { sessions: sessions.Size()? as usize, ..Default::default() };
            for i in 0..sessions.Size()? {
                let session = sessions.GetAt(i)?;
                let playing = session.GetPlaybackInfo()?.PlaybackStatus()? == Status::Playing;
                if playing || status.app.is_empty() {
                    status.app = session.SourceAppUserModelId()?.to_string();
                }
                if playing {
                    status.playing = true;
                    break;
                }
            }
            Ok(status)
        };
        *out.lock().unwrap() = read().map_err(|e| e.to_string());
        std::thread::sleep(Duration::from_secs(1));
    }
}

#[cfg(not(windows))]
fn watch(out: Arc<Mutex<Result<MediaStatus, String>>>) {
    *out.lock().unwrap() = Err("media sessions are read on Windows only".to_string());
}
