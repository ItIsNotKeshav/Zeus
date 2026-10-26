# Components

Every component Zeus draws, with its egui helper, its spec and its rules. Web renditions of each live in `components.css` (class prefix `z-`).

## Button
Pill-shaped text buttons: a filled primary, an outlined secondary, a soft surface button and a destructive fill.

**egui:** `theme::pill_button(ui, palette, label, primary)` and `theme::soft_button(ui, palette, icon, label, active)`.

| Part | Spec |
| --- | --- |
| Pill label | `control` style (13, semibold), padding 18 x 8, radius `radius-pill` |
| Primary | fill `accent`, text `on-accent`; hover `accent-hover` |
| Secondary | 1px inside stroke `dim`, text `text`; hover stroke `text` |
| Destructive | fill `danger`; use only for the confirming action of a destructive dialog |
| Soft | 13 medium, padding 12 x 7, fill `surface`, hover `surface-hover`; active = accent at 18% with `accent` text |
| Icon in soft | 15pt, 6pt gap before the label |
| Focus | 1px ring inside the capsule in `accent`; `on-accent` over a primary fill |
| Disabled | the whole Ui fades; no hover, no pointer cursor |

### Use

- One primary per dialog or section, placed last (right). Everything else is secondary or soft.
- Labels are verbs that say what happens: "Delete chat", not "OK".
- Soft buttons are for toolbar-like actions inside content (Copy link, filter chips that are toggles).
- Don't make every action a pill: icon-only actions use IconButton.

### Accessibility

Each button reports `WidgetInfo::labeled(Button, enabled, label)`; soft buttons report `selected` too. Focus is visible only after keyboard movement (`keyboard_focus_id`), like `:focus-visible`. In Messages and Halloween, white on the accent is 2.6 to 3.5:1, below 4.5:1: keep labels semibold and short there.

## IconButton
Frameless icon buttons and filled circle buttons for toolbars, headers and calls.

**egui:** `theme::icon_button(ui, icon, size, color, hover, tooltip)` and `theme::circle_button(ui, icon, diameter, fill, fill_hover, icon_color, tooltip)`.

