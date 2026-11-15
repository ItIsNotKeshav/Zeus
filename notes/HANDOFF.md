# Handoff

Where the work on this fork stands, for whoever picks it up next (a person
or an agent). Read this first, then [DECISIONS.md](DECISIONS.md) for why
things are the way they are, [ARCHITECTURE.md](ARCHITECTURE.md) for how the
app fits together, and [NOTES.md](NOTES.md) for commands and gotchas.

Last updated: 2026-11-05.

## Motion and haptics work (2026-11-05, open)

Goal: Zeus as delightful as Telegram macOS; send and receive exactly like
iMessage. All on `main`, pushed to `origin` (`pratyakshm/Zeus`). Nothing
here was run in the app by an agent: motion needs the maintainer's eyes.

Done, newest last:

| Commit | What |
|---|---|
| `cdf5da6` | Circle buttons ease hover, shrink when pressed; composer mic/send/check morphs |
| `8ee5868` | Chat list rows glide to new place (FLIP), unread badge pops and rolls |
| `889ab11` | Send: Telegram curve from composer (0.3 s, split x/y beziers), field shape morphs into bubble, list glides on same curve. Receive: grows from typing bubble. Reaction pop, tick pop |
| `0db0b19` | Deleted message fades and shrinks while its gap closes (`src/ui/leaving.rs`) |
| `cc09f75` | Trackpad taps (`src/haptics.rs`, `NSHapticFeedbackManager`): swipe threshold in and out, message lift, carry over a chat |
| `3e63d4d` | Inline edit keeps bubble size until text outgrows it; "Editing message" label floats above |
| `39bdf37` | Photo preview grows from its bubble and back; recording strip breathes with haptics; quick reactions spring in; quote jump glides |
| `1607be2` | First reaction and our own pick pop; ticks pop on every own bubble; deleted album leaves as one row |
| `7e535f3` | Chat list: mention badge and manual-unread dot pop in and out |
| `2638da7` | Video and round-note preview flight; bubble picture hidden while any flight is in the air |
| `30a6afa` | iMessage send/receive: `motion::Spring` (SwiftUI duration/bounce), send from composer text rect with field-colour crossfade 0.18 s, vertical spring 0.5 s bounce 0.2, horizontal critically damped 0.35 s, per-row wave 15 ms cap 90 ms; receive pops from tail corner 0.6 to 1 (0.45 s bounce 0.25); typing bubble pops and breathes. Telegram curve removed. First message in a chat after open now animates too |

| `91326d4` | Viewer video takes its poster's shape before the first frame (no 16:9 stretch); pictures in flight are cover-cropped (`paint_flying`, `wallpaper::cover_uv`), so a note or square album tile opens out instead of stretching. Not compiled |
| `3242946` | Ticks over a picture (`footer_over_picture`) pop through `tick_motion`, which takes a `plain` colour. Not compiled |
| `8cbda08` | Springy scroll bubbles (`src/ui/scroll_springs.rs`): `Spring::new(1.0, 0.2)`, resistance `/1200`, `MAX_LAG` 20 pt, order kept; only reader scrolls push. Test switch `scroll_springs::hold_still()` holds rows in the three layout-measuring scroll tests in `demo.rs`; that switch not compiled |

