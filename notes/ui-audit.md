# Zeus UI audit, 2026-11-08

Two parts:

- **Part A** is the read-only audit from the pasted brief: facts with `file:line`, one table per check, then "Disagreements to check".
- **Part B** is what you asked for on top of it: scores for Telegram, Discord, the WhatsApp-style mock and Zeus, how the open PRs move those scores, and before-and-after fixes.

How this was produced:

- **Code:** branch `claude/zeus-ui-audit-c3f3fa`, at `main` commit `96a248ec`.
- **Open PRs:** all 25 were listed on 2026-11-08, and #88 is the newest. Diffs were read for the 11 that touch the UI: #64, #68, #75, #77, #79, #81, #83, #84, #85, #87 and #88. `gh pr diff` could not return #64's diff (+45,951/-8,841, stacked on #56), so for #64 only its description was used.
- **Screenshots:** from the synthetic demo only (`--demo-shot`, 1333x780 points at 1.5x), never your account. They are in `notes/ui-audit-shots/`, which is not committed. Motion (springs, overscroll, send animations) does not show in a still image.
- **Counting:** a script read every non-test line under `src/`, skipping anything under `#[cfg(test)]` and any `mod tests`. It was checked by hand on one file: `src/ui/login.rs` has 13 literal `add_space` calls by both grep and the script.

## What the PR changes (added after the audit)

- **Chat-list dates** leave out this year's year ("12 Mar"), so names fit. Before: "Katherine Jo…". After: "Katherine Johnson".
- **Type scale:** `theme::TEXT_*` tokens. 465 literal sizes now use them, and 2 named exceptions remain.
- **Radii and colours:** 7 radii and 3 colour literals now use tokens.
- **Spacing:** 100 off-scale gaps are now on the 4/8/12/16/20/24/32/40 scale.
- **Motion:** 16 direct animation calls now go through `theme::animate` / `animate_value`, and all of them use `theme::motion_time`.
- **Filter chips** fade into the list's own colour over 32 points.

**Dropped after reading the code:**

- **Zeus-theme bubble colours:** own and others' already differ (`#4a4a4a` vs `#252525`); glass made them look closer in the screenshot.
- **Moving Language:** it is first on purpose, for readers who cannot read the rest.
- **Hiding the Settings back arrow:** it is the way out of Settings, not duplicate navigation.
- **Spelling:** the copy is consistently British; "Favorites" matches WhatsApp's own chip.

**Not done:**

- The Zeus theme's lone "edited" row needs layout debugging.
- The #85 and DESIGN.md spring notes belong to those PRs.

---

# Part A: audit (facts only)

## 1. Spacing literals: `add_space(<number>)` in `src/ui/`

The scan found 247 literal calls. Calls that pass a constant, such as `add_space(GAP)`, are not counted.

| Value | Count | On the 4/8/12/16/20/24/32/40 scale? |
|---|---|---|
| -5 | 1 | no |
| -1 | 1 | no |
| 2 | 16 | no |
| 4 | 50 | yes |
| 6 | 36 | no |
| 8 | 54 | yes |
| 10 | 34 | no |
| 12 | 18 | yes |
| 14 | 6 | no |
| 16 | 9 | yes |
| 18 | 6 | no |
| 20 | 5 | yes |
| 22 | 1 | no |
| 24 | 7 | yes |
| 32 | 2 | yes |
| 48 | 1 | no |
| **Total** | **247** | **145 on the scale, 102 off it (41%)** |

The files with the most calls are `settings.rs` (39), `dialogs.rs` (29), `info.rs` (20), `conversation.rs` (20), `benchmark.rs` (16), and `picker.rs`, `login.rs` and `call.rs` (13 each). The full list with lines is in the scratch output (`audit.txt`); ask if you want it pasted in here.

## 2. Corner-radius literals in `src/ui/` that are not a `theme::RADIUS_*` token

