# Zeus design system

Zeus is a native desktop messenger drawn with Rust and egui. Its look is calm, monochrome by default, and as round as iOS 26's Liquid Glass: content sits on opaque ground, and only what floats for a moment (menus, dialogs, pickers, the header and composer over the conversation) is frosted glass. Use these rules to build anything that should feel like Zeus, in egui or on the web.

Browse it with live previews of every component and all eight palettes on the [Zeus Design System page](https://claude.ai/artifact/BLKeoMCbELWAYahECTc7ZQ). The values here are copied from `src/theme.rs`, `src/ui/widgets.rs`, `src/ui/controls.rs` and `src/ui/glass.rs`; the code stays the source of truth, and [`DESIGN.md`](../DESIGN.md) keeps the workflow for changing the app's UI.

## Principles

- **Content first, glass on top.** Messages, lists and settings sit on opaque `window`, `panel` and `chat`. Glass (`menu-glass`, `glass`, `header-veil`) is only for layers that float over content and go away. Controls inside content are flat tinted capsules (`tint-field`), never glass.
- **Monochrome until you choose a colour.** Dark's accent is near-white `accent` on near-black, Light's is near-black on white. Colour comes from the user (the Red, Blue, Green, Pink, Purple and Yellow themes, `accent-*`) or from status (`danger`, `warning`).
- **Round, and nested.** Large surfaces are very round (`radius-dialog` 28, `radius-panel` 24, `radius-menu` 20). A shape inside a rounded surface takes the outer radius minus the room between them, so curves run parallel: a menu item's `radius-medium` (14) inside a menu's 20 with its 6 margin.
- **Bubbles stay tighter.** Message bubbles keep `radius-bubble` (10) or `radius-bubble-grouped` (18). Rounding them to panel radii made them read as cards.
- **Generous, not loose.** Use the spacing scale; prefer fewer repeated values. A messenger needs density in the chat list and breathing room in dialogs and settings.
- **Everything moves, briefly.** Every new or changed element (popup, panel, button, state change) eases in and out with an existing duration and curve from Motion. Nothing jumps; nothing lingers.
- **State by shape as well as colour.** Switches change knob size and add a check; unread rows show a badge and an accent timestamp; selected rows get a plate. Never colour alone.

## Content and copy

- Write in plain sentence case: "Show previews", "Delete chat". No Title Case, no all caps, no exclamation marks.
- Buttons are verbs that say what happens. The destructive button names the thing: "Delete chat", "Leave group".
- Descriptions say the consequence: "Messages are removed from this device. People in the chat still have them."
- Toasts are two or three words: "Link copied", "Message deleted".
- Address the person as "you"; the app does not speak as "I" or "we".
- Name the default themes **Dark** and **Light**. The others are WhatsApp Dark, WhatsApp Light, Messages and Halloween. The colour themes are named by their colour.
- No emoji in interface copy. Emoji belong to messages and reactions.

## Colour

Use roles, never raw values. Every widget takes its colours from the palette (`Palette` in egui, `var(--name)` on the web).

| Role | Tokens | Rule |
| --- | --- | --- |
| Ground | `window`, `panel`, `chat` | `window` behind everything; `panel` for the chat list, settings rail and sidebars; `chat` behind bubbles |
| Surfaces | `surface`, `surface-hover`, `surface-active`, `overlay` | `surface` for inline fields and cards on the panel; hover and active are interaction feedback; `overlay` is the solid fill of floating surfaces when Blur is off |
| Text | `text`, `secondary`, `dim` | `text` for primary content and icons; `secondary` for descriptions, menu icons and times; `dim` for disabled and lowest-priority meta. All three meet 4.5:1 on `window`, `panel`, `surface` and menus in every theme |
| Accent | `accent`, `accent-hover`, `on-accent` | Primary buttons, switches, badges, focus rings, the text cursor. Draw on it with `on-accent` only |
| Status | `danger`, `warning` | Always with a word or icon |
| Chat | `bubble-in`, `bubble-out`, `link`, `read` | Bubbles and their contents. On a bubble, re-derive `secondary`, `dim` and `read` so they keep 4.5:1 (3:1 for ticks): `Palette::on_bubble` |
| Glass | `menu-glass`, `glass`, `header-veil`, `glass-edge` | See Elevation and glass |
| Tints | `tint-field`, `tint-field-lift`, `tint-hover`, `tint-selected`, `selection` | See-through washes of `text` (or `accent` for `selection`) so the material shows through |

A custom accent goes through `Palette::with_accent`: the switch and button take it raw (on light themes, darkened until it has 3:1 on `panel`); `on-accent` is dark on a light accent (luma above 150) and white otherwise; own bubbles wash `bubble-in` toward it only as far as `text` keeps 4.5:1; links are lightened (dark themes) and then pushed to 4.5:1 on `chat`.

### Themes

Eight palettes ship. Dark is the default; Follow system picks Dark or Light.

| Theme | Character |
| --- | --- |
| Dark | Near-black `window` #080808, neutral greys, near-white accent, grouped translucent bubbles, glass everywhere it floats |
| Light | Paper white, `window` #f7f7f7, near-black accent, white incoming bubbles |
| Red, Blue, Green, Pink, Purple, Yellow | Dark with `accent-red` … `accent-yellow` |
| WhatsApp Dark, WhatsApp Light | WhatsApp's teal-green palette, tailed bubbles lifted by a shadow, opaque surfaces |
| Messages (dark and light) | macOS greys, system blue, Apple's balloon shape and blue gradient |
| Halloween (dark and light) | Dark's glass in plum and ember, or Light's in candlelit cream, with pumpkin `#ff7a1a` |

Known contrast exceptions, kept on purpose: white on the Messages accent and own bubbles is Apple's choice (2.6 to 3.5:1); white on Halloween Dark's orange accent is 2.6:1; WhatsApp Light's `link` on `chat` is 3.8:1. Keep text on those fills short and semibold.

## Typography

- Set the interface in `--font-sans`: **Google Sans Flex** by default (`opsz` 14, `wdth` 100). Users may pick System, Inter, Geist, Figtree, Plus Jakarta Sans, Manrope or DM Sans; never hard-code a family in a widget.
- Variable faces are drawn 30 units lighter than their named weight: regular 370, medium 470, semibold 570, bold 670. The type styles carry those numbers.
- Use the scale: `message` 14.5 for message text, `body` 14 for prose, `label` 14 medium for settings labels, `menu` 13.5 for menus, previews and toasts, `control` 13 semibold for buttons, `caption` 12.5 for descriptions, `meta` 12 for times, `small` 11.5, `badge` 11 semibold. Headings: `section` 16, `name` 15, `title` 20 bold, `heading` 22.
- Counting timers use `timer` (Inter, whose figures are all one width) so they don't shift.
- One title per screen or dialog. Hierarchy comes from weight and colour (`text` vs `secondary`) before size.
- Lay text out with BiDi shaping (`crate::bidi`); names and previews end in an ellipsis, never clip.

## Spacing and layout

- The scale: `space-2`, `space-4`, `space-6`, `space-8`, `space-10`, `space-12`, `space-14`, `space-16`, `space-20`, `space-22`, `space-24`, `space-32`, `space-48`. The most used are 8, 4, 6 and 10.
- egui defaults: item spacing 8 x 6, button padding 12 x 6, interact size 40 x 28, menu margin 6, window margin 16.
- Fixed sizes: chat rows `row-height` 68, headers `top-bar-height` 60, controls `control-height` 36 and `control-width` 220, menu items `menu-item-height` 28.
- The window is a chat list beside a conversation card (`radius-panel`), 8 apart. A narrow window shows one at a time and slides between them (`duration-slide`).
- Use space before lines. A separator only where groups genuinely differ.

## Shape

| Token | Value | For |
| --- | --- | --- |
| `radius-small` | 8 | badges, thumbnails, swatches, the default focus ring |
| `radius-control` | 12 | buttons and egui's own widgets |
| `radius-medium` | 14 | fields, list rows, toasts, menu items |
| `radius-menu` | 20 | menus and popovers |
| `radius-card` | 20 | settings groups, call tiles, previews |
| `radius-panel` | 24 | the conversation card, the composer |
| `radius-dialog` | 28 | dialogs, pickers, large floating surfaces |
| `radius-pill` | capsule | pill buttons, dropdowns, text fields, chips, switches |
| `radius-bubble` / `-grouped` / `-join` | 10 / 18 / 4 | message bubbles |

Circles are for avatars and circle buttons only.

## Elevation and glass

Three shadows, three jobs:

- `shadow-bubble` (0 2 6) lifts bubbles, day chips and the composer off the wallpaper, with a 1px raised edge along the top (`Palette::raised_edge`) so it also reads on dark chats.
- `shadow-float` (0 6 20) under menus, toasts, the emoji picker and popups.
- `shadow-modal` (0 12 40) under dialogs and the image preview, with the window dimmed by `backdrop`.

Glass is a real blur of what lies beneath, not a transparent fill:

- **Menu frost** (`blur-menu`): menus, dialogs, pickers. Only the colours of what is under come through. Fill `menu-glass` (`overlay` at 176 alpha on dark, 200 on light), 1px `outline` edge.
- **Bar frost** (`blur-bar`): the header and composer over the conversation. Text under it stays faintly legible; it fades to sharp toward the header's bottom 35%. Header fill `header-veil`, pills and composer `glass`.
- Frost is 1.3x more saturated than what it covers (`blur-saturation`).
- Settings > Blur turns all frost off; every glass token then falls back to its solid colour. Design both.

## Motion

Reuse these. A new animation picks the closest existing duration and curve.

| Token | Value | Where |
| --- | --- | --- |
| `duration-popup` | 140ms `ease-out` | menus and dropdown lists: fade in while scaling 0.92 to 1 from the corner they open at |
| `duration-control` | 140ms `ease-out` | capsule controls and hover lifts |
| `duration-widget` | 120ms linear | egui's own animations: the switch |
| `duration-slider` | 150ms | the slider thumb swelling to 1.12 |
| `duration-slide` | 220ms `ease-out-strong` | narrow-window view slides, settings sections |
| `duration-glide` | 250ms | the transcript gliding for a new message, the sticky date fading |
| `duration-entrance` | 280ms | a sent message flying from the send button, an upload rising in |
| `duration-landing` | 300ms | a message settling, a dragged item returning |
| `duration-carry-lift` / `-land` | 200 / 320ms | drag and drop: lift `ease-out`, land `ease-in-out` |
| `duration-reveal` | 550ms | a chat-theme change revealed in a circle from the card that chose it |

Micro-interactions: icon buttons press to 0.92; circle buttons grow 5% on hover; tooltips wait 400ms. Close is the open run backwards, or an immediate cut for menus.

Reduced motion: while a screen reader is attached, decorative animations (typing dots, spinners, the recording light) step at ten frames a second (`duration-step`) and transcript flights are skipped. On the web, honour `prefers-reduced-motion`: tokens.css sets the durations to 0.

## Iconography

- Lucide icons: 24 grid, 2px stroke, round caps and joins, no fill. Zeus adds its own in the same style (delivery ticks, star fill, Halloween variants) under `assets/icons/`.
- Sizes: 16 in menus and fields, 18 to 20 in headers, 22 in the composer, 40 for empty states. Icon buttons add 12 around the icon.
- Colour: `secondary` at rest, `text` on hover; `accent` or `danger` only for state. Icons are drawn white in the files and tinted at paint time.
- Every icon-only control has a tooltip, which is also its accessible name.

## States

Every interactive element designs: rest, hover, pressed, keyboard focus, disabled, selected, and where it applies loading, empty and error.

- Hover: `surface-hover`, or a tint over glass. Pressed: `surface-active` or a 0.92 scale.
- Focus: a 1px ring (`focus-stroke`) inside the control in `accent`, `on-accent` over an accent fill, following its corners. Shown only after keyboard movement.
- Disabled: faded by the Ui; no hover and no pointer cursor; labels stay readable in `dim`.
- Selected: a stable plate (`tint-selected` or `surface-active`) plus a second cue.
- Loading: the stepped spinner; keep the layout from jumping.

## Accessibility

- Text 4.5:1 on its ground in every theme; icons, focus rings and control edges 3:1. `theme::readable_on` nudges a colour toward the text colour until it gets there.
- Every widget reports `WidgetInfo` (role, label, enabled, selected) for AccessKit.
- Keyboard: Tab reaches every control; Enter sends or confirms; Shift+Enter adds a line; Escape closes popups and dialogs.
- Respect OS scaling and the user's font choice; never size a control for the English label only.

## Using it in other projects
The system ships as plain files in the Zeus repository under `design-system/`, so any project can take what it needs.

| File | For |
| --- | --- |
| `tokens.json` | Every token in the W3C Design Tokens (DTCG) format. Colours carry Dark's value in `$value` and all eight themes in `$extensions["zeus.themes"]`. Feed it to Style Dictionary, Tokens Studio or your own script. |
| `tokens.css` | The same tokens as CSS custom properties. Dark on `:root`, Light under `prefers-color-scheme: light`, and every theme under `[data-theme="<id>"]`. Type styles as `.zeus-<style>` classes. Durations drop to 0 under `prefers-reduced-motion`. |
| `components.css` | Web renditions of the components (`z-` classes) built only from `tokens.css`. |
| `components.md` | Every component's spec and rules. |
| `egui.md` | How to port the system into another egui app. |
| `SKILL.md` | A guide for coding agents: drop it into a project's skills to have Claude build in this style. |

### On the web

```html
<link rel="stylesheet" href="tokens.css">
<link rel="stylesheet" href="components.css">
<html data-theme="dark"> … <button class="z-btn primary">Save</button>
```

Load Google Sans Flex from Google Fonts or self-host the bundled file. Frost needs `backdrop-filter: blur(var(--blur-menu)) saturate(var(--blur-saturation))`; give it a solid fallback (`overlay`) where `backdrop-filter` is unsupported or the user asked for reduced transparency.

### In other native toolkits

- **SwiftUI / UIKit:** map `menu-glass` to `.regularMaterial` or `.ultraThinMaterial`, and use the radii and durations as they are (seconds = ms / 1000; `ease-out` = `.easeOut`).
- **Jetpack Compose / Flutter:** read `tokens.json` into a theme object; the cubic-bezier lists map to `CubicBezierEasing` / `Cubic`.
- **Any toolkit:** keep the three rules that make it Zeus. Glass only on what floats. Radii nest (inner = outer − margin). Every state change eases with an existing duration.

### Keeping it in sync

`src/theme.rs` is the source of truth. When a palette, radius or duration changes there, update `tokens.json`, `tokens.css` and the matching rule here in the same change.
