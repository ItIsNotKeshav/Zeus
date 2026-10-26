# To do

What is still open on Zeus, gathered from contributor pull requests and issues
and from the 2026-10-25 audit. Tick an item off, or remove it, in the commit
that finishes it.

## Contributor pull requests waiting on a decision

- [ ] #44: key the glass blur's GL objects by a weak reference to their
  context, so the frost stops drawing over text and bubbles after the window
  is closed to the tray and reopened. Small, tested, not on main. May need a
  rebase over #42, which also edits `src/ui/glass.rs`.
- [ ] #45: give the unbundled macOS Dock icon a 512 px, 144 dpi image
  (256 points) instead of the 1024 px one, which saves about 30 MB on Retina
  Macs for `cargo run` and the `.tar.gz` build. Small, tested, not on main.

## Issues

- [ ] #26: the phone gets background sync alerts about hourly.
  - Favourite chats retry a snapshot MAC mismatch every 15 minutes forever
    (`favorite_chats.rs`, `settle()`). Stop after a few tries per
    connection, or wait for the next app-state change.
  - Each app-state recovery request goes to the phone. Find what triggers
    them repeatedly and send at most one per connection.
  - Favourite stickers whose CDN reference is gone (403/410) are fetched
    again on every connection. Remember the failures and skip them.
- [ ] #27: contacts and avatars.
  - Names without letters draw a person icon instead of digits: done in #43,
    close #27's avatar half when #43 merges.
  - Contact names for some numbers still do not show. Needs a repro from
    the reporter (which chats, saved or unsaved, group or 1:1). #6 already
    carries names heard under a privacy id to the phone-number contact.
- [ ] #29: spacing, glass tint and doodles.
  - Glass slider from Clear to Solid: done in #43.
  - Scrim behind bubbles so doodles fade before the text: done in #43.
  - Still open: more vertical room between the messages, the reply preview
    and the composer (#13 tightened the gap above the composer, so this is a
    design call).

## Audit (2026-10-25), still open

- [ ] `session.db`, which holds the device identity keys, is plain SQLite,
  while `archive.db` uses SQLCipher. Encrypt it the same way, with a
  migration for existing files.
- [ ] Downloads get no Mark-of-the-Web on Windows or quarantine attribute on
  macOS. Add them so the system warns before a downloaded file runs.
- [ ] `worker.rs` logs the chat id at warn level when an album parent fails to
  send (`could not send an album parent in {chat}`). Drop the chat id or move
  it to debug.
- [ ] Decide whether to rotate the update signing key, which upstream's
  triage workflows could read until #35.

Done from the audit: crafted MP4 hang (#37), CI secrets (#35), release
panics (#38), photo memory (#36).
