//! Taps on a Force Touch trackpad for gestures that cross a line: a swipe
//! reaching the point where letting go acts (and backing off it), a message
//! lifted out of the chat, and a carried message over a chat that takes it.
//!
//! macOS plays a tap only while a finger rests on the trackpad, and only
//! when the system's "Force Click and haptic feedback" setting is on, so a
//! mouse or a Magic Trackpad without Force Touch feels nothing. Other
//! platforms have no such hardware and do nothing.

/// What a tap feels like, as AppKit names its patterns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feel {
    /// A line crossed or something snapping into place: the light, crisp
    /// tick of a guide in a drawing app.
    Alignment,
    /// Plain feedback, for picking something up.
    Generic,
}

/// Taps the trackpad once, now.
pub fn tap(feel: Feel) {
    #[cfg(test)]
    TAPS.with(|taps| taps.borrow_mut().push(feel));
    #[cfg(all(target_os = "macos", not(test)))]
    {
        use objc2_app_kit::{
            NSHapticFeedbackManager, NSHapticFeedbackPattern, NSHapticFeedbackPerformanceTime,
            NSHapticFeedbackPerformer,
        };
        let pattern = match feel {
            Feel::Alignment => NSHapticFeedbackPattern::Alignment,
            Feel::Generic => NSHapticFeedbackPattern::Generic,
        };
        NSHapticFeedbackManager::defaultPerformer()
            .performFeedbackPattern_performanceTime(pattern, NSHapticFeedbackPerformanceTime::Now);
    }
    #[cfg(not(any(target_os = "macos", test)))]
    let _ = feel;
}

#[cfg(test)]
thread_local! {
    static TAPS: std::cell::RefCell<Vec<Feel>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The taps made on this thread since the last call, for tests.
#[cfg(test)]
pub fn take_taps() -> Vec<Feel> {
    TAPS.with(|taps| std::mem::take(&mut *taps.borrow_mut()))
}
