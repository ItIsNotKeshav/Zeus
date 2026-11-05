# To do later

## Edit focus blur memory

The inline-edit focus blur (`glass::veil`, Linux and Windows only) keeps a
chat-card-size set of render targets allocated after an edit, about 17 MB of
video memory at 2400x1500 px. Only 4 sets are kept (`TARGET_SETS`), so while
editing a set may be reallocated every painted frame. Free it when the edit
ends, or share a set with the header.

## Animation opportunities

Ranked list from a read-only sweep with the Emil Kowalski skills (2026-11-05).
Line numbers are approximate: grep again before editing.


Recon: dialogs (`closing::surface`, dialogs.rs:151), picker (picker.rs:100), pane overlay (pane.rs:173), info panel, popups (FrostedPopup), toasts (mod.rs:718), switch (widgets.rs:1304), reaction tabs (reaction_list.rs:206), scroll-down button (conversation.rs:4415) already move. Zero motion hits in: rail, call, calls, lock, login, polls, message_info, new_group, labels, accounts, image_editor, update, focus, swipe. Tokens: MOTION_QUICK 0.14, MOTION_PANEL 0.22, MOTION_SCREEN 0.35, CLOSE 0.7, CLOSE_SCALE 0.96. Line numbers approximate to the function named; re-grep before editing.

### 1. Navigation and panes

| file:line | today | purpose | freq | motion |
|---|---|---|---|---|
| ui/mod.rs:257 (`Page` match) | Chats/Calls/Settings/Wallpaper swap content in one frame | prevent jarring change, spatial | tens/day (rail click) | cross-fade only: new page `theme::animate` 0..1 over 140 ms (MOTION_QUICK) via `multiply_opacity`, 4 pt rise from below. No slide. Keyboard switch: none |
| ui/mod.rs:181 + settings.rs:327 | List column swaps chats <-> settings categories instantly | spatial consistency | occasional | fade 140 ms; categories enter from x-8 pt, ease_out 220 ms |
| ui/settings.rs:2934 (`section`) | Category content replaces previous with no bridge | state indication | occasional | opacity 0->1 + 6 pt rise, ease_out 180 ms; pill already slides (settings.rs:1752), so content should follow it |
| ui/rail.rs:357 (`destination_button`) | lit surface appears/disappears on selection | state indication | tens/day | one lit pill that glides between destinations (FLIP the rect, Spring 0.3 s bounce 0.1) as settings.rs pill does; icon tint cross-fade 140 ms |
| ui/pane.rs:331 (`hit_row` selected) | search hit highlight jumps row to row | feedback | tens/day (arrow keys) | keyboard: none. Pointer hover only: 100 ms fade via `theme::animate` |
| ui/pane.rs:445 (`calendar_popup`) + month_grid:523 | month changes swap grid instantly | spatial | rare | month grid slides 12 pt in step direction + fade, ease_out 180 ms |

### 2. Dialogs and sheets

| file:line | today | purpose | freq | motion |
|---|---|---|---|---|
| ui/update.rs:25 (`egui::Window`) | Update window pops in/out un-animated | prevent jarring change | rare | same as dialogs.rs: scale 0.96->1 + fade, MOTION_PANEL, reuse `closing::surface` for exit |
| ui/new_group.rs:61 -> :283 (choosing -> naming) | step swap in the same dialog, height jumps | spatial, state | rare | slide content 24 pt left + fade 220 ms; dialog height eases (animate_value 220 ms) instead of snapping |
| ui/new_group.rs:172 (`chips`) | picked chips appear/vanish instantly, row reflows | state indication | occasional | chip scales 0.8->1 + fade, Spring 0.3 s bounce 0.2; remove: fade 100 ms |
| ui/labels.rs:219 (`manager`) | rows added/removed/renamed snap | prevent jarring change | rare | new row grows height 0->full + fade 180 ms; swatch select ring scales 0.9->1 Spring 0.25 bounce 0.2 |
| ui/image_editor.rs:34 (crop/rotate) | rotate turns 90 deg instantly (`paint_turned`:298) | state indication | occasional | rotate eases through angle, ease_out 250 ms; shade_outside:355 fades with crop edits not at all (keep) |
| ui/dialogs.rs:38 (confirm buttons) | destructive confirm is a plain click | feedback | rare | none; hold-to-confirm would change behaviour, out of scope |

### 3. Chat list and rail

| file:line | today | purpose | freq | motion |
|---|---|---|---|---|
| ui/widgets.rs:2382 (`dotted_chip` selected fill) | accent tint appears instantly on select | feedback | tens/day | cross-fade fill + text colour `theme::animate` MOTION_QUICK (140 ms); screen-reader 0 already handled. Used by chats, calls (calls.rs:281), labels |
| ui/labels.rs:97 (`chip_row`) | row pops into layout when first label exists | prevent jarring change | rare | height 0->28 pt + fade, ease_out 220 ms; collapse reversed |
| ui/chats.rs:449 (`search_and_filters`) | search field/filters toggle instantly | state indication | occasional | field height + fade 180 ms; no motion on keyboard focus |
| ui/chats.rs:2389 (`compact_list`) | list hide/show swaps full list <-> avatars | spatial | occasional | width ease_out 220 ms, rows crossfade; narrow slide already exists (mod.rs:350) |
| ui/accounts.rs:29 (avatar dot) + :181 (badge) | unread dot/badge appear instantly | feedback | tens/day | reuse chat-list unread badge pop (scale 0.6->1 Spring 0.3 bounce 0.25) |
| ui/calls.rs:317 (`history_row`) | rows pop in on filter All <-> Missed | prevent jarring change | tens/day | opacity cross-fade 120 ms only; no stagger |

