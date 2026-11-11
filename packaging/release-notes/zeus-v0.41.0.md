Zeus 0.41.0 is a big catch-up release: a first-run setup, split view and floating chat bubbles to try, much faster search, features from upstream ZapFast, and motion almost everywhere.

## New

- **First-run setup.** A new copy of Zeus walks you through the look, your name, notifications and privacy before it shows the QR code, with a live preview beside each step.
- **Zeus shows its window while it loads.** Instead of a splash, the outline of the chat list and chat appears at once and fades into the real thing.
- **Split view and a pop-out chat (Experimental).** Open a second chat beside the first, or pull a chat out into its own window. Turn it on in Settings, Experimental.
- **Chat bubbles (Experimental).** Keep chats as floating circles over your other apps and open one as a small chat card.
- **Sent, Delivered and Read labels.** The newest message you sent says where it is in words instead of ticks, and the time shows only after a five minute gap.
- **Block and unblock contacts** from a chat's menu or the contact panel, with a list of blocked contacts in Settings.
- **Disappearing messages.** Set a chat's timer from its menu; the chat says when the timer changes.
- **Link previews on what you send.** A link you type gets its preview card before you send it. The phone's "Disable link previews" setting is respected.
- **Animated Lottie stickers** play in chats.
- **Format from the right-click menu.** Right-click selected text in the message field for Bold, Italic, Strikethrough and Monospace.
- **Groups in common.** A person's info card lists the groups you share, and each one opens its chat.
- **Paste as plain text** with Ctrl+Shift+V (Cmd+Shift+V on a Mac).
- **New chat picks the first match on Enter.**
- **Download folders per chat**, and the tray icon shows the unread count.
- **One theme picker.** Accent colours now work with every theme, and the Glass slider frosts the WhatsApp theme too.
- **A message menu you can trim.** Choose which rows the right-click menu shows in Settings, Message menu, or switch it to icons only.
- **Motion everywhere.** Photos and videos grow out of their bubble and close back into it, a sent message rises out of the message field, messages ripple on springs as you scroll, chat list rows glide when a chat moves up, deleted messages close their gap, and the chat panel has a springy overscroll.
- **The contact panel's tiles** are Call, Video call, Mute and Search, and Media, Docs and Links slide in as pages.
- The benchmark is heavier, records whether the computer is on battery, and has a search phase.

## Fixed

- **Chats keep loading after days away.** A chat with many waiting messages used to make Zeus drop and reconnect over and over, loading a little each time. It now takes the backlog in one go, and reconnects within seconds when the connection is lost.
- **Search is much faster.** Typing waits for a short pause before searching, results come from an index instead of reading every message, and the result list scrolls smoothly. Matching is plain text only, no fuzzy matches.
- **History loads in short runs.** Zeus asks the phone for older messages a little at a time and rests in between, instead of flooding it.
- **Scrolling past messages with many reactions** no longer slows down.
- **The floating bubbles no longer show a thin rectangle around them** on Windows 11.
- Text sizes and spacing follow one scale across the app, and chat list dates are shorter.
- Editing a message keeps its bubble's size until the text outgrows it.
- The send button sits evenly in the message field's rounded end.
- A video call's camera opens on Macs whose cameras list unusual frame rates.
- The app says Zeus, not ZapFast, everywhere you read it.

## Thanks

Thanks to Pratyaksh Mehrotra ([@pratyakshm](https://github.com/pratyakshm)) for settling the motion tests, and to [@RathnamS089](https://github.com/RathnamS089) for the search index and the gentler history loading. Blocking, disappearing messages, link previews, Lottie stickers, text formatting from the menu and groups in common come from upstream ZapFast by Carmine Paolino.

Not tried by us: the Linux and macOS builds. Split view and chat bubbles are experimental and off by default.
