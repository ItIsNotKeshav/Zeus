//! The microphone Settings pick and push to talk, on the window's side.
//!
//! The pick is saved in [`crate::settings::Settings::microphone`] and handed
//! to every account's backend, which opens it for calls; voice messages open
//! it through [`crate::audio::Recorder`]. Push to talk is read here from the
//! window's keys and the call screen's button, and only whether it is held
//! goes to the call's backend, which feeds the engine silence until it is
//! (see [`crate::call_audio::AudioInput::set_open`]).

use std::sync::mpsc;
use std::time::{Duration, Instant};

use egui::Key;

use crate::audio::Recorder;
use crate::backend::Waker;

/// The microphones the platform reports, listed on a thread of their own:
/// opening the device list can wait on the sound server, which the window
/// must not.
#[derive(Default)]
pub struct MicrophoneScan {
    /// `(id, label)` in label order, once a scan has finished.
    pub found: Option<Vec<(String, String)>>,
    pending: Option<mpsc::Receiver<Vec<(String, String)>>>,
}

impl MicrophoneScan {
    /// Lists the microphones again, unless a scan is already running.
    pub fn refresh(&mut self, waker: &Waker) {
        if self.pending.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let waker = waker.clone();
        let spawned = std::thread::Builder::new()
            .name("microphones".to_owned())
            .spawn(move || {
                let _ = tx.send(crate::call_audio::microphones());
                waker.wake();
            });
        if spawned.is_ok() {
            self.pending = Some(rx);
        }
    }

    /// A fixed list, for offline demos and tests.
    #[cfg(any(test, feature = "demo"))]
    pub fn fixed(found: Vec<(String, String)>) -> Self {
        Self {
            found: Some(found),
            pending: None,
        }
    }

    /// Takes a finished scan's list.
    pub fn poll(&mut self) {
        let Some(pending) = &self.pending else {
            return;
        };
        match pending.try_recv() {
            Ok(found) => {
                self.found = Some(found);
                self.pending = None;
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => self.pending = None,
        }
    }

    pub fn scanning(&self) -> bool {
        self.pending.is_some()
    }

    /// The label of the microphone with `id`, if the last scan found it.
    pub fn label(&self, id: &str) -> Option<&str> {
        self.found
            .as_ref()?
            .iter()
            .find(|(known, _)| known == id)
            .map(|(_, label)| label.as_str())
    }
}

/// How long a microphone check listens before it stops by itself.
pub const CHECK_LENGTH: Duration = Duration::from_secs(30);

/// Listens to a microphone so Settings can show how loud it hears the room.
/// Nothing it records is kept or sent.
pub struct MicCheck {
    recorder: Recorder,
    started: Instant,
}

impl MicCheck {
    pub fn start(recorder: Recorder) -> Self {
        Self {
            recorder,
            started: Instant::now(),
        }
    }

    /// The loudness of the last 50 ms, from 0 to 1 on a scale that reads
    /// speech at a normal distance as about half.
    pub fn level(&self) -> f32 {
        let levels = self.recorder.levels();
        // The loudest of the last three segments, so the bar does not
        // flicker between syllables.
        let recent = levels.iter().rev().take(3).copied().fold(0.0_f32, f32::max);
        meter(recent)
    }

    /// Whether the device has delivered sound yet.
    pub fn listening(&self) -> bool {
        self.recorder.listening()
    }

    /// Why the check stopped early, if it did.
    pub fn failure(&self) -> Option<String> {
        self.recorder.failure()
    }

    /// Whether the check has run its course.
    pub fn expired(&self) -> bool {
        self.started.elapsed() >= CHECK_LENGTH
    }
}

/// Maps a root-mean-square level to the meter's 0..1, on a decibel scale
/// from -54 dBFS (empty) to -6 dBFS (full).
pub fn meter(rms: f32) -> f32 {
    if rms <= 0.0 {
        return 0.0;
    }
    let db = 20.0 * rms.log10();
    ((db + 54.0) / 48.0).clamp(0.0, 1.0)
}

/// Push to talk as the window sees it.
#[derive(Debug, Default)]
pub struct PushToTalk {
    /// Whether the key is held, as of this frame.
    pub key_held: bool,
    /// Whether the call screen's hold-to-talk button is held.
    pub pointer_held: bool,
    /// What the call's backend was last told.
    pub sent: bool,
    /// Whether Settings is waiting for the key to bind.
    pub capturing: bool,
}

impl PushToTalk {
    pub fn held(&self) -> bool {
        self.key_held || self.pointer_held
    }
}

/// Whether `key` still works for push to talk while a text field has the
/// keyboard: only keys that neither type nor edit, the function keys and
/// Insert, are free then.
pub fn free_while_typing(key: Key) -> bool {
    key == Key::Insert || is_function_key(key)
}

fn is_function_key(key: Key) -> bool {
    let name = key.name();
    name.len() > 1
        && name.starts_with('F')
        && name[1..]
            .chars()
            .all(|character| character.is_ascii_digit())
}

/// What the settings and the call screen call `key`.
pub fn key_label(key: Key) -> String {
    match key {
        Key::Space => "Space".to_owned(),
        key => key.symbol_or_name().to_owned(),
    }
}

/// The key a press binds, from this frame's events: the first key pressed,
/// or `None` for none. Escape is never bound; it cancels.
pub fn pressed_key(events: &[egui::Event]) -> Option<Key> {
    events.iter().find_map(|event| match event {
        egui::Event::Key {
            key,
            pressed: true,
            repeat: false,
            ..
        } => Some(*key),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_keys_that_do_not_type_are_free_while_typing() {
        assert!(free_while_typing(Key::F8));
        assert!(free_while_typing(Key::F12));
        assert!(free_while_typing(Key::Insert));
        assert!(!free_while_typing(Key::Space));
        assert!(!free_while_typing(Key::F));
        assert!(!free_while_typing(Key::Backtick));
        assert!(!free_while_typing(Key::Home));
    }

    #[test]
    fn the_meter_reads_silence_as_empty_and_a_loud_voice_as_full() {
        assert_eq!(meter(0.0), 0.0);
        assert_eq!(meter(0.000_1), 0.0, "-80 dBFS is the noise floor");
        assert!((meter(0.03) - 0.49).abs() < 0.05, "speech is about half");
        assert_eq!(meter(1.0), 1.0);
    }

    #[test]
    fn a_saved_key_name_comes_back_as_the_same_key() {
        for key in [Key::Space, Key::F8, Key::V, Key::Backtick, Key::Insert] {
            assert_eq!(Key::from_name(key.name()), Some(key));
        }
    }

    #[test]
    fn the_first_new_press_is_the_one_bound() {
        let press = |key, repeat| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat,
            modifiers: egui::Modifiers::NONE,
        };
        let events = [
            press(Key::A, true),
            egui::Event::Text("v".to_owned()),
            press(Key::V, false),
            press(Key::B, false),
        ];
        assert_eq!(pressed_key(&events), Some(Key::V));
        assert_eq!(pressed_key(&[]), None);
    }
}
