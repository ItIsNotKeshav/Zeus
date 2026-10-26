---
name: zeus-design-system
description: Build or restyle UI in the Zeus look (iOS 26 liquid glass, monochrome Dark and Light themes, nested large radii, eased motion) in egui, on the web or in another toolkit. Use when asked to design something "like Zeus" or to apply the Zeus design system.
---

# Zeus design system

Use this when building or restyling an interface in the Zeus style. Tokens live in `tokens.json` (DTCG) and `tokens.css` beside this file; component specs in `components.md`; egui porting notes in `egui.md`; web renditions in `components.css`.

## Before you build

1. Read `README.md` in this folder (principles, colour roles, type, spacing, shape, glass, motion, states).
2. Find the component in `components.md` and copy its spec exactly: sizes, radii, tokens, motion.
3. In egui, reuse the helpers named there; on the web, load `tokens.css` and `components.css` and use the `z-` classes.

## Rules that make it Zeus

- **Glass only on what floats.** Menus, popovers, dialogs and pickers: `menu-glass` over a deep blur (`blur-menu`, saturate 1.3), 1px `outline`. Header and composer over content: `header-veil` / `glass` over a light blur (`blur-bar`). Everything else is opaque (`window`, `panel`, `chat`, `surface`). Controls in content are flat capsules tinted with `tint-field`, never glass. Always design the Blur-off fallback (solid `overlay`).
- **Monochrome by default.** Dark: near-black ground, near-white `accent`, `on-accent` dark. Light: the reverse. Colour comes from a chosen accent theme or from status (`danger`, `warning`). Call the default themes Dark and Light.
- **Large radii that nest.** `radius-small` 8, `radius-control` 12, `radius-medium` 14, `radius-menu` 20, `radius-card` 20, `radius-panel` 24, `radius-dialog` 28, capsules for pills, fields, chips and switches. Inner radius = outer radius minus the margin between them (menu 20, margin 6, item 14). Message bubbles stay tighter: 10, or 18 with 4 where a run joins.
- **Spacing scale.** 2, 4, 6, 8, 10, 12, 14, 16, 20, 22, 24, 32, 48. Controls 36 tall; rows 68; headers 60; menu items 28; dialogs 380 wide with 22 margin.
- **Type.** Google Sans Flex (variable weights drawn 30 lighter: 370, 470, 570, 670). Message 14.5, body 14, label 14 medium, menu 13.5, button 13 semibold, caption 12.5, meta 12, badge 11 semibold; section 16, title 20 bold. Inter for counting timers. Sentence case everywhere.
- **Shadows by job.** `shadow-bubble` (0 2 6) plus a 1px raised top edge for things on the chat; `shadow-float` (0 6 20) for menus and toasts; `shadow-modal` (0 12 40) for dialogs over a `backdrop`.
- **Everything eases, with existing values.** Popups and controls 140ms ease-out (popups scale 0.92 to 1 from their anchor corner and fade); egui widgets 120ms; slides 220ms ease-out-strong; glides 250ms; entrances 280ms; landings 300ms; theme reveal 550ms. Icon buttons press to 0.92; circle buttons grow 5% on hover. Never invent a new duration when one of these fits. Respect reduced motion.
- **States by shape and colour.** Every control designs rest, hover, pressed, focus (1px `accent` ring inside, `on-accent` over accent fills, keyboard only), disabled, selected. Never signal state by colour alone.
- **Contrast.** Text 4.5:1 on its ground in every theme; icons, rings and edges 3:1. On fills you don't control, nudge the colour toward `text` until it passes (`readable_on`).
- **Icons.** Lucide, 2px stroke, round caps; 16 in menus, 20 in headers, 22 in the composer; `secondary` at rest, `text` on hover; every icon-only control has a tooltip that is also its accessible name.

## Don'ts

- Don't put glass, borders or shadows on every surface, or a card around every group.
- Don't make every button a pill or use the accent for every action.
- Don't round bubbles to panel radii.
- Don't hard-code colours inside a component; take them from the palette or tokens.
- Don't ship a new UI element without its open, close and state-change animation.
