Zeus 0.44.1 reads your PDFs, picks your camera, and runs smoother on Windows. Linux users can now get Zeus straight from their package manager, and calls, history and media got a round of fixes.

## New

- **PDFs open right in Zeus.** Click a PDF in a chat and it flies out of its message into a viewer, with search, zoom, Save as and Open in another app. Each card shows the first page, and Zeus remembers where you were in every PDF until you quit.
- **Choose your camera.** Settings now lets you pick the camera video calls use, with a live preview.
- **Richer link previews.** YouTube and Instagram links show their title and picture.
- **Search a chat beside it.** Chat search opens in the info card, so the conversation stays in view.
- **A wallpaper for each chat.** Pick a picture from a chat's menu to give that chat its own wallpaper, over your usual one.
- **Pin for as long as you need.** Pinned messages can stay for 24 hours, 7 days or 30 days.
- **Themes with their own glow.** Catppuccin, Nord, Gruvbox, Solarized and Dracula wash their surfaces in their own colour, and every theme leans toward the accent you pick. The theme list now offers Zeus's nine themes only; palette files are no longer listed.
- **Quieter status.** While Zeus connects or loads history, a small turning sign beside Chats says so instead of a pop-up, and "Copied" is a little pill above where you type.
- **Menus that read at a glance.** Chat menus have icons where they help, smoother hovers, and frosted submenus.
- **A new icon.** Zeus has a fresh bolt Z mark, with sharp small versions.
- **Smoother setup.** Linking your phone looks calmer, an expired code waits for a click to refresh instead of getting stuck, and Start at login is on by default.
- **Livelier chats.** The conversation rises gently when someone starts typing, when you reply or edit, and when the box you type in grows.
- **Newer emoji.** Emoji 18.0 is here, and the picker offers only emoji Zeus can draw.
- **Phone numbers read naturally**, grouped the way each country writes them.
- **Try it: your messages on the left.** Under Settings, General, Experimental, group chats can show your own messages on the left with everyone else's.
- **Zeus on Linux, kept up to date.** Add the Zeus repository once and apt, dnf or zypper bring each new version with your other updates. The commands are at [itisnotkeshav.github.io/zeus-packages](https://itisnotkeshav.github.io/zeus-packages/). The packages are signed, and the RPM now installs on openSUSE too.

## Fixed

- **Smoother on Windows.** Scrolling, the sticker picker and switching chats use less of your processor, as they did before 0.43, so frames stop dropping on battery.
- **No more "Waiting for this message"** in chats with people you had talked to before.
- **Calls behave.** Clicking Accept twice no longer ends the call, and a call that rang out stops ringing here.
- **Quitting never hangs** on an account that is busy.
- **History arrives faster** when you link or scroll back.
- **Albums download as you reach them**, one photo or video at a time.
- **Forwarded mentions keep their names** instead of showing numbers.
- **A contact you add opens the right chat.**
- **M4A audio plays**, even when its file is laid out unusually.
- **Read receipts follow WhatsApp.** The extra Zeus switch is gone: WhatsApp's own Read receipts setting decides.
- **Macs.** Notifications stay after you quit, the Dock icon hides when there is no window, Macs with two graphics chips use the one driving the screen, and the camera is let go after a video call.
- **Keyboard users reach every theme.** Tabbing to a theme in Settings scrolls the page down to it.
- **Small things, tidied.** A sent sticker pops once, tooltips share one style, and the chat card's rounded corners cut its wallpaper and contents cleanly.

## Thanks

Pratyaksh Mehrotra ([@pratyakshm](https://github.com/pratyakshm)) made the livelier chats, per-chat wallpapers, the macOS and onboarding fixes and much of this release. Many fixes come from upstream [ZapFast](https://github.com/crmne/zapfast) by Carmine Paolino ([@crmne](https://github.com/crmne)), with contributions there from Sarmad Durrani, sonthomsen, Goodly58 and unheardpika. Keshav made the PDF viewer, the camera picker, link previews, themes and the Windows and Linux work.

**Full changelog:** [v0.43.0...v0.44.1](https://github.com/ItIsNotKeshav/Zeus/compare/v0.43.0...v0.44.1)
