# Handoff

Where the work on this fork stands, for whoever picks it up next (a person
or an agent). Read this first, then [DECISIONS.md](DECISIONS.md) for why
things are the way they are, [ARCHITECTURE.md](ARCHITECTURE.md) for how the
app fits together, and [NOTES.md](NOTES.md) for commands and gotchas.

Last updated: 2026-10-19.

## Repository

- Fork: `github.com/ItIsNotKeshav/zapfast` (remote `origin`). Upstream is
  `github.com/crmne/zapfast`.
- Working branch: `codex/nothing-themes-fonts-auto-titlebar`, cut from this
  fork's `main` at `d1fdf55` ("Move fastframe to 0.10.0"). The branch is not
  behind `main`.
- **Nothing on this branch since `b7b221a` is pushed.** `origin` has the
  branch up to `b7b221a`; the four commits after it exist only locally.
- The project's own rules live in [`AGENTS.md`](../AGENTS.md). It asks for
  trunk-based work on `main`; this fork has been using a feature branch.

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
