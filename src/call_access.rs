//! Whether a placed call may start: on macOS, voice needs the microphone and
//! video needs the camera as well. The UI asks before placing; this module
//! decides when to ask, when to wait off the interface thread, and when a
//! refusal means the call must not start at all.

use std::sync::mpsc;

use crate::backend::Waker;

/// Used by the macOS ask, and by the tests. Other platforms grant the call
/// without reading a permission.
#[cfg(any(test, target_os = "macos"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Status {
    Granted,
    Denied,
    Unknown,
}

/// Voice needs the microphone. Video needs the microphone and the camera.
#[cfg(any(test, target_os = "macos"))]
pub(crate) fn allowed(video: bool, mic: Status, camera: Status) -> bool {
    mic == Status::Granted && (!video || camera == Status::Granted)
}

/// A required permission is already refused, so asking further cannot help.
#[cfg(any(test, target_os = "macos"))]
pub(crate) fn blocked(video: bool, mic: Status, camera: Status) -> bool {
    mic == Status::Denied || (video && camera == Status::Denied)
}

pub(crate) enum Begin {
    /// Every needed permission is already granted: place in this turn.
    Ready,
    /// A needed permission is refused, or the ask could not run: place nothing.
    #[cfg(any(test, target_os = "macos"))]
    Stop,
    /// A background thread is asking; poll the receiver from the frame loop.
    #[cfg(any(test, target_os = "macos"))]
    Wait(mpsc::Receiver<bool>),
}

pub(crate) fn begin(video: bool, waker: &Waker) -> Begin {
    #[cfg(test)]
    {
        test_gate::begin(video, waker)
    }
    #[cfg(not(test))]
    {
        begin_os(video, waker)
    }
}

#[cfg(not(test))]
fn begin_os(video: bool, waker: &Waker) -> Begin {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (video, waker);
        Begin::Ready
    }
    #[cfg(target_os = "macos")]
    {
        let mic = macos::microphone_status();
        let camera = if video {
            macos::camera_status()
        } else {
            Status::Granted
        };
        if allowed(video, mic, camera) {
            Begin::Ready
        } else if blocked(video, mic, camera) {
            Begin::Stop
        } else {
            let waker = waker.clone();
            let (sender, receiver) = mpsc::channel();
            let spawned = std::thread::Builder::new()
                .name("call-permission".into())
                .spawn(move || {
                    let granted = request(video);
                    let _ = sender.send(granted);
                    waker.wake();
                });
            match spawned {
                Ok(_) => Begin::Wait(receiver),
                Err(error) => {
                    log::warn!("[CALL] call permission could not be asked: {error}");
                    Begin::Stop
                }
            }
        }
    }
}

/// Asks the OS for every permission this call needs and waits for the answers.
#[cfg(any(test, target_os = "macos"))]
fn request(video: bool) -> bool {
    if !ensure_microphone() {
        return false;
    }
    if video && !crate::camera::camera_allowed() {
        return false;
    }
    true
}

#[cfg(any(test, target_os = "macos"))]
fn ensure_microphone() -> bool {
    #[cfg(target_os = "macos")]
    {
        macos::ensure_microphone()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::sync::mpsc;
    use std::time::Duration;

    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_av_foundation::{
        AVAuthorizationStatus, AVCaptureDevice, AVMediaType, AVMediaTypeAudio, AVMediaTypeVideo,
    };

    use super::Status;

    fn media_audio() -> Option<&'static AVMediaType> {
        unsafe { AVMediaTypeAudio }
    }

    fn media_video() -> Option<&'static AVMediaType> {
        unsafe { AVMediaTypeVideo }
    }

    /// Same budget as the camera permission in [`crate::camera`].
    const PERMISSION_WAIT: Duration = Duration::from_secs(60);

    fn map(status: AVAuthorizationStatus) -> Status {
        match status {
            AVAuthorizationStatus::Authorized => Status::Granted,
            AVAuthorizationStatus::Denied | AVAuthorizationStatus::Restricted => Status::Denied,
            _ => Status::Unknown,
        }
    }

    pub(super) fn microphone_status() -> Status {
        let Some(media) = media_audio() else {
            return Status::Denied;
        };
        map(unsafe { AVCaptureDevice::authorizationStatusForMediaType(media) })
    }

    pub(super) fn camera_status() -> Status {
        let Some(media) = media_video() else {
            return Status::Denied;
        };
        map(unsafe { AVCaptureDevice::authorizationStatusForMediaType(media) })
    }

    pub(super) fn ensure_microphone() -> bool {
        match microphone_status() {
            Status::Granted => true,
            Status::Denied => false,
            Status::Unknown => {
                let Some(media) = media_audio() else {
                    return false;
                };
                let (sender, receiver) = mpsc::channel();
                let handler = RcBlock::new(move |granted: Bool| {
                    let _ = sender.send(granted.as_bool());
                });
                unsafe {
                    AVCaptureDevice::requestAccessForMediaType_completionHandler(media, &handler);
                }
                matches!(receiver.recv_timeout(PERMISSION_WAIT), Ok(true))
            }
        }
    }
}

