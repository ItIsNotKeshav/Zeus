# Notes

Small practical things learned on this machine (Windows 11, Git Bash and
PowerShell). Commands assume the repository root.

## Building

- Builds go into `target-nmake/`, which already holds compiled
  dependencies. Using another target folder starts from scratch.
  `target-nmake/` and `target-vs2019/` are excluded from git in
  `.git/info/exclude`, not `.gitignore`.
- Debug builds and tests work from Git Bash with:

  ```sh
  CMAKE_GENERATOR="NMake Makefiles" CARGO_TARGET_DIR=target-nmake cargo build --locked
  ```

- **Release builds need the Visual Studio 2019 Build Tools environment and
  Scoop's Perl ahead of Git Bash's**, or OpenSSL's configure fails looking for
  `Locale/Maketext/Simple.pm`. A working script:

  ```bat
  call "C:\Program Files (x86)\Microsoft Visual Studio\2019\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul
  set PATH=C:\Users\kesha\scoop\apps\perl\current\perl\bin;C:\Users\kesha\scoop\apps\perl\current\c\bin;%PATH%
  set CMAKE_GENERATOR=NMake Makefiles
  set CARGO_TARGET_DIR=D:\VSCode\zapfast\target-nmake
  cargo build --release --locked
  ```

- Timings seen: small debug rebuild 10 to 60 s; test build about 1.5 min plus
  about 70 s to run; first release build 13.5 min (OpenSSL, SQLCipher,
  openh264 and Opus from source, NMake running one job at a time, thin LTO
  with one codegen unit). Ways to speed it up: Windows Defender exclusions for
  `target-nmake` and `%USERPROFILE%\.cargo`, Ninja instead of NMake
  (`scoop install ninja`, `CMAKE_GENERATOR=Ninja`), and mise with mbx as
  `mise.toml` intends. mise and mbx are not installed here.
- **A running ZapFast locks its `.exe`.** To build over a running dev copy,
  rename the running file first (`mv target-nmake/debug/zapfast.exe
  target-nmake/debug/zapfast-running.exe`); Windows allows renaming a running
  executable.

## Full checks (AGENTS.md definition of done)

```sh
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets
cargo test --locked --all-targets --all-features
RUSTDOCFLAGS='-D warnings' cargo doc --locked --all-features --no-deps
```

Prefix each with the `CMAKE_GENERATOR` and `CARGO_TARGET_DIR` above.

## Running

- Only one ZapFast runs at a time. `zapfast.exe quit` asks a running copy to
  quit; the installed 0.19.0 original is too old to take that request, so quit
  it from its tray menu.
- Demo screenshots (synthetic data, no account, safe to share):

  ```sh
  target-nmake/debug/zapfast.exe --demo-shot shot.png --demo-page chat,nothing --demo-size 1180x780
  ```

  Pages used here: `chat`, `light`, `nothing`, `nothing-light`, `rail`
  (list hidden), `info`, `chat-search`, `settings`, `calls`, `empty`. Keep
  screenshots out of the repository (`target-nmake/shots/` was used).
  Screenshots are at 1.5x, so divide pixel positions by 1.5 for points.
- After starting the app, the log shows the link state:
  `grep "link: " "$LOCALAPPDATA/paolino/zapfast/data/zapfast.log"`.
  The log is per run; an earlier run's log is gone.

## Microphones

- Level test (opens every microphone for a few seconds, reports frames, peak
  and exact-zero count, keeps no audio):

  ```sh
  cargo test --locked --lib -- --ignored --nocapture call_audio::tests::hardware
  ```

- Devices on this laptop: "Jack Mic (Realtek(R) Audio)" (the Windows default,
  the headphone jack) and "Microphone Array (Intel® Smart Sound Technology for
  Digital Microphones)". Both start slowly: the array gave its first frames
  after about 400 ms, the jack after more than 600 ms.
- Windows records apps that used the microphone under
  `HKCU\Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone\NonPackaged`.

## Rules worth remembering

- Never read message contents, contacts or chat rows from `archive.db` or
  logs. Schema and counts are fine.
- No em dashes in user-facing writing (AGENTS.md).
- One topic per commit; `AGENTS.md` asks for trunk-based work on `main`.
- Dependencies only from crates.io or `github.com/crmne/`.
