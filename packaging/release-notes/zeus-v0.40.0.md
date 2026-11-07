Zeus 0.40.0 brings in Pratyaksh's fork: ZapFast 0.20.1's group calls and group tools, a big cut in memory, Metal on macOS, and smoother pickers and scrolling.

## New

- **Group calls.** Start a voice or video call from a group's header, answer group calls here, and join call links. If WhatsApp has not turned group calling on for linked devices on your account, Zeus says so.
- **Echo cancelling in calls.** Your speakers no longer echo back to the other side, and quiet voices are lifted.
- **Group tools in the info panel.** Admins can add people, share or reset the invite link, change group permissions, and promote, demote or remove members.
- **Metal on macOS.** The Mac app draws with Metal and holds about 90 MB less while its window is open. The chat header and message field are solid there instead of frosted.
- **New messages ease in.** A new message springs into place and the chat glides up to make room for it.
- **Updates show on the rail** instead of in a popup.
- **Open the last chat**, a new setting for starting where you left off. Zeus still opens on its welcome page by default.
- **The benchmark shows CPU per frame**, so a result tells a slow app apart from a slow screen. It now runs in Zeus's own Dark theme with the glass at its clearest.

## Fixed

- **Much less memory.** Zeus keeps only the open chat and the last two you left in memory, frees stickers, GIFs, voice messages and previews as soon as nothing shows them, and holds less while linking a phone, syncing history or sending large files.
- **No more freezes when opening the emoji or sticker picker.** Opening the emoji picker held the window for up to a second, and a shelf of stickers for up to two. Both now open at once and fill in as their pictures are ready.
- **Smoother scrolling past new emoji and stickers.** Each one used to cost the screen a moment the first time it appeared.
- Video calls use less CPU, and calls wait for microphone and camera permission before they start.
- A missed call's time lines up with its label, and you can call back from it.
- Selecting text in a long chat no longer slows Zeus down.
- Light WhatsApp-style bubbles and list rows no longer cast a shadow.

## Thanks

Thanks to Pratyaksh Mehrotra ([@pratyakshm](https://github.com/pratyakshm)) for the fork this release is built on: bringing in ZapFast 0.20.1, the memory work, Metal on macOS, the new message animations, the rail update badge and the Open the last chat setting. Thanks also to everyone behind ZapFast 0.20.1, whose group calls, echo cancelling and group tools came with it.
