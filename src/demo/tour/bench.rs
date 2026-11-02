//! The benchmark: a photo-heavy chat scrolled through, chats opened in turn,
//! the sticker picker, a minimized window, and idle stretches, on a fixed
//! two-minute timetable. `scripts/bench/` launches it, reads the phase marks it
//! writes (`--demo-tour-marks`), and measures the process from outside.
//!
//! The timetable runs on the monotonic clock from the first frame. Between
//! cues the tour sends no input and asks for no frames, so an idle phase is as
//! idle as a window nobody touches.

use super::{Cue, Gesture, Target};
use crate::{
    app::{App, Conversation},
    model::{Chat, ChatFilter, Content, LastMessage, Page},
};
use egui::{Key as K, Modifiers, PointerButton};
use std::time::Duration;

/// The photo-heavy chat, pinned at the top of the list.
pub(super) const PHOTO_CHAT: &str = "15550002000@s.whatsapp.net";
/// Pictures in the photo chat, each its own file, so each is its own texture.
pub(super) const PHOTOS: usize = 240;
/// Animated stickers among them.
pub(super) const ANIMATED: usize = 4;
/// Chats besides the photo chat, enough for the churn to open 20 new ones.
pub(super) const OTHER_CHATS: usize = 24;
/// The oldest and the newest message of the photo chat.
pub(super) const FIRST: &str = "bench-photo-000";
pub(super) const LAST: &str = "bench-last";

/// Phases in order, each ended by the next. `scripts/bench` names them the same.
pub const PHASES: [&str; 6] = [
    "settle",
    "scroll",
    "churn",
    "picker",
    "minimized",
    "settle-again",
];
/// The phases whose frame times are recorded.
pub(super) const TIMED: [&str; 3] = ["scroll", "churn", "picker"];
/// The phase that finds nothing by its label, so its frames are the app's own.
pub(super) const SCROLL: &str = "scroll";

/// Long enough for the photo chat's first pictures to load and the window to
/// go quiet.
const SETTLE: f32 = 10.0;
/// One scroll pass: up to the oldest message, then down to the newest.
const SWEEP: f32 = 8.0;
/// Points a second the transcript moves. A sweep covers more than the photo
/// chat's height once its pictures have loaded, so it reaches the end on any
/// platform's fonts; the checks say whether it did.
const SCROLLED: f32 = 14_400.0;
/// Points a second the tour's wheel events carry. They come in points, as a
/// touchpad's do, and only Linux scales those up (to match a macOS
/// touchpad), so elsewhere the events carry that much more: every platform
/// scrolls the same distance.
const SPEED: f32 = if cfg!(target_os = "linux") {
    SCROLLED / fastframe_scroll::TOUCHPAD_SCALE
} else {
    SCROLLED
};
const PASSES: usize = 2;
/// Pause after each sweep, where the check is made.
const PAUSE: f32 = 0.5;
const SCROLL_START: f32 = SETTLE;
const SCROLL_END: f32 = SCROLL_START + PASSES as f32 * 2.0 * (SWEEP + PAUSE);
const CHURN_STEPS: usize = 20;
const CHURN_STEP: f32 = 0.6;
const CHURN_END: f32 = SCROLL_END + (CHURN_STEPS + 1) as f32 * CHURN_STEP;
const PICKER_ROUNDS: usize = 5;
const PICKER_ROUND: f32 = 2.2;
const PICKER_END: f32 = CHURN_END + PICKER_ROUNDS as f32 * PICKER_ROUND;
const MINIMIZED: f32 = 10.0;
/// The minimized phase is measured this long before the window comes back.
const BEFORE_RESTORE: f32 = 1.0;
const RESTORE: f32 = PICKER_END + MINIMIZED;
/// Past the 30 seconds after which a window left alone may let go of what it
/// does not show.
const SETTLE_AGAIN: f32 = 40.0;

/// Length of the benchmark, excluding its start delay.
pub const DURATION: Duration = Duration::from_secs((RESTORE + SETTLE_AGAIN) as u64);

/// Wheel scrolls over the transcript: from, until, and points a second
/// (positive scrolls up, towards older messages).
pub(super) static SCROLLS: [(f32, f32, f32); PASSES * 2] = {
    let mut sweeps = [(0.0, 0.0, 0.0); PASSES * 2];
    let mut index = 0;
    while index < PASSES * 2 {
        let from = SCROLL_START + index as f32 * (SWEEP + PAUSE);
        let speed = if index % 2 == 0 { SPEED } else { -SPEED };
        sweeps[index] = (from, from + SWEEP, speed);
        index += 1;
    }
    sweeps
};