| file:line | Literal | Function | Nearest token |
|---|---|---|---|
| src/ui/conversation.rs:4592 | `CornerRadius::same(20)` | `messages` (shadow under the scroll-to-bottom button) | `RADIUS_MENU` / `RADIUS_CARD` = 20, same value but untokenised |
| src/ui/conversation.rs:10467 | `.corner_radius(10)` | `carousel` | none; equals the bubble radius in DESIGN.md |
| src/ui/conversation.rs:11042 | `.corner_radius(6.0)` | `carousel_picture` | none |
| src/ui/conversation.rs:11637 | `.corner_radius(4.0)` | `preview_card` | none |
| src/ui/conversation.rs:11711 | `.corner_radius(8.0)` | `location_card` | `RADIUS_SMALL` = 8, same value but untokenised |
| src/ui/new_group.rs:193 | `CornerRadius::same(15)` | `chips` | none |
| src/ui/settings.rs:1359 | `CornerRadius::same(3)` | `level_meter` | none |

## 3. Font sizes given as literals in `src/`

| Pattern | Count |
|---|---|
| `FontId::proportional(<n>)` / `FontId::monospace(<n>)` | 0 |
| `.size(<n>)` | 0 matched as a font size; spot-checked, the `.size(` hits are not text |
| `theme::regular/medium/semibold/bold(<n>)` (not asked for by the check; reported as the extra row the brief's spirit needs) | 468 |
| `FontId::new(<n>, ..)` in `src/theme.rs:1015-1021` (`text_styles`) | 5 |

The 468 theme-helper sizes span **21 distinct values**. Of these 468 calls, 172 use half-point sizes.

| Size | Count | DESIGN.md role |
|---|---|---|
| 10.0 | 3 | below micro (11) |
| 10.5 | 1 | none |
| 11.0 | 19 | micro |
| 11.5 | 20 | none |
| 12.0 | 41 | caption |
| 12.5 | 82 | none |
| 13.0 | 110 | control |
| 13.5 | 53 | none |
| 14.0 | 53 | body |
| 14.5 | 16 | none |
| 15.0 | 13 | none |
| 16.0 | 23 | section |
| 17.0 | 3 | none |
| 18.0 | 11 | none |
| 19.0 | 4 | none |
| 20.0 | 7 | title |
| 22.0 | 1 | none |
| 24.0 | 4 | display |
| 26.0 | 2 | none |
| 28.0 | 1 | none |
| 30.0 | 1 | none |

The files with the most are `conversation.rs` (81), `dialogs.rs` (55), `settings.rs` (43), `info.rs` (40), `chats.rs` (31) and `call.rs` (25).

egui's `text_styles` (`src/theme.rs:1015-1021`):

| Style | Code | DESIGN.md |
|---|---|---|
| Small | 11.5 | micro 11 or caption 12 |
| Body | 14 | 14 |
| Button | 14 | control 13 |
| Heading | 22 | title 20 or display 24 |
| Monospace | 13 | (none) |

## 4. Colour literals in `src/ui/` (theme.rs is skipped because it is outside `src/ui/`)

| file:line | Call | Function | Literal or derived |
|---|---|---|---|
| src/ui/conversation.rs:8008 | `Color32::from_rgb(0xEA, 0xB3, 0x08)` (`FOOTER_STAR`) | `paint_forwarded_label` | **literal** |
| src/ui/mod.rs:397 | `from_rgba_unmultiplied(233, 237, 239, 32)` | `central_frame` | **literal** |
| src/ui/mod.rs:399 | `from_rgba_unmultiplied(10, 10, 10, 32)` | `central_frame` | **literal** |
| src/ui/conversation.rs:5161 | `from_rgba_unmultiplied(r, g, b, ..)` | `doodle_scrim` | derived from palette |
| src/ui/conversation.rs:6690 | `from_rgba_unmultiplied(r, g, b, ..)` | `edit_focus_shade` | derived |
| src/ui/conversation.rs:8184 | `from_rgba_premultiplied(darker.r(), ..)` | `shaded` | derived |
| src/ui/labels.rs:61 | `Color32::from_rgb(red, green, blue)` | `color_of` | derived (parsed label hex) |
| src/ui/settings.rs:1914 | `Color32::from_rgb(r, g, b)` | `accent_picker` | derived (accent RGB) |
| src/ui/conversation.rs:16967 | `ColorImage::from_rgba_unmultiplied` | `pending_poster` | image buffer, not a colour |
| src/ui/conversation.rs:17198, 17206 | `ColorImage::from_rgba_unmultiplied` | `pending_strip` | image buffer |
| src/ui/image_editor.rs:238, 243 | `ColorImage::from_rgba_unmultiplied` | `pasted_image` | image buffer |

## 5. Motion: `animate_bool*` and `animate_value*` calls that bypass `theme::animate`

The scan found 27 calls. **No call passes a non-zero literal duration.** Every duration is a `MOTION_*` token, a constant made from one (`STICKY_DATE_FADE` and `UPLOAD_ENTRANCE` are both `MOTION_PANEL`), or `0.0`.

| file:line | Function | Duration | Reduced-motion gate |
|---|---|---|---|
| src/theme.rs:763 | `animate` | (the helper's own body) | screen reader |
| src/ui/benchmark.rs:120 | `progress` | MOTION_PANEL | none |
| src/ui/carry.rs:576 | `fade_glow` | MOTION_QUICK | none |
| src/ui/carry.rs:642 | `paint` | MOTION_QUICK | none |
| src/ui/closing.rs:138 | `surface` | literal 0.0 (a reset) | n/a |
| src/ui/controls.rs:62 | `dropdown` | MOTION_QUICK | none |
| src/ui/controls.rs:128 | `option` | MOTION_QUICK | none |
| src/ui/controls.rs:228 | `show` | MOTION_QUICK | none |
| src/ui/conversation.rs:176 | `show` | MOTION_QUICK | none |
| src/ui/conversation.rs:1755 | `composer` | MOTION_QUICK | none |
| src/ui/conversation.rs:2872 | `sticky_date` | STICKY_DATE_FADE, or 0.0 while scrolling | none |
| src/ui/conversation.rs:4557 | `messages` | MOTION_QUICK | `reduce_motion` |
| src/ui/conversation.rs:5982 | `bubble_row` | MOTION_QUICK | `reduce_motion` |
| src/ui/conversation.rs:10766 | `album_download_button` | MOTION_QUICK | `reduce_motion` |
| src/ui/conversation.rs:10794 | `album_download_button` | MOTION_QUICK | `reduce_motion` |
| src/ui/conversation.rs:12015 | `picture` | literal 0.0 (a reset) | n/a |
| src/ui/conversation.rs:14046 | `upload_rows` | UPLOAD_ENTRANCE | none |
| src/ui/conversation.rs:14051 | `upload_rows` | MOTION_PANEL | none |
| src/ui/conversation.rs:17132, 17137 | `pending_strip` | `animation_time` (MOTION_*, 0 when reduced; defined at 17071) | `reduce_motion` |
| src/ui/reaction_list.rs:115 | `show` | MOTION_QUICK | none |
| src/ui/reaction_list.rs:278 | `list_height` | MOTION_QUICK | screen reader |
| src/ui/reaction_list.rs:309 | `popup` | MOTION_QUICK | screen reader |
| src/ui/settings.rs:2483 | `wallpaper_color_button` | MOTION_QUICK | none |
| src/ui/settings.rs:2598 | `doodle_cards` | MOTION_QUICK | none |
| src/ui/widgets.rs:1145 | `show_over_glass` | MOTION_QUICK | none |
| src/ui/widgets.rs:1160 | `show_over_glass` | literal 0.0 (a reset) | n/a |

Three different reduced-motion gates are in use: `theme::animate` checks the screen reader, some views check `reduce_motion`, and 15 sites check nothing.

## 6. User-visible strings containing "Nothing" with a capital N

| file:line | String | Function |
|---|---|---|
| src/paths.rs:184 | "{} holds a single-account setup, but {} already exists. Nothing was moved; ..." | `adopt_databases` |
| src/settings.rs:633 | "Nothing" (a touchpad-swipe action meaning "do nothing") | `label` |
| src/ui/chats.rs:1100 | "Nothing archived" | `list` |
| src/ui/picker.rs:235 | "Nothing matches" | `rows_for` |
| src/ui/picker.rs:1370 | "Nothing here yet. Right-click any sticker to add it to this pack." | `sticker_tab` |
| src/ui/settings.rs:1206 | "Speak and watch the level. Nothing is recorded or sent." | `voice_section` |

All six use "Nothing" as the ordinary English word at the start of a sentence. None of them is the Nothing OS brand. No theme label contains "Nothing".

## 7. Theme labels

`ThemeChoice` (`src/settings.rs:32`, `label()` at :73):

| Variant | label() | Default |
|---|---|---|
| Dark | "WhatsApp Dark" | **#[default]** |
| Light | "WhatsApp Light" | |
| System | "Follow system" | |
| Messages | "Messages" | |
| NothingDark | "Dark" | |
| NothingLight | "Light" | |
| Red / Blue / Green / Pink / Purple / Yellow | "Red" / "Blue" / "Green" / "Pink" / "Purple" / "Yellow" | |

`ChatTheme` (`src/chat_themes.rs:15`, `label()` at :75):

| Variant | label() | Default |
|---|---|---|
| Zeus | "Zeus" | |
| WhatsApp | "WhatsApp" | **#[default]** |
| Messages | "Messages" | |
| Orchestrate, Vercel, GitHub, One, Catppuccin | same as the variant name | |
| TokyoNight, Dracula, Nord, Gruvbox, RosePine, Solarized, Kanagawa | "Tokyo Night", "Dracula", "Nord", "Gruvbox", "Rosé Pine", "Solarized", "Kanagawa" | |
| VsCode, Monokai, Material, Ayu, NightOwl, Everforest, Vitesse, Flexoki | "VS Code", "Monokai", "Material", "Ayu", "Night Owl", "Everforest", "Vitesse", "Flexoki" | |

## Disagreements to check

1. **Who sees `ThemeChoice` labels?** No file under `src/ui/` names `ThemeChoice`. Its labels are read in `src/demo.rs:3333`, to match `--demo-page` names. On `main`, Settings shows `ChatTheme` cards instead (`src/ui/settings.rs:2036`). So "Dark" meaning the Nothing theme, beside a default called "WhatsApp Dark", may matter only to the demo and saved settings. Confirm before anyone renames them.
2. **The "Nothing" check is case-sensitive.** The brief asked for a capital N, so lowercase "nothing" strings are not listed.
3. **Check 3 is ambiguous.** The brief's patterns (`FontId::proportional/monospace`, `.size(`) find no literal font sizes. All sizes go through `theme::regular/medium/semibold(..)`, so those helper calls are reported as the real count.
4. **Some counts may be incomplete.** The test exclusion is a brace-counting heuristic. A `{` inside a raw string could hide or expose a few lines. One file was checked by hand.
5. **Durations behind a variable.** The tokens behind the local `animation_time` variable at conversation.rs:5977 and :17071 were read by hand. Other variables were followed to their constant definition.

---

# Part B: scores, comparison and fixes

## What the reference images actually are

| Image | What it shows | Caveat |
|---|---|---|
| 1 | Telegram Desktop or Web, an older build (flat blue selection, photo wallpaper) | Not today's Telegram |
| 2 | Discord, an older build (pre-2023 server rail and channel list) | Not today's Discord |
| 3 | A **third-party macOS UI kit** advertised as "WhatsApp, Updated 2025 … +10 editable screens" | Not WhatsApp's own app, and it shows no open chat |

They are scored as shown.

## Rubric (the same for all four)

| Criterion | Weight | What a 10 looks like |
|---|---|---|
| Hierarchy | 20 | One obvious title per pane; name, preview and time read in that order |
| Density and scanning | 20 | Many chats visible, no truncated names, short timestamps |
| Consistency | 20 | One spacing scale, one type scale, one radius vocabulary |
| Affordances and states | 15 | Every control looks clickable; hover, selected and unread are distinct |
| Contrast and readability | 15 | Secondary text readable; own vs others obvious without colour alone |
| Polish and identity | 10 | Feels deliberate and has its own character |

## Scores

| App | Hier. | Density | Consist. | Afford. | Contrast | Polish | **Score /100** |
|---|---|---|---|---|---|---|---|
| Telegram (img 1) | 8 | 9 | 8 | 7 | 7 | 7 | **78** |
| Discord (img 2) | 6 | 9 | 7 | 6 | 6 | 6 | **68** |
| WhatsApp-style kit (img 3) | 9 | 7 | 9 | 8 | 8 | 9 | **83** |
| **Zeus `main`, WhatsApp Dark** | 7 | 6 | 5 | 7 | 7 | 8 | **65** |
| **Zeus `main`, Zeus (Nothing) theme** | 7 | 6 | 5 | 6 | 5 | 8 | **60** |
| Zeus with open PRs merged as they are | 7 | 6 | 5 | 7 | 7 | 8 | **~65** |
| **Zeus after the fixes below** | 8 | 8 | 8 | 8 | 8 | 9 | **~82** |

What each score rests on:

- **Telegram:**
  - Strengths: tight 2-line rows, short times ("7:15 PM", "Mon"), a coloured sender prefix in group previews, and one blue for selection.
  - Weaknesses: green times on light-green bubbles are faint, and the hamburger menu hides navigation.
- **Discord:**
  - Strengths: very dense, and the "NEW MESSAGES" divider is a strong unread anchor.
  - Weaknesses: grey on grey is low contrast, there are many tiny unlabelled icons, and the flat message list has no own/other distinction.
- **WhatsApp-style kit:**
  - Strengths: calm, with one radius family, a clean icon rail, generous but even rows, short times, and a single green badge style.
  - Weakness: the rows are taller than needed.
- **Zeus `main`** (screenshots `chat-dark.png`, `chat-nothing.png`, `settings.png`, `info.png`):
  - Strong: the rail and list structure (very close to the kit), typing in accent colour, the @ and count badges, the info panel's tiles, and the glass and motion.
  - Weak:
    - **Every older chat's date carries the year.** It reads "12 Mar 2026", which truncates names ("Section 8 Ber…", "Katherine Jo…", "Margaret Ha…"). Cause: the chat list's `stamp_relative_to` falls back to `short_date`, which always appends `date.year()` (`src/util.rs:452-455`), even in the current year.
    - **The filter chips clip mid-word** ("Favorit") at the list edge, with no fade or overflow cue.
    - In the Zeus (Nothing) theme, per-message times are dropped and "edited" sits alone on its own row (`chat-nothing.png`). Own and others' bubbles do differ (`#4a4a4a` vs `#252525`), but over glass the difference is small.
    - **Settings shows a back arrow beside a sidebar that already shows where you are.** "Language" sits first on the Appearance page. The Custom/Themes toggle splits one choice into two flows (`settings.png`).
    - **The spelling is mixed:** "Colours"/"Colour" (`settings.rs:492`, `:515`, `labels.rs:79`) beside "Favorites" (`model.rs:277`).
    - **The code has no type or spacing system in practice:** 21 font sizes, and 41% of spacings off the scale.

## The open PRs against these findings

| PR | What it changes for the UI | Effect on the audit | Verdict |
|---|---|---|---|
| #79 One theme picker | Drops Custom/Themes; Mode, then theme cards, then Accent, always on | Removes the split flow; one fewer concept | **Improves.** Merge it, then rename `ThemeChoice` leftovers |
| #83 Spring motion | Routes hovers, popups and similar through shared springs; removes 19 direct `animate_*` calls | Fixes most of check 5's bypasses and gates them on one helper | **Improves.** But its `SNAPPY` preset has bounce, while DESIGN.md and `theme.rs:735` say "no overshoot". Update the doc or drop the bounce |
| #77 Springy overscroll | Rubber band, bounce 0.2 | Adds overshoot, the same drift as #83. Correctly off with reduced motion | Fine, if the doc is updated |
| #85 Sent / Delivered / Read words | Replaces ticks with words; adds `theme::regular(10.0)` | Adds a 22nd font size (10, below micro 11). Words are clearer, but take about 3x a tick's width in every footer; Telegram and WhatsApp keep ticks | **Mixed.** Use 11 (micro); consider the word only on the last own message, ticks elsewhere |
| #81 Restore menu rows | Puts Reply, Forward, Select, Message info, Reload and Copy ID back; insets the send button 6 pt | Conflicts with #84 | **Conflict** (below) |
| #84 Trim menu (stacked on #81) | Drops Reply, Forward, Reload, Copy ID, Open… | A third menu definition | **Conflict** |
| #75 Glass for WhatsApp | Frosted header, composer and menus in the default theme | More glass by default (Glass 0.3); check contrast of header text over busy wallpapers | Probably fine; re-shoot after merging |
| #87 Rename to Zeus | Strings only | No design effect | Merge |
| #88 Upstream features, part 1 | Adds dialogs and rows | Adds 5 spacings (6 and 10 are off-scale), 16 font sizes (7 of them 13.5), and a 4.0 radius | Adds debt. Map it to tokens while merging |
| #68 First-run setup (CONFLICTING) | New setup flow with a **Fraunces serif** hero | 37 new `add_space` (1, 2, 5, 10, 11, 14, 18 and 22 are off-scale); 42 new sizes, including 10, 10.5, 11.5 and 12.5; a second typeface | **Most debt per line.** Tokenise before reviving |
| #64 Split view and bubbles (stacked on #56) | Very large | Diff not readable here | Not audited |

**The message menu has three definitions at once:**

- `main` (`1a2da501`, from Pratyaksh's merge) removed Reply, Forward, Select, Message info, Reload and Copy ID.
- #81 restores all of them.
- #84, stacked on #81, keeps Select and Message info and drops the rest.

Decide on one list before merging either PR.

## Fixes: before and after

These are proposals; no code has changed. Each is small, and most can be done by search and replace inside one file.

### 1. A real type scale (largest win for consistency)

Add named sizes to `src/theme.rs`, and replace the literals in views with them.

| Before (literal) | Count | After |
|---|---|---|
| 10, 10.5, 11, 11.5 | 43 | `MICRO` 11 |
| 12, 12.5 | 123 | `CAPTION` 12 (12.5 rows that hold a message preview become `CONTROL` 13) |
| 13, 13.5 | 163 | `CONTROL` 13 |
| 14, 14.5, 15 | 82 | `BODY` 14 |
| 16, 17, 18 | 37 | `SECTION` 16 |
| 19, 20, 22 | 12 | `TITLE` 20 |
| 24, 26, 28, 30 | 8 | `DISPLAY` 24; emoji-only messages and the call screen keep a named exception |
| `text_styles` Small 11.5, Button 14, Heading 22 | 3 | Small 11, Button 13, Heading 20 |

**Done, with these differences from the table:**

- 13.5 went to `BODY` 14, not 13, because it is the composer and mention text, which should match message bodies.
- 11.5 went to `CAPTION` 12, the chat-list timestamps.
- 15 and 17 went to `SECTION` 16.
- 18 went to `TITLE` 20, apart from the Settings section heading, which went to 16.
- egui's `Button` style stays 14: changing it would resize every egui button, and that needs its own look.
- Two literals stay, with a comment saying why: the emoji glyph (22) and the pairing code (30).

The result is 7 sizes plus named exceptions, down from 21. Telegram and the WhatsApp-style kit each show about 4 sizes on screen.

### 2. Spacing on the scale

| Before | Count | After |
|---|---|---|
| 2 | 16 | 4, or a named `HAIRLINE_GAP` where it aligns a baseline |
| 6 | 36 | 4 inside a component, 8 between items |
| 10 | 34 | 8, or 12 between a label and its control |
| 14, 18, 22 | 13 | 12/16, 16/20, 24 |
| 48 | 1 | 40 |
| -5, -1 | 2 | Keep, with a comment saying why (overlap correction) |

### 3. Radii as tokens

| Before | After |
|---|---|
| conversation.rs:4592 `same(20)` | `RADIUS_MENU` |
| conversation.rs:11711 `8.0` | `RADIUS_SMALL` |
| conversation.rs:10467 `10` | A `BUBBLE_RADIUS` const (DESIGN.md already says bubbles are 10) |
| conversation.rs:11042 `6.0`, 11637 `4.0` | `RADIUS_SMALL` less the inset, per DESIGN's nesting rule (a named `nested(RADIUS_SMALL, inset)` helper) |
| new_group.rs:193 `15` | chip height / 2 (a pill), so it follows the chip's height |
| settings.rs:1359 `3` | Keep, named `METER_RADIUS` |

### 4. Colours as palette roles

| Before | After |
|---|---|
| conversation.rs:8008 `FOOTER_STAR` #EAB308 | `palette.star`, so dark, light and coloured themes can tune it |
| mod.rs:397/399 wallpaper-page border literals | `palette.border` (or `border_strong`) at alpha 32 |

### 5. One reduced-motion path

Merge #83, then route the remaining direct calls in check 5 through `theme::animate`. That covers the 15 sites with no gate. The helper should check both the screen reader and `reduce_motion`, so the three gates become one.

### 6. The chat list (largest win for scanning)

Before (`chat-dark.png`):

```
Katherine Jo…        11 Mar 2026
✓✓ Pushed 😊
```

After:

```
Katherine Johnson         11 Mar
✓✓ Pushed 😊
```

- In the chat list's stamp (`stamp_relative_to`, `src/util.rs:238`), drop the year when it is the current year: "11 Mar". Keep "11 Mar 2025" for older years. `short_date` itself keeps the year, because "last seen" and the media pane use it. **Done.**
- Give the chip row a fade at its right edge, or let it scroll, so "Favorit" does not cut off mid-word.

### 7. The Zeus (Nothing) theme

- Give own bubbles a slightly lighter or darker grey than others' (a few lightness steps), so ownership is not shown by position alone.
- Put "edited" on the time line ("edited 7:59 PM"), as the WhatsApp theme already does, instead of on its own row.

### 8. Settings

- Merge #79, which removes the Custom/Themes toggle.
- Move Language out of Appearance into System.
- Hide the back arrow while the settings sidebar is visible; keep it for a narrow window.
- Pick one spelling. "Favorites" (`model.rs:277`, `picker.rs:1220`) is American, while "Colours"/"Colour" (`settings.rs:492`, `:515`, `:2325`, `labels.rs:79`) is British. Choose one and change the msgids and their translations together.

### 9. Message status (#85)

- Use 11 (micro) instead of 10.
- Show the word only on the newest own message, and keep ticks on the rest. That keeps #85's clarity without widening every footer.

### 10. DESIGN.md

- Under Interaction and motion, state that springs may bounce up to 0.2 for direct manipulation (overscroll, carry), and not for menus and dialogs. Or remove the bounce from #77 and #83.
- List the type scale above in Typography, with the named exceptions.

## Order to do it in

1. Pick the message menu list, then merge #81 or #84 (not both as they are).
2. Merge #79 and #83.
3. Change the year in chat-list dates, and add the chip fade (a small change).
4. Add type-scale tokens and sweep the views, one file per commit, starting with `conversation.rs`, `dialogs.rs` and `settings.rs`.
5. Fix spacing, radii and colours file by file.
6. Tokenise #88 and #68 while merging them.

After step 4, re-shoot the same demo pages (`chat,dark`, `chat,nothing`, `settings`, `info`) to compare before and after.
