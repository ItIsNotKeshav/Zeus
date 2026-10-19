# Decisions

Why things on this branch are the way they are. Newest first. Each entry
says what was decided, why, and what it costs.

## 2026-10-19: Read the call microphone through cpal with a timeout (`5c0266e`)

- **Decision:** `src/call_audio.rs` reads the call microphone through cpal
  (bundled by rodio as `rodio::cpal`) instead of `rodio::microphone::Microphone`.
  A reader waits at most `MIC_POLL` (100 ms) per sample. A device that sends
  no sample within `MIC_STARTUP` (2 s) of opening is given up; one that was
  sending and stops for `MIC_STALLED` (2 s) is reopened.
- **Fallback order** (`next_microphone`): a picked device falls back to the
  system default (and the call reports the lost choice, as before); a silent
  default moves to the machine's other microphones in the picker's order,
  never one already tried in this call. When all were tried, start again
  from the default after a pause.
- **Why:** rodio's `Microphone::next` blocks on an `mpsc` receiver with no
  timeout. A device that opens and never sends held the reader thread for
  ever, and `MicReader`'s drop joins that thread, so picking another
  microphone mid-call hung too.
- **Not done on purpose:** exact digital zeros are not treated as a dead
  microphone. Noise-suppressing microphones (Intel Smart Sound among them)
  can send true zeros while the room is quiet, and switching away from a
  working microphone because the user was silent would be worse.
- **Honest scope:** this did not fix the reported "they can't hear me" call.
  The level test showed no microphone hearing a voice at all; see HANDOFF.md.

## 2026-10-19: Switch knob contrast (`9912dba`)

- **Decision:** an on switch's knob is white unless white reaches less than
  2:1 against the accent; then it takes `palette.on_accent`
  (`widgets::switch_knob`, `SWITCH_KNOB_CONTRAST`).
- **Why:** Nothing Dark's accent is `#f4f4f4`, so the white knob and the
  accent-coloured check vanished into the track.
- **Why 2:1 and not 3:1:** the default dark theme's green accent reaches only
  about 3.03:1 against white. A 3:1 threshold would sit on that edge and could
  flip the default theme's knob dark. 2:1 changes only accents that are truly
  light.

## 2026-10-19: Line the headers up with the list title (`39ade75`)

- **Decision:** `chats::HEADER_DROP` (16 points: the 12-point canvas above
  the floating list plus its 4-point surface margin) is added to the chat
  header's top margin whenever the window is not narrow. The info and search
  panes use `pane::header_margin`, which centres their 32-point row on the
  chat header's 44-point row and ends level with it.
- **Why:** the floating list (from `b7b221a`) moved the "Chats" title 16
  points down, while the chat header still started at the window's top, so
  the open chat's picture, name and call buttons sat above the title line.
  The owner asked for them to share one line.
- **Narrow windows** keep the old tight header: the chat is alone there and
  has nothing to line up with.
- **Test change:** `demo::tests::both_headers_share_their_first_row` (from
  Codex) asserted the list header sat at least 12 points below the chat
  header, which was the bug. It now asserts they share their top, within one
  point, because the list surface's 0.75-point hairline adds to its margin.

## 2026-10-19: Remove the custom title bar entirely (`7cac3eb`)

- **Decision:** removed the Custom title bar and Auto-hide title bar
  settings, `Action::SetCustomTitlebar`, the drawn caption buttons and their
  reveal animation, `ui::caption_room` / `CaptionRoom` / `caption_drag`,
  `theme::caption_footprint` / `drawn_titlebar` / `preview_titlebar`,
  `pane::caption_header`, the `--demo-titlebar` flag, and the layout tests
  that only checked room around the drawn buttons.
- **Why:** the owner chose "the whole custom title bar" over removing only the
  auto-hide part.
- **Kept:** `fastframe-titlebar` stays a dependency because macOS uses it for
  the traffic lights (`align_traffic_lights`, `drag`, `footprint`).
  `theme::chrome_in_headers` is now macOS only.
- **Compatibility:** `Settings` does not deny unknown fields, so settings
  files that still carry `custom_titlebar` or `auto_hide_titlebar` load and
  ignore them.
- **Docs updated:** `docs/_reference/settings-and-files.md`, `AGENTS.md`,
  `DEMO.md`.

## 2026-10-19: Discard the floating sidebar refinement

- **Decision:** a refinement of the rail and chat list was implemented and
  screenshotted, then discarded at the owner's request before the title bar
  work.
- **What it found**, in case it is picked up again:
  - The rail used `Palette::rail()`, the list's colour one shade darker. With
    the rail floating on the window colour, that shade nearly matched the
    window (`#f2f2f2` on `#f0f2f5` in light), so the rail read as a grey strip
    with a hairline while the list was a raised white card.
  - Rail outer margins were 8/4/12/4 and the list's 4/8/12/12, so their
    bottoms did not line up, and with the list hidden only 4 points separated
    the rail from the conversation.
  - The selected destination used `widgets::highlight_card` (shadow, raised
    edge, fill), heavy for a 32-point plate, and hover differed only in grey.
  - The narrow-window list painted `palette.panel` behind itself, unlike the
    window colour around the rail.
- **What the discarded version did:** one material and hairline for both
  panels (`widgets::floating_panel`), a shared `CANVAS_GAP` of 8, rail radius
  16 against the list's 12, flat tinted plates with an accent mark beside the
  lit destination (shared with the collapsed list's open-chat mark), and
  48-point cells top and bottom.

## Earlier, by Codex (`0ea1230` to `b7b221a`)

Recorded from the commits, not decided in the Claude Code session:

- Nothing Dark and Nothing Light palettes with `BubbleStyle::Nothing` and
  translucent surfaces; both count as vibrant and request a transparent
  window like Messages.
- Bundled Geist and Google Sans Flex (OFL), with Google Sans Flex made the
  default font (see HANDOFF.md, open issue 3).
- The chat list as a floating rounded surface beside a floating rail.
- Ctrl+Tab between the two most recently opened chats.
- A 540-line design token section in `DESIGN.md`.