/// Opens the photo chat at its newest message, in the dark theme, with the
/// other chats and the sticker picker's stickers in place.
pub fn prepare(app: &mut App) {
    super::common_setup(app);
    // One fixed look for every build and every date: the plain Dark theme.
    app.settings.set_halloween(false);
    for chat in &mut app.chats {
        chat.unread_mention = None;
    }
    app.chat_filter = ChatFilter::All;
    app.label_filter = None;
    let dir = app.dirs.media_cache_dir().join("bench");
    let _ = std::fs::create_dir_all(&dir);
    let now = crate::util::now();
    let mut photo_chat = Chat::new(PHOTO_CHAT.to_owned(), "Photo dump".to_owned());
    photo_chat.pinned = true;
    photo_chat.pinned_at = now * 1000 + 1;
    photo_chat.last_activity = now - 30;
    let photos = photo_messages(&dir, photo_chat.last_activity);
    add_chat(app, photo_chat, photos);
    for index in 0..OTHER_CHATS {
        let id = format!("1555010{index:04}@s.whatsapp.net");
        let mut chat = Chat::new(id.clone(), format!("Bench friend {:02}", index + 1));
        // Newer than every sample chat, so the churn stays among these.
        chat.last_activity = now - 60 - index as i64;
        let rows = friend_messages(&dir, &id, index, chat.last_activity);
        add_chat(app, chat, rows);
    }
    // Animated stickers on the picker's shelves too.
    let animated: Vec<_> = (0..ANIMATED)
        .map(|index| {
            super::super::stock::save(
                &dir,
                &format!("picker-animated-{index}.webp"),
                super::super::stock::ANIMATED_STICKER,
            )
        })
        .collect();
    app.stickers.extend(animated.iter().cloned());
    if let Some(pack) = app.sticker_packs.first_mut() {
        pack.stickers.extend(animated);
    }
    app.page = Page::Chats;
    app.open_chat = Some(PHOTO_CHAT.to_owned());
    app.scroll_to_bottom = true;
    app.video.silence();
}

/// The photo chat's messages, oldest first: a picture each minute, a line of
/// text now and then, and a few animated stickers.
fn photo_messages(dir: &std::path::Path, newest: i64) -> Vec<crate::model::Message> {
    use super::super::{media, message, stock};
    let start = newest - PHOTOS as i64 * 60 - 120;
    let mut rows = Vec::new();
    let mut sticker = 0;
    for index in 0..PHOTOS {
        let at = start + index as i64 * 60;
        let from_me = index % 3 == 0;
        let photo = stock::PHOTOS[index % stock::PHOTOS.len()];
        let mut content = media(
            "image/jpeg",
            photo.bytes.len() as u64,
            Some(photo.width),
            Some(photo.height),
        );
        content.path = Some(stock::save(
            dir,
            &format!("photo-{index:03}.jpg"),
            photo.bytes,
        ));
        let mut row = message(
            PHOTO_CHAT,
            &format!("bench-photo-{index:03}"),
            from_me,
            at,
            Content::Image {
                motion: None,
                caption: (index % 7 == 3).then(|| format!("Picture {}", index + 1)),
                media: content,
            },
        );
        row.thumbnail = Some(stock::thumbnail_for(index as u32));
        rows.push(row);
        if index % 6 == 5 {
            rows.push(message(
                PHOTO_CHAT,
                &format!("bench-text-{index:03}"),
                !from_me,
                at + 20,
                Content::text("More from the launch, the light was unreal"),
            ));
        }
        // Spread the animated stickers out, the last near the bottom.
        if sticker < ANIMATED && index == (sticker + 1) * PHOTOS / ANIMATED - 3 {
            let mut content = media(
                "image/webp",
                stock::ANIMATED_STICKER.len() as u64,
                Some(256),
                Some(256),
            );
            content.path = Some(stock::save(
                dir,
                &format!("sticker-{sticker}.webp"),
                stock::ANIMATED_STICKER,
            ));
            rows.push(message(
                PHOTO_CHAT,
                &format!("bench-sticker-{sticker}"),
                !from_me,
                at + 30,
                Content::Sticker {
                    media: content,
                    animated: true,
                },
            ));
            sticker += 1;
        }
    }
    rows.push(message(
        PHOTO_CHAT,
        LAST,
        true,
        newest,
        Content::text("That is all of them!"),
    ));
    rows
}

