//! The Mac's media keys and Now Playing: macOS sends play, pause, next and previous to
//! whichever app last reported a song, so odeion reports one and listens.

use crate::library::Track;
use souvlaki::{MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

/// How far the reported position may drift before it is corrected, after a seek.
const DRIFT: Duration = Duration::from_secs(1);

pub struct Media {
    controls: MediaControls,
    pub events: Receiver<MediaControlEvent>,
    /// The song and length last reported.
    shown: Option<(PathBuf, Option<Duration>)>,
    /// Playing or not, at which position, when.
    reported: Option<(bool, Duration, Instant)>,
}

impl Media {
    /// Media keys for `window`. Windows ties them to a window and souvlaki panics without
    /// its handle, so there it goes without them when the handle can't be had.
    pub fn new(window: &impl raw_window_handle::HasWindowHandle) -> Option<Media> {
        let hwnd = window.window_handle().ok().and_then(|h| match h.as_raw() {
            raw_window_handle::RawWindowHandle::Win32(h) => Some(h.hwnd.get() as *mut std::ffi::c_void),
            _ => None,
        });
        if cfg!(windows) && hwnd.is_none() {
            return None;
        }
        let mut controls = MediaControls::new(PlatformConfig {
            display_name: "Odeion",
            dbus_name: "odeion",
            hwnd,
        })
        .ok()?;
        let (tx, events) = channel();
        controls
            .attach(move |event| {
                let _ = tx.send(event);
            })
            .ok()?;
        Some(Media { controls, events, shown: None, reported: None })
    }

    /// Tells macOS which song is on, when it changed.
    pub fn show(&mut self, track: &Track, duration: Option<Duration>) {
        let shown = Some((track.path.clone(), duration));
        if self.shown == shown {
            return;
        }
        self.shown = shown;
        self.reported = None;
        // NSURL wants the space in "Application Support" escaped.
        let cover = track
            .cover
            .as_ref()
            .map(|c| format!("file://{}", c.display()).replace(' ', "%20"));
        let _ = self.controls.set_metadata(MediaMetadata {
            title: Some(&track.title),
            artist: Some(&track.artist),
            album: Some(&track.album),
            cover_url: cover.as_deref(),
            duration,
        });
    }

    /// Tells macOS whether it plays and where, when that differs from what it was told.
    pub fn status(&mut self, playing: bool, position: Duration) {
        let expected = self.reported.map(|(was, at, when)| {
            (was, if was { at + when.elapsed() } else { at })
        });
        let stale = match expected {
            Some((was, at)) => was != playing || at.abs_diff(position) > DRIFT,
            None => true,
        };
        if !stale {
            return;
        }
        self.reported = Some((playing, position, Instant::now()));
        let progress = Some(MediaPosition(position));
        let _ = self.controls.set_playback(if playing {
            MediaPlayback::Playing { progress }
        } else {
            MediaPlayback::Paused { progress }
        });
    }
}