All of the above landed 2026-11-05 on local `main`, made by three agents in
worktrees. Checks: fmt, `cargo check`, clippy `-D warnings` and focused
tests on E-cores; full suite and `--all-features` left to CI. The last
three: the maintainer stopped all local builds ("do not build it, just code
and commit"), so they rely on CI. `91326d4..8cbda08` not pushed yet.

Test failures the springs agent saw that are likely already on `83c2fc3`
(confirm on CI): `a_text_selection_skips_rows_far_from_it_and_the_view` and
`a_copy_across_a_long_text_selection_has_every_message` (proved broken on
main, by a commit in `e2d8d2e..0db0b19`); jump tests
(`a_jump_far_up_lands_on_the_message_and_stays_there`,
`a_jump_to_an_album_member_reaches_the_group_and_finishes`,
`a_message_reached_from_a_quote_flashes_across_the_view_then_fades`), about
nine image-preview zoom/drag/name tests, and
`tour::tests::polls_are_created_and_voted_through_real_controls`.

Album leave, read from the code: each item of a deleted album is its own
`DeleteLocal`, its own tokio task and app-state mutation, and its own
`MessageDeleted` on acceptance. They merge into one leaving row only if all
arrive within `MOTION_PANEL` (0.22 s), which a real network often misses.
A fix would hold confirmed items on screen until the album's other pending
items are confirmed (or a timeout), which bends "remove after WhatsApp
accepts" visually; maintainer to decide.

Next, in order:

1. **Maintainer looks at it in the app.** Nothing was seen running.
   Watch for: wave timing and spring overshoot; where the send starts; the
   crossfade (`field_veil` finds the bubble's background by its size, only
   tested on a hand-built shape); several sends in quick succession;
   receive pop and typing breath.
2. Video flight quirks to check: poster-only flight stretches to the
   preview's 16:9 fallback; notes fly full-frame while the bubble crops
   square; possible one-frame overlap as the bubble reappears.
3. Remaining small gaps: ticks over a picture (`footer_over_picture`) do not
   pop; album leave untested against a real multi-item deletion (does it
   emit one `MessageDeleted` per item close enough to merge?); a partial
   album deletion leaves as a smaller album ghost.

Rules for this work: build only on E-cores (`taskpolicy -b cargo ... -j 6`),
never P-cores (an agent worktree can start from `cp -cR <main>/target ./target`,
though the 2026-11-05 iMessage agent still rebuilt its deps cold, ~25 min); commits carry the maintainer's identity only, no Claude
trailer; push only when the maintainer says so (this session had a standing
"keep pushing"); group commits by area.

## Repository

- Fork: `github.com/ItIsNotKeshav/zapfast` (remote `origin`). Upstream is
  `github.com/crmne/zapfast`.
- All work is on `main`, pushed to `origin`. The feature branches are gone.
- Since 2026-10-19 the app is **Zeus** (`src/brand.rs`), installed beside
  ZapFast. How to build it and its installer is in [`AGENTS.md`](../AGENTS.md),
  under "Zeus builds on Windows".
- The project's own rules live in [`AGENTS.md`](../AGENTS.md). It asks for
  trunk-based work on `main`, one topic per commit.

## What the branch contains

Oldest first. The first seven were made with Codex; the last four in a Claude
Code session.

| Commit | What it does |
|---|---|
| `0ea1230` | Nothing Dark and Nothing Light themes, bundled Geist and Google Sans Flex fonts, auto-hide custom title bar |
| `89df810` | Nothing chat bubbles styled after Autumn Spires |
| `b542a77` | Nothing message metadata matched to Autumn Spires |
| `4899819` | Nothing chat message metadata restored |
| `e08b6f7` | Variable UI font weights 30 lighter |
| `1321858` | Message media and image preview polish |
| `b7b221a` | Floating sidebar layout, Ctrl+Tab between the two latest chats, `DESIGN.md` tokens |
| `7cac3eb` | Removed the custom title bar entirely (Windows and Linux use the native frame) |
| `39ade75` | Chat header, info pane and search pane headers lined up with the chat list's title |
| `9912dba` | On switches keep a visible knob on Nothing Dark's near-white accent |
| `5c0266e` | Call microphone read with a timeout; a microphone that sends nothing is given up for the next one |

## State of the machine

- The installed app in `%LOCALAPPDATA%\Programs\ZapFast\zapfast.exe` is a
  **release build of `5c0266e`**. The original 0.19.0 binary is kept beside it
  as `zapfast-0.19.0-original.exe`. To go back, quit ZapFast and copy the
  original over `zapfast.exe`.
- That installed copy is running with the linked WhatsApp account and
  connects normally.
- It still reports version 0.19.0, so its updater may offer the next upstream
  release. Installing that (it needs a click on restart) replaces this build
  with upstream's.

## Open issues

1. **Outgoing call audio is silent.** The other person hears nothing. On
   2026-10-19 a level test while talking showed neither microphone hearing a
   voice: Jack Mic (the Windows default) peaked at 257 and the built-in
   Microphone Array at 148, out of 32767. Windows privacy allows desktop apps,
   neither device is muted, and volumes are 89% and 100%. The block is below
   ZapFast: check the laptop's microphone-mute key, the "Test your microphone"
   meter in Windows Sound settings, and whether other apps hear the
   microphone. Also set the default input to Microphone Array; Jack Mic only
   works with a headset that has a microphone. Rerun the level test with
   `cargo test --lib -- --ignored --nocapture call_audio::tests::hardware`
   (see NOTES.md for the environment).
2. **macOS is unverified.** The chat header now sits 16 points lower beside
   the list, but the traffic lights are still centred on the old 60-point
   header. Nothing was built or run on macOS or Linux.
3. **The default font changed for every user** in `0ea1230`:
   `FontChoice::GoogleSansFlex` is the default, so settings without a saved
   font switch from the system font. Upstream's rule is to keep existing
   behaviour unless a task changes it; decide whether that is intended.
4. **Voice-message recording** (`src/audio.rs`, `Recorder`) still reads the
   microphone through rodio's blocking reader, so it has the same "a silent
   device hangs the reader" weakness the call path had.
5. **Floating sidebar refinement** (a design brief to make the rail and the
   chat list one coherent floating composition) was started and then
   discarded at the owner's request. The analysis of what makes the rail look
   disconnected is in DECISIONS.md if it is picked up again.
6. Translation catalogs still contain the strings of the removed title bar
   settings. They are unused and harmless.

## Checks last run (Windows only)

On `5c0266e`: `cargo fmt --all --check`, both `cargo clippy` variants with
`-D warnings`, both `cargo test --all-targets` variants (1834 passed, 11
ignored, in each), and `cargo doc` with `-D warnings`. All passed.
