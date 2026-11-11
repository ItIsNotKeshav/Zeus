# Zeus user guide

This guide is for people who have just installed Zeus. It covers what Zeus
does, how to install and link it, and how to use it day to day. It describes
the code on `main` at release 0.41.0; where a feature is still experimental or
has a known gap, the guide says so.

Zeus is a desktop WhatsApp client for Windows, macOS and Linux. It links to
your phone as a companion device, the way WhatsApp Web does, and keeps your
chats in an encrypted archive on your own computer. Your phone stays the main
account.

> **Unofficial client.** Zeus is not affiliated with, endorsed by or connected
> to WhatsApp or Meta. WhatsApp can limit accounts that use unofficial clients,
> so use Zeus at your own risk.

## Contents

1. [Install Zeus](#1-install-zeus)
2. [First run](#2-first-run)
3. [Link your phone](#3-link-your-phone)
4. [Message history](#4-message-history)
5. [The window](#5-the-window)
6. [Send and read messages](#6-send-and-read-messages)
7. [Attachments, voice messages and stickers](#7-attachments-voice-messages-and-stickers)
8. [Chats, search and organising](#8-chats-search-and-organising)
9. [Groups](#9-groups)
10. [Calls](#10-calls)
11. [Several numbers](#11-several-numbers)
12. [Experimental: split view and bubbles](#12-experimental-split-view-and-bubbles)
13. [Settings](#13-settings)
14. [Notifications and the tray](#14-notifications-and-the-tray)
15. [Keyboard shortcuts](#15-keyboard-shortcuts)
16. [Links](#16-links)
17. [Your data and privacy](#17-your-data-and-privacy)
18. [Updates](#18-updates)
19. [Troubleshooting](#19-troubleshooting)
20. [Known limitations](#20-known-limitations)
21. [Getting help and contributing](#21-getting-help-and-contributing)

---

## 1. Install Zeus

Download the package for your system from the
[Zeus releases page](https://github.com/ItIsNotKeshav/Zeus/releases/latest).
Each release also includes a checksum file (`checksums.txt`) you can use to
verify the download.

| System | What to download | Notes |
| --- | --- | --- |
| **Windows** | `zeus-v<version>-x86_64-pc-windows-msvc-setup.exe` | Installs for your user only, without administrator rights, and keeps itself up to date. The installer is not code-signed, so SmartScreen warns once: choose **More info**, then **Run anyway**. |
| **macOS** | `.dmg` for your Mac | One universal build for Apple silicon and Intel. The app is not signed, so macOS may ask you to allow it under **System Settings > Privacy & Security** the first time. |
| **Linux** | `.AppImage`, `.deb`, `.rpm` or `.tar.gz` | Pick the one for your distribution, and `x86_64` or `aarch64` for your processor. The `.tar.gz` contains the `zeus` program and a desktop entry you can copy into place. |
| **From source** | The repository | Install Rust, then run `cargo build --release --locked`. You need a C/C++ toolchain, CMake and Perl. On Linux you also need ALSA, PipeWire with clang headers, and the development packages listed in `CONTRIBUTING.md`. |

The maintainer tests the Windows build. The macOS and Linux builds are produced
by automated builds and have had less testing, so please report anything that
does not work.

Zeus runs beside ZapFast, the app it is forked from. It has its own folders,
settings and linked device, so both can be installed at once.

## 2. First run

The first time Zeus starts, it shows a short setup before the QR code. Each
step has a live preview beside it.

1. **Welcome.** Choose **Get started**, or **Skip to linking** to go straight
   to the QR code.
2. **Choose your look.** Light or dark, and one of the built-in themes.
3. **Make it yours.** An accent colour, a typeface and the doodles behind your
   chats. You can change all of these later in **Settings > Appearance**.
4. **Stay in the loop.** Desktop notifications, whether to show message
   previews, whether to play a sound, and whether Zeus keeps running in the
   tray when you close the window.
5. **Private by default.** Read receipts and typing indicators. Both are
   yours to choose, and you can change them later.

After setup, Zeus shows the link screen.

## 3. Link your phone

Zeus links to your WhatsApp account as a companion device. You need WhatsApp
on your phone.

1. On the link screen, open WhatsApp on your phone. Go to **Settings** (or
   **Menu** on Android), then **Linked devices**, then **Link a device**.
2. Scan the QR code on the screen with your phone.
3. If you cannot scan the code, choose **Link with phone number instead** in
   Zeus. Enter the eight-character code it shows into WhatsApp.

The link survives restarts. Your phone does not need to stay online to read
messages that Zeus has already stored. You can see and remove this computer
at any time under **Linked devices** on your phone.

## 4. Message history

After linking, your phone sends its recent history. The chat list appears
within seconds, and messages can take a few minutes to finish loading.

Zeus stores every message it receives in its own archive on this computer.
When you scroll past what is stored, Zeus asks your phone for older messages.
Your phone must be online for this. You can also turn on **Fetch older
history** in **Settings > Chats** to let Zeus ask in the background, a few
messages at a time.

## 5. The window

From left to right:

- **The rail.** A thin strip with **Chats** and **Calls** at the top, and
  **Starred**, **Archived**, **Locked** (when you have chats in them),
  **Settings** and your own picture at the bottom. Your picture is the account
  switcher. Press `Ctrl+B` to fold the chat list into the rail.
- **The chat list.** Your chats, newest activity first. Pinned chats stay at
  the top. The search bar and filter chips sit above the list.
- **The conversation.** Messages, with the composer at the bottom.
- **The info card.** Click a chat's header, or choose **Info** in its menu, to
  see its picture, members or number, its actions, and its photos, videos,
  documents and links.

If your window is narrow, parts of this fold away. That is intentional.

## 6. Send and read messages

**Sending.** Type in the composer and press **Enter** to send. Press
**Shift+Enter** for a new line. If you prefer Enter to add a line, turn off
**Enter sends** in **Settings > Chats**; then Ctrl+Enter (Command+Enter on
macOS) sends.

**Formatting.** Use WhatsApp's marks: `*bold*`, `_italic_`, `~strike~` and
` ```monospace``` `. A message made only of emoji shows them large.

**Mentions.** In a group, type `@` and choose a member. Choose **all** to mention
everyone when the group's member list is available.

**Replies.** Double-click beside a message, or on its edge, to reply to it.
A double-click on the text itself selects a word instead. If **Touchpad
swipes** in Settings is set to reply, a two-finger swipe on a message also starts
a reply.

**Right-click menu.** Right-click a message to react with an emoji, edit it,
copy its text, star or pin it, save its attachment, see its info, or delete it.
Pick **Edit** to change your own message; press Escape to cancel. The reaction
row has a **+** that opens the full emoji picker. Hover over a reaction to see
who added it.

**Select several messages.** Choose **Select messages** from the chat's menu
(the three dots at the top), or Ctrl-click (Command-click on macOS) a message.
Tick the messages you want. From the bar under the chat you can **Forward…**
them in their original order or **Delete…** them.

**Forward.** Select messages and choose **Forward…**, or drag a message onto a
chat in the list. You can forward to up to five chats at once.

**Delete.** Right-click a message and rest on **Delete**. You can choose
**Delete for me**, or **Delete for everyone** while your own message can still
be revoked. Either asks you to confirm. Deleting for everyone revokes the
message through WhatsApp; deleting for me removes it from this computer and
from your other linked devices.

**Star and pin.** Star a message to find it later in **Starred** in the rail.
Pin a message for everyone in the chat. WhatsApp keeps a pin for seven days,
and a chat can hold three pins at a time.

**Delivery labels.** The newest message you sent says whether it was **Sent**,
**Delivered** or **Read**. The time shows below a message after a short gap.

**Copying.** Select text with the mouse and copy as usual. To copy whole
messages, select them and press `Ctrl+C` (`Cmd+C` on macOS). The clipboard gets
the messages in chat order, with the same name and time format WhatsApp uses
when sharing.

## 7. Attachments, voice messages and stickers

**Attaching.** Click the paperclip, paste a picture or copied files with
`Ctrl+V`, or drop files onto the window. Attachments stay above the composer
until you send them. Type a caption in the composer; it goes with the first
file.

**Previews and progress.** A sent attachment appears in the chat straight
away, with a progress ring while it uploads.

**Sending a picture as it is.** Pictures sent normally become photos, which
WhatsApp compresses. To keep a picture's transparency and metadata, use
**Send as document** in the plus menu beside the composer.

**View once.** With exactly one JPEG, PNG or WebP picture staged, use the
**View once** icon beside Send. It is not available for documents or several
files together.

**Crop.** Click a picture above the composer to crop it. Your original file is
not changed.

**Albums.** Pictures and videos sent as one WhatsApp album appear together in
a grid. **Download all** and **Save all…** work on the whole album.

**Downloads.** Files up to 64 MiB download when they come into view, if
**Download files automatically** is on. Larger files download only when you
click them. A failed download can be retried by clicking it again. If an
attachment has expired, Zeus asks your phone to upload it again.

**Viewing.** Click a photo to open it full-window, with zoom and **Copy
image**. Use the arrow keys to move between the pictures in a chat. Videos play
in their message, and H.264 MP4 files play inside Zeus; other formats open in
your system player.

**Location.** Use **Send location** in the plus menu. Paste coordinates such as
`-23.5505, -46.6333` or a Google Maps, Apple Maps or OpenStreetMap link. Zeus
reads the text on this computer and does not look up place names.

**Polls.** Choose **Create poll** from the plus menu. A poll takes two to
twelve answers, and **Allow multiple answers** can be turned off. Click an
answer to vote, and click it again to take the vote back.

**Voice messages.** When the composer is empty, the send button becomes a
microphone. Press Enter or the send button to send the recording, or Escape or
the delete button to discard it. To play a voice message, click it. The
speed chip cycles 1x, 1.5x and 2x. Right-click a voice message to choose
another speed, or give one chat its own speed from its menu in the chat list.

**Stickers and GIFs.** The smiley beside the composer opens emoji (searchable),
GIFs and stickers. Stickers are organised in tabs: **Recent**, **Favorites**,
your packs and **Received**. Right-click a sticker to add it to Favorites.
Under **+** you can import a Signal sticker pack from a `signal.art` link, or
open a `.wastickers` file. You can also make a sticker from a picture.

## 8. Chats, search and organising

**New chat.** Press `Ctrl+N`, or use the **New chat** button above the list.
To message a number you have not saved, choose **Add contact**, enter the
number with its country code, and choose **Message**. Zeus checks with
WhatsApp that the number is registered before it opens the chat.

**Search.** Press `Ctrl+K` (or `Ctrl+Shift+F`) to search chats, messages and
contacts. Results include message matches across everything stored on this
computer. Press `Ctrl+F` inside a chat to search only that chat.

**Filter chips.** Under the search bar, the chips show **Unread**, **Private**,
**Favorites**, **Groups**, **Channels** and any labels you have made. Click an
active chip again, or **All**, to see everything. Filters apply only to the
list, not to search.

**Chat menu.** Right-click a chat in the list to pin, favourite, archive, mark
as unread, or mute it for eight hours, one week or indefinitely. These changes
also apply on your phone. From the same menu you can delete a chat or lock it.
**Delete chat** and **Clear chat** need a connection: the phone acts first, and
the chat leaves this computer only after WhatsApp confirms.

**Archived.** Chats you archive are kept in **Archived**, in the rail. Press
`Ctrl+E` to archive or unarchive the open chat. Zeus follows your phone's
**Keep chats archived** setting, so an archived chat can come back to the list
when a new message arrives.

**Locked chats.** Choose **Lock chat** from a chat's menu to move it into a
locked folder. It disappears from the list, search and unread count, and
never notifies. Open the **Locked** tab in the rail and enter your local code.
The first time, Zeus offers to set one up. The code protects the screen only;
it does not add encryption beyond the encrypted archive. Locked chats are
read-only in Zeus for now.

**Labels.** Labels are yours alone and stay on this computer. Open **Labels**
in a chat's menu, then **Manage labels…**, to make up to twenty. A chat can
wear several labels, and each label becomes a filter chip.

**Disappearing messages.** A chat's info shows its timer. Messages you send
follow it. Messages you receive stay in Zeus's archive after they disappear on
your other devices.

**Blocking.** Block or unblock a contact from a chat's menu or from the contact
panel. Blocked contacts are listed in **Settings**.

**Message yourself.** Use WhatsApp's **Message yourself** chat to try messages,
reactions, edits, voice messages and attachments privately.

## 9. Groups

Choose **New group** from the **New chat** dialog. Search for and tick each
contact, choose **Next**, name the group (up to 100 characters), optionally add
a photo, and choose **Create**. You become the group's admin.

Some people's privacy settings stop them being added to groups. Zeus names
anyone WhatsApp will not add, and you can send them the group's invite link.

If you run a group, its info panel also lets you:

- **Invite via link.** Copy the group's link, or **Reset link** to turn the old
  one off. Anyone with the link can join, so share it only with people you
  would add yourself.
- **Add members**, and **Group permissions** for who can edit the group, send
  messages, add members and approve new members.
- **Make group admin**, **Dismiss as admin** and **Remove from group** from a
  member's **…** menu.

Clicking a `chat.whatsapp.com` invite link shows the group and lets you join it
without leaving Zeus.

## 10. Calls

A one-to-one chat has voice and video buttons in its header. While a call is
going on, a hang-up button takes their place.

- **Outgoing.** The screen says **Connecting…**, then **Calling…**, then
  **Ringing…** when the other phone rings. Hanging up before then cancels the
  call.
- **Incoming.** A call takes over the window with the caller's picture and
  **Accept** and **Decline**. It also rings and raises a notification until you
  answer, decline, or it stops.
- **During a call.** The window shows the call length, mute, the camera
  button, the microphone and speaker pickers (the gear button) and hang up. The
  arrow in the top corner puts the call aside so you can keep using chats.
  Only hanging up ends the call.
- **Push to talk.** Off by default. Turn it on in **Settings > Calls and
  voice**, then hold the key (Space by default) to talk.
- **Microphone.** Pick the microphone in **Settings > Calls and voice**. The
  system default is used unless you choose another. **Check your microphone**
  shows how loud it hears you without recording anything.
- **Call history.** Calls are filed in the chat, and the **Calls** tab
  (`Ctrl+Shift+2`) lists them with **All** and **Missed** filters.

**Group calls are experimental.** Start one from a group's header, or join from
a call link in a message. Group video can stop for a moment every few seconds
in testing.

## 11. Several numbers

You can link more than one WhatsApp number to Zeus and switch between them in
one window. Click your picture at the bottom of the rail, or press
`Ctrl+Shift+A` for the next number. A dot on your picture means another number
has unread chats. Each number keeps its own keys, archive, media, notifications
and wallpaper.

To add a number, use **Settings > Account > Add account**, or the account
switcher. Calls work on one number at a time.

## 12. Experimental: split view and bubbles

Two features are in **Settings > Experimental**, and both are **off** by default.

- **Split view.** Drag a chat from the list onto the open chat to read two
  side by side. You can also choose **Pop out chat** from a chat's menu to open
  it in its own small window above the others.
- **Chat bubbles.** Keep up to three chats as small circles floating over your
  other windows. Click one to open a small chat card where you can reply, and
  attach, paste or drop pictures and files. Start one with **Open as bubble** in
  a chat's menu.

## 13. Settings

Open Settings with `Ctrl+,` or with **Settings** at the bottom of the rail. Changes take
effect immediately. Use the search field at the top (`Ctrl+F`) to find a
setting by name. The sections are:

- **Appearance.** Colours (a custom look, or one of twenty built-in themes, each
  in light and dark), **Blur** for the frosted panels, **Always show time**,
  wallpaper and doodles, zoom, and the interface **Language**. Zeus is available
  in English and ten other languages; **Auto** follows your system.
- **Chats.** **Enter sends**, **Open the last chat** (off by default), **Download
  files automatically**, **Fetch older history**, **Keep chats archived**, and
  the locked-chats code.
- **Touchpad swipes.** What a two-finger swipe does on a message or in the chat
  list.
- **Calls and voice.** Microphone, **Check your microphone**, **Push to talk**
  and its key.
- **Notifications.** Desktop notifications, **Show message content in
  notifications** (on by default; turn it off to show only "New message"),
  message and mention sounds, group sounds, and **Limit repeat notifications**.
- **Privacy.** Read receipts, typing indicators and what other people can see.
- **System.** Start at login, keep running in the tray, and related options.
- **Account.** Your linked numbers, and adding another.
- **Files.** Where downloads go, how much space the media cache uses, and
  **Clear…** to free it.
- **Experimental.** Split view and chat bubbles.
- **About.** The version, **Keyboard shortcuts**, the source code link, and
  **Check for updates**.

## 14. Notifications and the tray

**Closing the window keeps Zeus running** in the tray, so messages keep
arriving. Click the tray icon to reopen the window (on macOS, use the menu-bar
icon and choose **Show or hide Zeus**). Starting Zeus again also reopens the
window.

**To quit,** use **Quit** in the tray menu, **Quit** in the Dock menu on macOS,
or `Ctrl+Q`. You can also run `zeus quit` from a terminal.

**Badges.** Zeus shows the unread count on the taskbar or Dock where your
system supports it. Windows shows it as a small overlay on the taskbar button
while the window is open.

**Clicking a notification** opens the chat at the message it announced.

**Quiet by design.** A chat you are looking at does not notify, and muted chats
stay silent. A chat that is minimised, fully covered by other windows, or hidden
with the app does notify. In busy chats, **Limit repeat notifications** sends one
notification and then waits ten minutes, or until you read the chat.

## 15. Keyboard shortcuts

Press `Ctrl+/` (or `?` when you are not typing) to see this list in Zeus. On
macOS, Ctrl becomes Command.

| Shortcut | What it does |
| --- | --- |
| `Ctrl+K` or `Ctrl+Shift+F` | Search chats |
| `Ctrl+F` | Search the open chat, or the settings |
| `Ctrl+L` | Focus the message box |
| `Ctrl+N` | New chat, or message yourself |
| `Ctrl+Tab` | Switch between the last two chats |
| `Alt+↑` / `Alt+↓` | Previous / next chat |
| `Ctrl+Shift+[` / `]` | Previous / next chat, as in WhatsApp |
| `Ctrl+1` … `Ctrl+9` | Open the chat at that place in the list |
| `Ctrl+Shift+1` / `Ctrl+Shift+2` | Chats / Calls |
| `Ctrl+E` | Archive or unarchive the open chat |
| `Ctrl+B` | Collapse or expand the chat list |
| `Ctrl+Shift+A` | Switch to the next linked number |
| `Ctrl+End` | Jump to the newest message |
| `PgUp` / `PgDn` | Scroll the open chat by a page |
| `Home` / `End` | Top / bottom of the open chat (when the box is empty) |
| `↑` | Edit your previous message (when the box is empty) |
| `Enter` | Send (`Shift+Enter` for a new line) |
| `Escape` | Close the current panel, search or chat |
| `Ctrl+V` | Paste text, or stage a picture from the clipboard |
| `Ctrl+C` | Copy the selected messages |
| `Ctrl+,` | Settings |
| `Ctrl++` / `Ctrl+-` / `Ctrl+0` | Zoom in / out / reset |
| `←` / `→` | Previous / next item in the media viewer |
| `Ctrl+Shift+L` | Lock Zeus with the app lock password |
| `Ctrl+W` | Close the window (Zeus stays in the tray) |
| `Ctrl+Q` | Quit |

## 16. Links

Zeus registers the `whatsapp:` and `wa:` link schemes, so a link that names a
chat opens that chat in Zeus. You can also run `zeus` from a terminal:

```sh
zeus +20123456789
zeus 'whatsapp://send?phone=20123456789'
zeus 'https://wa.me/20123456789?text=Invoice%20attached'
zeus 'https://chat.whatsapp.com/AbCdEf1234567890XyZ'
```

A number, a `wa.me` or `api.whatsapp.com` link, or a `whatsapp://send?phone=`
link all open the same chat. A `text=` message in a link lands in the composer
without being sent, and never replaces a draft you have already written.

A plain `https://wa.me/...` link opened in a browser still goes to WhatsApp Web.
Use the `whatsapp:` form, or `zeus`, to open Zeus instead.

## 17. Your data and privacy

Zeus has no telemetry and no Zeus-operated server. Messages go between your
computer and WhatsApp's servers. The only other outside request is a daily
check of GitHub for a newer release, which you can turn off in Settings.

**Where your data lives.** Each linked number has its own folder, so files and
keys never mix between numbers.

| What | Linux | Windows | macOS |
| --- | --- | --- | --- |
| Settings | `~/.config/zeus/settings.json` | `%APPDATA%\paolino\zeus\config` | `~/Library/Application Support/me.paolino.zeus` |
| Session keys and archive | `~/.local/state/zeus/accounts/<id>/` | `%LOCALAPPDATA%\paolino\zeus\data` | `~/Library/Application Support/me.paolino.zeus` |
| Attachments and avatars (safe to delete) | `~/.cache/zeus/accounts/<id>/` | `%LOCALAPPDATA%\paolino\zeus\cache` | `~/Library/Caches/me.paolino.zeus` |

Inside an account folder, `session.db` holds the keys that link Zeus to your
phone, and `archive.db` holds your message history. The archive is encrypted
with a key kept in your system's credential store: Secret Service (GNOME
Keyring or KeePassXC) on Linux, Keychain on macOS, and Windows Credential
Manager on Windows.

**Coming from ZapFast.** On first start, Zeus copies your ZapFast settings once.
It does not share ZapFast's linked session or archive, so Zeus links to your
phone as a separate device and scans a new code.

**Backing up.** Back up the archive *and* its keyring entry. Copying `archive.db`
alone to another computer is not enough. Deleting the archive, or creating new
keyring entries, does not restore access to an existing archive.

**Unlinking.** Unlinking from Settings tells your phone to forget this device
and deletes the local archive and caches. Neither can be undone.

**Full-disk encryption.** Only the archive is encrypted by Zeus. Session keys,
downloaded media, stickers and settings are ordinary files, so use full-disk
encryption for those, and for swap and backups.

## 18. Updates

Zeus checks GitHub once a day for a newer release and offers it in the rail.
**Restart to update** installs it. You can check at any time with **Check for
updates** in Settings. If an update does not install on your system, download the
release from the [releases page](https://github.com/ItIsNotKeshav/Zeus/releases/latest)
and install it by hand.

## 19. Troubleshooting

**The window says the keyring is locked.** Unlock your system's keyring, then
click **Retry**. Zeus keeps the archive intact and waits. It never creates a
plaintext copy.

**Old messages are missing.** Your phone has to be online for Zeus to fetch
older history. Attachments that have expired can be re-uploaded only while your
phone is online.

**A file will not download.** Click it to retry. Larger files take longer and
time out after a while, when you can retry.

**Zeus says a copy is already running.** Launching Zeus again brings the
existing window forward. If an older ZapFast copy is running, quit it first.

**The archive cannot be read after a reinstall or move.** Restore the original
keyring entry or profile location if you can. If the key is gone for good,
**Start over…** keeps the unreadable archive as `archive-unreadable-<date>.db`,
unlinks the session and shows the link screen. Linking again brings back recent
history from your phone. Afterwards, remove the old Zeus entry under **Linked
devices** on your phone.

**Reporting a problem.** Use the bug form on GitHub. Include your operating
system, the Zeus version, the exact steps, and the error text, with personal
paths removed. Never attach your archive, keys, QR codes, or full logs from older
releases. Logs live in the state folder listed above, and `--verbose` writes more.

## 20. Known limitations

These are current gaps, not permanent decisions.

- **Unsupported message types.** Forms, payments, shopping flows and some
  business templates need WhatsApp on your phone. Zeus shows a note on them.
  Embedded videos and documents in templates also need another client.
- **Interactive messages cannot be forwarded** from Zeus yet.
- **Polls cannot be created in chats with disappearing messages.**
- **Locked chats are read-only** in Zeus.
- **Group calls are experimental.** Group video does not yet recover from lost
  packets the way one-to-one calls do.
- **Calls.** On some Windows setups the microphone can be silent in a call. Check
  the microphone mute key, the system's microphone test, and the default input.
- **Platforms.** The maintainer tests Windows. macOS and Linux builds are less
  tested.
- **Split view and bubbles** are experimental and off by default.

Zeus will never offer bots, scripting, an API or automatic replies. This is a
deliberate product boundary, because WhatsApp's terms forbid automation.

## 21. Getting help and contributing

- **Issues and feature requests.** Open them on the
  [Zeus issue tracker](https://github.com/ItIsNotKeshav/Zeus/issues). Search first.
- **Contributing.** Read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull
  request. Keep each pull request to one change, and include tests and
  before-and-after screenshots, captured in demo mode, for any visual change.
- **Privacy in reports.** Never post screenshots of real conversations, contact
  names, phone numbers, keys or QR codes. Use demo mode, or crop and redact.
- **Developers.** [AGENTS.md](AGENTS.md) describes the architecture and the
  project's conventions. [DESIGN.md](DESIGN.md) describes the visual design.
