---
name: verify
description: Run Zeus and look at a change before committing it. Use after any UI or behaviour change, before a commit or pull request, to build the demo and screenshot the affected screens in the Dark and Light themes.
---

# Verify a Zeus change by running it

A change is verified when you have run the app and looked at the result, not
when it compiles. Tests and CI still run on GitHub; this is the quick smoke
check you do yourself first. Do not run `cargo test` or clippy on the
maintainer's machine (see AGENTS.md).

## 1. Build the demo

The `demo` feature has offline sample chats and needs no account.

- Linux (cloud container):

  ```sh
  sudo apt-get install -y libxkbcommon-dev libwayland-dev libgl1-mesa-dev \
    libasound2-dev libpipewire-0.3-dev clang libclang-dev cmake perl \
    xvfb libgl1-mesa-dri
  cargo build --features demo          # first build ~15 to 35 min, then ~2
  ```

- Windows (maintainer's PC): `cargo build --features demo` from Git Bash, which
  builds into `target-nmake` through the local `.cargo/config.toml`.

## 2. Screenshot the screens you touched

`--demo-shot` stops the clock at a fixed moment, so two builds compare pixel
for pixel. On Linux, run it under a virtual display:

```sh
BIN=target/debug/zeus
for page in chat nothing-light settings setup empty; do
  xvfb-run -a -s "-screen 0 1600x1000x24" \
    $BIN --demo-shot shots/$page.png --demo-page $page --demo-size 1333x780
done
```

Useful pages (`src/demo.rs`, `apply_flags`): `chat`, `nothing` (Dark),
`nothing-light` (Light), `settings`, `settings-search=Notifications`,
`about`, `info`, `rail`, `empty`, `calls`, `call-active`, `login`, `pair`,
`setup`, `setup-look`, `setup-personal`, `setup-notifications`,
`setup-privacy`, `setup-link`, `app-lock`, `new-chat`, `connecting`,
`offline`. Combine with a comma: `--demo-page setup-look,light`.

Always check both the Dark and the Light theme. Name them by theme, never
"Nothing".

## 3. Look at them

Open each PNG and check, against the screen before your change:

- Nothing clipped, overlapping, or misaligned; text not cut off.
- Spacing on the 4 / 8 / 12 / 16 / 24 / 32 scale (`theme::SPACE_*`), type on
  the scale (`theme::TEXT_*`), radii from `theme::RADIUS_*`.
- No new decoration: no gradient glows, tinted icon tiles, or extra cards
  around content that already has a surface.
- Copy says what a control does, plainly. No slogans.

Motion (opening, closing, sending, scrolling) does not show in a screenshot.
Run `target/debug/zeus --demo` to click through it, or say in the pull
request that motion was not seen.

## 4. Report it

Put before and after screenshots in the pull request description or a
comment, never in the repository (`check-review-evidence.py` fails CI on
unused images under `docs/`). Say what you ran and what you only compiled.
