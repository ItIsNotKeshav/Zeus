//! The window's side of the microphone and camera Settings picks and of push
//! to talk: what every backend is told, the microphone and camera checks, and
//! what the call's backend hears about the key.

use super::App;
use crate::audio::Recorder;
use crate::backend::Command;
use crate::camera_check::CameraCheck;
use crate::voice_input::MicCheck;

impl App {
    /// What every account's backend needs to know about the call microphone.
    pub(super) fn call_input(&self) -> Command {
        Command::SetCallInput {
            microphone: self.settings.microphone.clone(),
            camera: self.settings.camera.clone(),
            push_to_talk: self.settings.push_to_talk,
        }
    }

    /// Hands the microphone and camera picks and push to talk to every
    /// account: whichever one a call comes on opens them.
    pub(super) fn send_call_input(&self) {
        for account in &self.accounts {
            account.backend.send(self.call_input());
        }
    }

    /// Lists the microphones for Settings, off the window's thread. An
    /// offline demo lists made-up ones instead of the machine's.
    pub(super) fn refresh_microphones(&mut self) {
        #[cfg(any(test, feature = "demo"))]
        if self.backend.is_offline() {
            self.microphones = crate::voice_input::DeviceScan::fixed(demo_microphones());
            return;
        }
        self.microphones
            .refresh(&self.waker, crate::call_audio::microphones);
    }

    /// Listens to the picked microphone so Settings can show its level. A
    /// call has the microphone to itself, and an offline demo hears a
    /// made-up voice.
    pub(super) fn start_mic_check(&mut self) {
        self.mic_check = None;
        if self.call.as_ref().is_some_and(|call| call.phase.is_live()) {
            return;
        }
        #[cfg(any(test, feature = "demo"))]
        if self.backend.is_offline() {
            self.mic_check = Some(MicCheck::start(Recorder::simulated(self.waker.clone())));
            return;
        }
        let recorder = Recorder::start(self.waker.clone(), self.settings.microphone_id());
        self.mic_check = Some(MicCheck::start(recorder));
    }

    /// Lists the cameras for Settings, off the window's thread. An offline
    /// demo lists made-up ones instead of the machine's.
    pub(super) fn refresh_cameras(&mut self) {
        #[cfg(any(test, feature = "demo"))]
        if self.backend.is_offline() {
            self.cameras = crate::voice_input::DeviceScan::fixed(demo_cameras());
            return;
        }
        self.cameras.refresh(&self.waker, crate::camera::cameras);
    }

    /// Looks through the picked camera so Settings can show it. A call has
    /// the camera to itself, and an offline demo shows a made-up picture.
    pub(super) fn start_camera_check(&mut self) {
        // The old check lets its camera go before the new one asks for it.
        self.camera_check = None;
        if self.call.is_some() {
            return;
        }
        #[cfg(any(test, feature = "demo"))]
        if self.backend.is_offline() {
            self.camera_check = Some(CameraCheck::simulated());
            return;
        }
        self.camera_check = Some(CameraCheck::start(
            self.settings.camera.clone(),
            self.waker.clone(),
        ));
    }

    /// Takes finished device lists, and ends a check that ran its course or
    /// that a call now needs the device for. The camera also stops while
    /// nobody can see it: the window hidden or minimized, or the app locked.
    pub(super) fn tick_voice_input(&mut self) {
        self.microphones.poll();
        self.cameras.poll();
        // Any call, even one still ringing, stops the look: a camera opens
        // for one program at a time on Windows, and the call opens it next.
        if self.camera_check.as_ref().is_some_and(|check| {
            check.expired()
                || self.call.is_some()
                || self.window_hidden
                || self.window_unseen
                || self.app_lock.is_locked()
        }) {
            self.camera_check = None;
        }
        let call_live = self.call.as_ref().is_some_and(|call| call.phase.is_live());
        if self
            .mic_check
            .as_ref()
            .is_some_and(|check| check.expired() || call_live)
        {
            self.mic_check = None;
        }
    }

    /// Tells the call's backend whether push to talk is held, when that
    /// changed. Nothing is held without a window in front, behind the app
    /// lock, or with push to talk off.
    pub(super) fn sync_push_to_talk(&mut self) {
        let live = self.call.as_ref().is_some_and(|call| call.phase.is_live());
        if !live || !self.window_focused || self.app_lock.is_locked() {
            self.push_to_talk.key_held = false;
            self.push_to_talk.pointer_held = false;
        }
        if !live {
            // The worker starts each call closed, so a new call starts from here.
            self.push_to_talk.sent = false;
            return;
        }
        let held = self.settings.push_to_talk && self.push_to_talk.held();
        if held != self.push_to_talk.sent {
            self.push_to_talk.sent = held;
            self.send_call_command(Command::SetCallTalking(held));
        }
    }
}

/// The cameras an offline demo offers.
#[cfg(any(test, feature = "demo"))]
pub(crate) fn demo_cameras() -> Vec<(String, String)> {
    vec![
        ("0".to_owned(), "Integrated Camera".to_owned()),
        ("1".to_owned(), "USB Webcam".to_owned()),
    ]
}

/// The microphones an offline demo offers.
#[cfg(any(test, feature = "demo"))]
pub(crate) fn demo_microphones() -> Vec<(String, String)> {
    vec![
        ("demo-headset".to_owned(), "Headset Microphone".to_owned()),
        ("demo-array".to_owned(), "Microphone Array".to_owned()),
        ("demo-usb".to_owned(), "USB Microphone".to_owned()),
    ]
}