#[cfg(test)]
mod test_gate {
    use std::cell::RefCell;
    use std::sync::mpsc;

    use super::{Begin, Waker};

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(super) enum Gate {
        Granted,
        Denied,
        Hold,
    }

    thread_local! {
        static GATE: RefCell<Gate> = const { RefCell::new(Gate::Granted) };
        static HOLD_SENDER: RefCell<Option<mpsc::Sender<bool>>> = const { RefCell::new(None) };
    }

    pub(super) fn set(gate: Gate) {
        GATE.with(|slot| *slot.borrow_mut() = gate);
    }

    pub(super) fn take() -> Gate {
        GATE.with(|gate| *gate.borrow())
    }

    pub(super) fn begin(video: bool, waker: &Waker) -> Begin {
        let _ = (video, waker);
        match GATE.with(|gate| *gate.borrow()) {
            Gate::Granted => Begin::Ready,
            Gate::Denied => Begin::Stop,
            Gate::Hold => {
                let (sender, receiver) = mpsc::channel();
                HOLD_SENDER.with(|hold| *hold.borrow_mut() = Some(sender));
                Begin::Wait(receiver)
            }
        }
    }

    pub(super) fn answer(granted: bool) {
        if let Some(sender) = HOLD_SENDER.with(|hold| hold.borrow_mut().take()) {
            let _ = sender.send(granted);
        }
    }

    pub(super) fn reset(gate: Gate) {
        set(gate);
        HOLD_SENDER.with(|hold| *hold.borrow_mut() = None);
    }
}

#[cfg(test)]
pub(crate) fn answer(granted: bool) {
    test_gate::answer(granted);
}

/// Tests pin the gate so a reused thread cannot keep a refusal from a prior run.
#[cfg(test)]
pub(crate) struct GateGuard {
    previous: test_gate::Gate,
}

#[cfg(test)]
impl GateGuard {
    pub(crate) fn denied() -> Self {
        Self::new(test_gate::Gate::Denied)
    }

    pub(crate) fn hold() -> Self {
        Self::new(test_gate::Gate::Hold)
    }

    fn new(gate: test_gate::Gate) -> Self {
        let previous = test_gate::take();
        test_gate::set(gate);
        Self { previous }
    }
}

#[cfg(test)]
impl Drop for GateGuard {
    fn drop(&mut self) {
        test_gate::reset(self.previous);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_is_allowed_with_only_the_microphone() {
        assert!(allowed(false, Status::Granted, Status::Denied));
    }

    #[test]
    fn video_needs_both_permissions() {
        assert!(!allowed(true, Status::Granted, Status::Denied));
        assert!(!allowed(true, Status::Granted, Status::Unknown));
        assert!(allowed(true, Status::Granted, Status::Granted));
        assert!(!allowed(true, Status::Denied, Status::Granted));
    }

    #[test]
    fn video_is_blocked_when_the_camera_is_already_refused() {
        assert!(blocked(true, Status::Unknown, Status::Denied));
    }

    #[test]
    fn voice_is_not_blocked_by_a_refused_camera() {
        assert!(!blocked(false, Status::Granted, Status::Denied));
        assert!(!blocked(false, Status::Unknown, Status::Denied));
    }

    #[test]
    fn unknown_microphone_is_not_blocked() {
        assert!(!blocked(false, Status::Unknown, Status::Granted));
        assert!(!blocked(true, Status::Unknown, Status::Granted));
    }

    #[test]
    fn the_os_request_function_is_linked() {
        let _request: fn(bool) -> bool = request;
        #[cfg(target_os = "macos")]
        {
            let _mic: fn() -> Status = super::macos::microphone_status;
            let _camera: fn() -> Status = super::macos::camera_status;
        }
    }
}