/// A short chat with two pictures of its own.
fn friend_messages(
    dir: &std::path::Path,
    chat: &str,
    index: usize,
    newest: i64,
) -> Vec<crate::model::Message> {
    use super::super::{media, message, stock};
    let lines = [
        "Are you coming on Saturday?",
        "Yes, I will bring the snacks",
        "Perfect, see you there",
    ];
    let mut rows = Vec::new();
    for (step, line) in lines.iter().enumerate() {
        rows.push(message(
            chat,
            &format!("bench-{index}-text-{step}"),
            step % 2 == 1,
            newest - 600 + step as i64 * 120,
            Content::text(*line),
        ));
    }
    for picture in 0..2 {
        let photo = stock::PHOTOS[(index + picture) % stock::PHOTOS.len()];
        let mut content = media(
            "image/jpeg",
            photo.bytes.len() as u64,
            Some(photo.width),
            Some(photo.height),
        );
        content.path = Some(stock::save(
            dir,
            &format!("friend-{index:02}-{picture}.jpg"),
            photo.bytes,
        ));
        let mut row = message(
            chat,
            &format!("bench-{index}-photo-{picture}"),
            picture == 1,
            newest - 60 + picture as i64 * 60,
            Content::Image {
                motion: None,
                caption: None,
                media: content,
            },
        );
        row.thumbnail = Some(stock::thumbnail_for((index + picture) as u32));
        rows.push(row);
    }
    rows
}

fn add_chat(app: &mut App, mut chat: Chat, messages: Vec<crate::model::Message>) {
    chat.last = messages.last().map(|last| LastMessage {
        labelled: last.content.labelled(),
        from_me: last.from_me,
        sender: last.sender.clone(),
        sender_name: last.sender_name.clone(),
        summary: last.summary(),
        full: last.content.full_summary(),
        status: last.status,
    });
    app.conversations.insert(
        chat.id.clone(),
        Conversation {
            complete: true,
            requested: true,
            phone_exhausted: true,
            messages,
            ..Default::default()
        },
    );
    app.chats.retain(|other| other.id != chat.id);
    app.chats.push(chat);
}

pub(super) fn script() -> Vec<Cue> {
    use Gesture::*;
    use Target::*;
    let mut cues = Vec::new();
    let mut add = |at: f32, gesture| cues.push(Cue { at, gesture });
    let left = PointerButton::Primary;
    add(0.0, Phase(PHASES[0]));
    add(
        SETTLE - 1.0,
        Expect(Bubble(LAST), "settle: newest message shown"),
    );
    add(SCROLL_START, Phase(PHASES[1]));
    for (pass, &(_, until, speed)) in SCROLLS.iter().enumerate() {
        let (target, label) = if speed > 0.0 {
            (FIRST, SCROLL_TOP[pass / 2])
        } else {
            (LAST, SCROLL_BOTTOM[pass / 2])
        };
        add(until + PAUSE * 0.5, Expect(Bubble(target), label));
    }
    add(SCROLL_END, Phase(PHASES[2]));
    for step in 0..CHURN_STEPS {
        add(
            SCROLL_END + (step + 1) as f32 * CHURN_STEP,
            Key(K::ArrowDown, Modifiers::ALT, "Alt + ↓ · Next chat"),
        );
    }
    add(CHURN_END, Phase(PHASES[3]));
    for round in 0..PICKER_ROUNDS {
        let at = CHURN_END + round as f32 * PICKER_ROUND;
        // A pointer move takes 0.28 seconds; each click waits for it.
        add(at + 0.1, Move(Picker));
        add(at + 0.5, Click(left));
        add(at + 1.0, Move(Label("Stickers")));
        add(at + 1.4, Click(left));
        add(
            at + 1.8,
            Key(K::Escape, Modifiers::NONE, "Esc · Close the picker"),
        );
    }
    add(PICKER_END, Phase(PHASES[4]));
    add(PICKER_END + 0.1, Minimize(true));
    add(
        PICKER_END + 1.5,
        ExpectMinimized("minimized: window reports minimized"),
    );
    add(RESTORE - BEFORE_RESTORE, Phase(PHASES[5]));
    add(RESTORE, Minimize(false));
    add(DURATION.as_secs_f32(), End);
    cues.sort_by(|a, b| a.at.total_cmp(&b.at));
    cues
}

const SCROLL_TOP: [&str; PASSES] = [
    "scroll 1: reached the oldest message",
    "scroll 2: reached the oldest message",
];
const SCROLL_BOTTOM: [&str; PASSES] = [
    "scroll 1: back at the newest message",
    "scroll 2: back at the newest message",
];
