# To do

What is still open on Zeus, gathered from contributor pull requests and issues
and from the 2026-10-25 audit. Tick an item off, or remove it, in the commit
that finishes it.

## Issues

- [x] #26: the phone gets background sync alerts about hourly. Favourite
  chats now stop after three failed requests per connection (and start again
  on the next connection or a list from the phone), so a list that never
  validates no longer makes the library ask the phone to rebuild it every 15
  minutes; favourite stickers whose CDN reference is gone rest for a week
  since #51. Close #26 when this merges.
- [ ] #29: spacing, glass tint and doodles. The glass slider and the scrim
  behind bubbles landed in #43. Still open: more vertical room between the
  messages, the reply preview and the composer (#13 tightened the gap above
  the composer, so this is a design call).
- [ ] #55: macOS scrolling stutters from unpaced frames and the glass blur's
  per-frame cost (frames not held to the display's refresh; four frosts run
  every frame while a chat scrolls).

## Audit (2026-10-25), still open

- [ ] `session.db`, which holds the device identity keys, is plain SQLite,
  while `archive.db` uses SQLCipher. Encrypt it the same way, with a
  migration for existing files.
- [ ] Downloads get no Mark-of-the-Web on Windows or quarantine attribute on
  macOS. Add them so the system warns before a downloaded file runs.
- [ ] Decide whether to rotate the update signing key, which upstream's
  triage workflows could read until #35.

Done from the audit: crafted MP4 hang (#37), CI secrets (#35), release
panics (#38), photo memory (#36), the chat id in the album warning.