### 4. Conversation details

| file:line | today | purpose | freq | motion |
|---|---|---|---|---|
| conversation.rs:1555 (reply), :2324 (`edit_strip`), :2289 (`unsent_voice_strip`) | strip above composer appears, field and transcript jump by its height | spatial, prevent jarring | tens/day | strip grows height 0->full, Spring 0.35 s bounce 0 (no overshoot, layout-affecting) + fade 140 ms; exit reverse 100 ms. Transcript bottom follows via `stick_to_bottom` |
| ui/polls.rs:259 (`ballot` bars) | vote bar width snaps on vote/update | state indication | occasional | bar width ease_out 300 ms from old fraction, `animate_value_with_time`; selected check pops Spring 0.3 bounce 0.25; first render: no animation |
| ui/info.rs:184 (`members_open`, Show all) | member list expands in one frame | prevent jarring change | occasional | height ease_out 220 ms + rows fade; collapse 150 ms |
| ui/message_info.rs:73 (`receipts`) | read/delivered sections appear with data, tick times snap | state indication | occasional | section fade 140 ms; nothing on live receipt updates (reading UI) |
| ui/conversation.rs:3138 (unread divider) | divider present from first paint; vanishes on read | state | tens/day | fade 200 ms when cleared; none on appear |
| ui/conversation.rs:13130 (`selection_bar`) | other agent (selection spring) | skip | | in progress |

### 5. Calls, login, lock, misc

| file:line | today | purpose | freq | motion |
|---|---|---|---|---|
| ui/call.rs:53 (call `Area`), :845 (`ringing`) | full-window call screen appears/leaves in one frame | spatial, prevent jarring | occasional | scale 0.97->1 + fade, Spring 0.45 s bounce 0.15 from the call button/notification; exit fade 150 ms. Avatar breathing ring while ringing (scale 1->1.06, 1.2 s sine, still when `screen_reader_attached`) |
| ui/call.rs:82 / :118 / :237 (phase changes: Incoming -> Active, video on/off) | label, controls and tiles swap in place | state indication | occasional | status text cross-fade 140 ms; tile appear scale 0.9->1 Spring 0.35 bounce 0.15; peer video first frame fade 200 ms; `tile_grid` (call.rs:973) rects FLIP as people join, 250 ms |
| ui/call.rs:513 (`controls` mute/camera toggles) | icon + fill snap | feedback | occasional | fill cross-fade 120 ms; icon swap scale 0.8->1 Spring 0.25 bounce 0.2 (as composer mic/send morph) |
| ui/login.rs:238 (`qr_view`) / :287 (`pair_code_view`) / :23 (`splash`) | QR, code and splash replace each other abruptly; QR refresh flashes | state indication | rare | 200 ms cross-fade between views; QR refresh opacity dip 0.6->1 over 200 ms; connected success: check scale 0.5->1 Spring 0.5 bounce 0.3 (delight tier) |
| ui/lock.rs:86 (`app_lock.wrong`) + :16 | wrong password only shows text; unlock cuts to app | feedback, prevent jarring | occasional | field shake +-6 pt, 3 cycles, 280 ms ease_out; error text fade 140 ms; unlock fades lock screen out 200 ms, app scale 1.02->1 |

### Rejected

- Ctrl+K style jumps, Alt+Up/Down chat switch, keyboard hit selection in search (pane.rs:374 `keyboard`, keys.rs): frequency, 100+/day, instant.
- Composer typing, mention/slash suggestion lists (conversation.rs:1319): frequency + function, read-and-act UI, appears per keystroke.
- Chat list scroll and row hover, message text selection, sticky date: frequency/function, already tuned or must track the pointer.
- Delete/destructive dialog confirm "hold to confirm": function, changes behaviour, not motion polish.
- Settings toggles beyond existing switch, text fields in settings: purpose, already animated (widgets.rs:1304) or no state story.
- Image editor crop handle drag (image_editor.rs:321): function, direct manipulation must follow the pointer 1:1.

### Top 5 picks

1. Composer strips grow in/out (reply, edit, voice): daily, kills the layout jump above the field.
2. Call screen enter/exit + phase and tile transitions: biggest un-animated full-window surface.
3. Chip selected fill cross-fade (`dotted_chip`): one fix covers chats, calls, labels filters.
4. Rail lit pill glides between destinations + page content cross-fade: ties navigation together like the settings pill.
5. Lock screen wrong-password shake + unlock fade: cheap, clear feedback, nothing there today.