| Part | Spec |
| --- | --- |
| Icon button target | icon size + `icon-button-pad` (12) square, e.g. 20 icon in a 32 target |
| Icon colour | `secondary` at rest, `text` (or the caller's hover colour) on hover or focus |
| Press | icon scales to 0.92 while the pointer is down |
| Circle button | disc of `diameter`, icon 46% of it; fill grows 5% on hover and takes `fill_hover` |
| Composer pair | 22pt icons in 36pt targets, 8pt between icons |

### Use

- Every icon button needs a tooltip; the tooltip is also its accessible name. Tooltips appear after `duration-tooltip-delay` (400ms).
- Use circle buttons for the one action a surface exists for: send, record, answer, hang up. Hang up is `danger`.
- Keep icons Lucide, 2px stroke, round caps (see Iconography).

## Chip
Small glass capsules for filters, date separators and tags.

**egui:** `widgets::chip`, `widgets::filter_chip`, `widgets::dotted_chip`, laid out in `widgets::chip_scroll_row`.

| Part | Spec |
| --- | --- |
| Shape | capsule (`radius-pill`), padding 10 x 5, `small`/`caption` text |
| Fill | `glass` with a 1px `glass-edge` on vibrant themes; on opaque themes a raised fill with `shadow-bubble` and the 1px raised edge |
| Selected filter | `accent` fill, `on-accent` text |
| Dotted | dashed `dim` outline: an add or empty slot |
| Row | scrolls sideways, fading its ends over 16pt (`CHIP_FADE`) |

### Use

- Day chips float over the conversation and stick to the top while scrolling; they fade after the list stops (`duration-glide`).
- A filter row is single-select; show the count in the label, never in a second badge.

## Badge
Unread counts, mention marks and unread dots beside chat rows.

**egui:** `widgets::badge`, `widgets::mention_badge`, `widgets::unread_dot`, chosen by `widgets::unread_indicator`.

| Part | Spec |
| --- | --- |
| Count | 20 tall, at least 20 wide, label + 12; radius 10; `badge` text (11 semibold) |
| Over 99 | shows "99+" |
| Fill | `accent`, text `on-accent`; muted chats `dim` |
| Mention | a 20pt disc with "@" at 12 semibold |
| Dot | a 10pt disc: a chat marked unread by hand |

### Use

- The badge sits at the row's right edge, under the timestamp; the timestamp turns `accent` while there are unread messages, so the state shows twice, not only by colour.
- Never show a count and a dot together.

## Switch
A switch that shows its state by shape as well as colour.

**egui:** `widgets::switch(ui, palette, &mut on)`.

| Part | Spec |
| --- | --- |
| Track | 40 x 22 capsule (`switch-width`) |
| Off | `surface-active` track with a 1.5px `secondary` outline; a 12pt `secondary` knob |
| On | `accent` track, no outline; a 16pt knob with a 12pt check in `accent` |
| Knob colour on | white, unless white has under 2:1 against the accent (Dark's near-white accent), then `on-accent` |
| Motion | `duration-widget` (120ms), egui's linear `animate_bool`; the check fades in over the second half |

### Use

- A switch applies at once. If a change needs confirming or saving, use a checkbox in a form instead.
- Put the switch in a SettingsRow; the label says what is on ("Show previews"), never "Enable ...".

### Accessibility

The off and on states differ in knob size, outline and the check mark (WCAG 1.4.1), not only in colour.

## Slider
The liquid-glass slider: an accent-filled track under a translucent capsule thumb.

**egui:** `widgets::glass_slider(ui, palette, &mut value, range, width)`.

| Part | Spec |
| --- | --- |
| Hit area | `width` x 28 (`slider-height`), click and drag anywhere |
| Track | 6 tall, radius 3, `surface-active` at 90% with a 1px `secondary` 18% edge |
| Fill | `accent` from the start to the thumb |
| Thumb | 30 x 20 capsule: white at 70 alpha (dark) or 225 (light), 1px rim, accent glow through its lower half, a 1.5pt highlight along its top, three stacked soft shadows |
| Active | thumb swells to 1.12 over `duration-slider` (150ms) while hovered or dragged |

### Use

- For continuous values with an immediate preview (doodle intensity, playback speed, volume). Show the value beside it when the number matters.

## Dropdown
The capsule dropdown and its frosted option list.

**egui:** `controls::dropdown(ui, palette, salt, label, selected, width, |ui| controls::option(...))`. The list is an `egui::Popup::menu` shown with `FrostedPopup::show_frosted`.

| Part | Spec |
| --- | --- |
| Control | 36 tall (`control-height`), 220 wide in settings rows (`control-width`), capsule |
| Fill | flat `tint-field` (text at 8%); hover or open `tint-field-lift` (14%), eased over `duration-control` with `ease-out` |
| Text | `body` (14 regular), inset 16; chevron 34 wide on the right |
| List | 6 below the control, its width; `menu-glass` over `blur-menu`, radius `radius-menu`, 6 margin, `shadow-float` |
| Option | 34 tall (`option-height`), radius `radius-medium`; hover `surface-hover`; selected marked by a dot and the value in the control |
| Open | scales 0.92 to 1 from its top-left corner with a fade, `duration-popup` (140ms) `ease-out` |
| Long list | scrolls past 320 (`list-height-max`) |

### Use

- Controls are flat tinted capsules on opaque content; only the transient list is glass (Apple's Liquid Glass guidance).
- Escape or a click outside closes the list.

## TextField
The capsule text field and the search field.

**egui:** `controls::text_field` for forms and settings; `widgets::search_field(ui, palette, ...)` for the chat list and pickers.

| Part | Spec |
| --- | --- |
| Field | 36 tall, capsule, `tint-field` fill, text inset 16, `body` text, placeholder `dim` |
| Hover | `tint-field-lift` over `duration-control` |
| Focus | 1px ring in `accent` inside the capsule; the cursor is 2px `accent` |
| Search | a 16pt search icon in `secondary` before the text, a clear button after it once there is text; its focus ring is a quiet grey (text at 28%) since the accent is kept for results |
| Invalid | the description under the field turns `danger` and says how to fix it |

### Use

- Labels sit above or beside the field (SettingsRow), never only as placeholder text.
- Keep one search field per view, at the top of the list it filters.

## SettingsRow
A labelled settings row: label and description on the left, its control in a right-aligned column.

**egui:** `widgets::setting_row(ui, palette, label, description, |ui| control)`.

| Part | Spec |
| --- | --- |
| Label | `label` (14 medium), `text` |
| Description | `caption` (12.5 regular), `secondary`, wraps |
| Text column | the row width minus `control-width` minus 52, at least 120 |
| Control | right-to-left aligned, vertically centred: a Switch, Dropdown, TextField or soft button |
| Spacing | `space-10` after each row; groups sit in a card (`radius-card`, `panel` fill) with `space-20` between groups |

### Use

- One setting per row. A description explains the consequence, not the label again.
- Group related rows under a `section` heading; don't put each row in its own card.

## Menu
The frosted context menu and popover.

**egui:** `widgets::menu_frame(palette)` as the popup frame, `FrostedPopup::show_frosted` to show it, `widgets::menu_item`, `menu_item_enabled`, `submenu` and `menu_separator` inside. Size it with `widgets::menu_width(ui, labels, icons)`.

| Part | Spec |
| --- | --- |
| Frame | `menu-glass` fill over `blur-menu` frost, 1px `outline`, radius `radius-menu` (20), margin 6, `shadow-float` |
| Item | 28 tall (`menu-item-height`), radius `radius-medium` (14 = 20 minus the 6 margin, so the curves nest) |
| Item content | 16pt icon in `secondary` at 10 + 8, label `menu` (13.5) at 36; long labels end in an ellipsis |
| Hover | `surface-hover` |
| Disabled | label and icon `dim`, no hover |
| Destructive | label and icon `danger`, grouped last |
| Separator | 9 tall, a 1px `outline` line inset 6 |
| Submenu | a chevron on the right; it is as wide as its widest label and keeps 8 from the window edge |
| Open | 0.92 to 1 from the corner it opened at, with a fade, `duration-popup` (140ms) `ease-out`; the frost scales with it |

### Use

- Order by frequency, then group: the message menu runs Reply, Edit, Forward, Copy text, Select, Star, Pin, media actions; then Message info; then deletes; then Reload earlier messages and Copy message ID.
- With Settings > Blur off the menu is the solid `overlay`.

## Dialog
The frosted modal dialog.

**egui:** `ui::dialogs::show` builds an `egui::Modal` with a `Frame` of `menu_glass()`, radius `RADIUS_DIALOG`, margin 22, `modal_shadow()`, and reserves a `widgets::Frost` under it.

| Part | Spec |
| --- | --- |
| Frame | `menu-glass` over `blur-menu`, 1px `outline`, radius `radius-dialog` (28), margin 22, `shadow-modal` |
| Width | 380 for confirmations (`dialog-width`); 400 to 440 for forms and pickers |
| Backdrop | the palette's `shadow` colour over the window |
| Title | `title` (20 bold), one line where possible |
| Body | 13.5 `secondary`, says the consequence |
| Actions | right-aligned pill buttons, primary or destructive last |
| Scrolling body | grows with content to the window height less 158, at most 592 (`widgets::dialog_scroll_height`) |

### Use

- Use a dialog only to confirm something that can't be undone or to collect a few fields. Everything else is a menu, popover or toast.
- Name the action on the button: "Delete chat", never "Yes".
- Escape and a click on the backdrop cancel (`egui::Modal::should_close`); Enter confirms where the dialog has one primary action.

## Toast
Transient feedback at the bottom left, above the composer.

**egui:** `ui::toasts` in `src/ui/mod.rs`.

| Part | Spec |
| --- | --- |
| Frame | `overlay` fill, 1px `outline`, radius `radius-medium` (14), padding 14 x 10, `shadow-float` |
| Text | `menu` weight 470 (13.5 medium), wraps at 360 |
| Undo | a soft button with an undo icon |
| Lifetime | info 3.2s (`duration-toast`), undo 6s; a close button is always there |
| Position | above the composer, never covering its buttons; stacked upward |

### Use

- For outcomes that are otherwise invisible: copied, saved, deleted with undo. Not for errors that need action; those go inline.
- Say what happened in two or three words: "Link copied".

## MessageBubble
Message bubbles in the Dark and Light style: grouped runs whose inner corners tighten where they meet.

**egui:** `widgets::grouped_bubble_shape` / `grouped_bubble_corners(own, first, last)` for the Dark and Light style; `widgets::bubble_shape` / `bubble_corners(tail)` for WhatsApp; `widgets::messages_balloon` for Messages. Contents take `Palette::on_bubble(own)`.

| Part | Spec |
| --- | --- |
| Text | `message` (14.5 regular); Messages draws 13 and scales by `MESSAGES_SCALE` |
| Padding | 10 x 7 around the text, meta floated bottom right |
| Dark/Light corners | `radius-bubble-grouped` (18); `radius-bubble-join` (4) on the sender's side where two in a run meet |
| WhatsApp corners | `radius-bubble` (10) with an 8 x 11 tail at the top of a run; lifted by `shadow-bubble` and a 1px raised edge |
| Messages | Apple's continuous balloon, radius 15, curled tail under the last of a run, flat |
| Fill | `bubble-in` for others, `bubble-out` for yours; Messages shades yours top to bottom |
| Width | up to 72% of the conversation (65% in Messages) |
| Gaps | 2 inside a run, 8 between runs |
| Reply quote | a `radius-small` card washed with the window colour, a 3pt accent bar that follows its curve (`widgets::quote_bar`), sender in semibold |
| Meta | `meta`/`small` text; secondary and dim are re-derived to 4.5:1 on the bubble; ticks in `read` at 3:1 |

### Use

- Bubbles stay tighter than panels on purpose: never round them to the dialog radius.
- New content arrives with `duration-entrance` and settles with `duration-landing`; the list glides with `duration-glide`.

## Composer
The glass composer at the bottom of a conversation.

**egui:** `ui::conversation::composer`, shown with `widgets::raised` so it lifts off the chat like a bubble.

| Part | Spec |
| --- | --- |
| Field | `glass` fill with `glass-edge`, radius `radius-panel` (24, `COMPOSER_RADIUS`), frosted with `blur-bar` |
| Lift | `shadow-bubble` plus a 1px raised edge along its top |
| Row height | one text line + 14 (`COMPOSER_PADDING`), never under 36 (`COMPOSER_CONTROL`) |
| Inset | 2 between the field and the buttons at its ends |
| Buttons | plus and emoji: 22pt icons in 36 targets, 8 between the icons; then the text; then a circle button that is Send when there is text and Record when empty |
| Text | `message` size; placeholder `dim`; grows line by line, then scrolls |

### Use

- Enter sends, Shift+Enter adds a line. The send button appears only when there is something to send.
- Toasts and the status banner sit above it and never cover its buttons.

## ChatRow
A chat-list row: avatar, name, time, preview and unread state.

**egui:** `ui::chats`, highlighted with `widgets::row_highlight`, avatars from `widgets::avatar`, unread marks from `widgets::unread_indicator`.

| Part | Spec |
| --- | --- |
| Height | 68 (`row-height`) |
| Highlight | a `radius-medium` card inset 8 x 2: on vibrant themes a flat `tint-hover` (hover) or `tint-selected` (selected); on opaque themes `surface-hover`/`surface-active` raised with a soft shadow |
| Avatar | 48 disc, colour from the chat's hue (`Palette::avatar`: 38% saturation, 42% lightness dark; 45%, 62% light) |
| Name | `name` (15 semibold), ellipsis |
| Time | `meta` (12) in `dim`; `accent` while unread |
| Preview | `menu` (13.5) in `secondary`, one line |
| Unread | a Badge at the right of the preview line |

### Use

- Selected, hovered and focused rows look different from one another; selection never relies on colour alone (the conversation opens beside it).
- Group senders in previews are named; their colour in groups comes from `Palette::sender(hue)`.

## EmptyState
A centred icon, title and line of help where a list or view has nothing to show.

**egui:** `widgets::empty_state(ui, palette, icon, title, body)`.

| Part | Spec |
| --- | --- |
| Inset | 48 from the top |
| Icon | 40pt in `dim` |
| Title | `section` (16 semibold), 8 below the icon |
| Body | 13.5 regular `secondary`, 2 below the title, wraps |

### Use

- Say what will appear and how to get it there. No illustrations, no jokes.
