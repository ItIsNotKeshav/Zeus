# Architecture

A short map of ZapFast for this fork's work. The authoritative and much
longer description is [`AGENTS.md`](../AGENTS.md); this file is the part
needed to find your way, plus where this branch changed things.

## What it is

A native WhatsApp desktop client for Linux, macOS and Windows, in Rust
(edition 2024, toolchain 1.98.0) with egui 0.36. The WhatsApp protocol,
including calls, comes from the `whatsapp-rust` library; ZapFast never
reimplements it. No browser engine, telemetry or hosted backend. About
164,000 lines of Rust.

## Two halves and one channel

```
 window thread (egui)                    tokio runtime thread (per account)
 ---------------------                   ----------------------------------
 src/ui/*  draws, pushes model::Action   src/backend/worker.rs  owns the
 src/app.rs applies actions after frame  whatsapp-rust Bot, the archive,
                                         downloads, profile pictures
          ---- backend::Command ---->
          <---- backend::Event ------    every Event wakes the window
```

- Views never change application state beyond their own fields; they push
  `Action`s that `App::apply` handles after the frame.
- `src/backend.rs` is the window's handle to one account's runtime.
- Views never touch a protobuf: the worker translates into `src/model.rs`
  types (`classify()`, `parse_conversation()`).

## Accounts and storage

- Several accounts per process. Each `Account` (`src/account.rs`) owns a
  `Backend`. `App` derefs to the account on screen.
- Per account: `state/accounts/<id>/` (`session.db`, `archive.db`, stickers)
  and `cache/accounts/<id>/` (media, avatars). `ChatId` is unique only inside
  one account.
- `src/archive.rs` is SQLite encrypted with SQLCipher; the key is in the OS
  keyring. It keeps each message's raw protobuf because attachment keys live
  there.
- On Windows the folders are `%LOCALAPPDATA%\paolino\zapfast\data` (state and
  the per-run `zapfast.log`), `%LOCALAPPDATA%\paolino\zapfast\cache`, and
  `%APPDATA%\paolino\zapfast\config`.

## Startup and the window

`src/main.rs`: update-helper flags first (`fastframe_update::intercept`),
then the single-instance check (a second launch asks the first to show or
open a chat), folder migration, logging, settings, then
`fastframe_shell::Shell`, which can keep the app alive headless in the tray
after the window closes. Shared plumbing (tray, fonts, emoji, themes,
updates, notifications, scrolling, dock) comes from the maintainer's
`fastframe-*` crates. egui, winit and rodio are pinned forks under
`github.com/crmne/`.

## Layout of the main window

`ui::show` (`src/ui/mod.rs`) draws, left to right:

1. **Rail** (`src/ui/rail.rs`): Chats, Calls, Starred at the top; Archived,
   Locked, Settings and the account avatar at the bottom. With the list
   hidden, the chats' avatars fill it.
2. **Chat list** (`src/ui/chats.rs`): a floating rounded surface since
   `b7b221a`. Its column has a 12-point canvas above and below and a 4-point
   surface margin, which is `chats::HEADER_DROP` (16) above its header.
3. **Conversation** (`src/ui/conversation.rs`, the largest file): header,
   messages, composer. Its header drops by `HEADER_DROP` beside the list
   (`39ade75`).
4. **Right-hand panes**: search (`src/ui/pane.rs`) and info
   (`src/ui/info.rs`), whose headers use `pane::header_margin`.

Below 620 points wide (`ui::NARROW_WIDTH`) the list and the open chat take
turns at the whole window.

Window chrome: Windows and Linux use the desktop's own frame (the drawn title
bar was removed in `7cac3eb`). macOS hides its title bar and centres the
traffic lights on the chat header through `fastframe-titlebar`.

## Theme

`src/theme.rs` owns palettes, fonts and icons; `src/ui/widgets.rs` the shared
controls. Built-in choices (`settings::ThemeChoice`): System, Light, Dark,
Messages, Nothing Dark, Nothing Light, plus custom JSON palettes. Fonts
(`settings::FontChoice`): System, Inter, Geist, Google Sans Flex, Figtree, Plus Jakarta Sans, Manrope, DM Sans. Tests always
lay out in Inter.

## Calls

- `src/calls.rs`: one call's phase machine and media wiring. The call hands
  `whatsapp-rust`'s `voip().call(..).audio(mic_rx, output_tx)` an input
  channel and an output channel.
- `src/call_audio.rs`: the 16 kHz mono bridge between those channels and the
  devices. `AudioInput::spawn` runs `mic_pump`, which opens a `MicReader` on
  its own thread; `read_microphone` converts device audio to 960-sample
  engine frames. Since `5c0266e` the device is read through cpal with a
  timeout, and silent devices are given up (see DECISIONS.md).
- `src/camera.rs` and `src/video_rate.rs`: video capture and bitrate.
- The call screen's device pickers apply to the running call only and are
  not saved.

## Tests and demo

- The `demo` feature builds offline sample chats; `src/demo.rs` holds the
  sample and a headless layout test of every screen.
- `--demo-shot FILE --demo-page PAGES --demo-size WxH` writes a screenshot
  of synthetic content. Never capture real chats.
