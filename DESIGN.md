---
version: "alpha"
name: "ZapFast Native"
description: "A calm, precise, native desktop messaging interface for ZapFast, built with Rust and egui."
colors:
  accent: "#007F68"
  accent-hover: "#006B58"
  accent-pressed: "#005847"
  accent-subtle-light: "#DDF4ED"
  accent-subtle-dark: "#173D34"
  on-accent: "#FFFFFF"
  light-window: "#F7F8F7"
  light-panel: "#F0F2F0"
  light-surface: "#FFFFFF"
  light-surface-raised: "#FFFFFF"
  light-surface-hover: "#E9ECE9"
  light-surface-selected: "#DDEFE9"
  light-text: "#1B211F"
  light-text-secondary: "#5D6863"
  light-text-tertiary: "#69746E"
  light-border: "#DCE2DE"
  light-border-strong: "#BEC9C2"
  light-focus: "#087F68"
  light-danger: "#B42318"
  light-danger-subtle: "#FDE9E7"
  light-success: "#16794B"
  light-warning: "#8A5A00"
  dark-window: "#171B19"
  dark-panel: "#1D2320"
  dark-surface: "#222925"
  dark-surface-raised: "#29312C"
  dark-surface-hover: "#303A34"
  dark-surface-selected: "#25473D"
  dark-text: "#F0F4F1"
  dark-text-secondary: "#B5C0B9"
  dark-text-tertiary: "#8F9C94"
  dark-border: "#35413A"
  dark-border-strong: "#4A5A50"
  dark-focus: "#64D9B8"
  dark-danger: "#FFB4AB"
  dark-danger-subtle: "#512A27"
  dark-success: "#82D9A7"
  dark-warning: "#F3C56B"
typography:
  display:
    fontFamily: "Inter, system-ui"
    fontSize: 24px
    fontWeight: 600
    lineHeight: 1.2
  title:
    fontFamily: "Inter, system-ui"
    fontSize: 20px
    fontWeight: 600
    lineHeight: 1.25
  section:
    fontFamily: "Inter, system-ui"
    fontSize: 16px
    fontWeight: 600
    lineHeight: 1.35
  body:
    fontFamily: "Inter, system-ui"
    fontSize: 14px
    fontWeight: 400
    lineHeight: 1.45
  body-strong:
    fontFamily: "Inter, system-ui"
    fontSize: 14px
    fontWeight: 600
    lineHeight: 1.4
  control:
    fontFamily: "Inter, system-ui"
    fontSize: 13px
    fontWeight: 500
    lineHeight: 1.3
  caption:
    fontFamily: "Inter, system-ui"
    fontSize: 12px
    fontWeight: 400
    lineHeight: 1.35
  micro:
    fontFamily: "Inter, system-ui"
    fontSize: 11px
    fontWeight: 500
    lineHeight: 1.3
rounded:
  none: 0px
  small: 8px
  control: 12px
  medium: 14px
  menu: 20px
  card: 20px
  panel: 24px
  dialog: 28px
  pill: 999px
spacing:
  none: 0px
  hairline: 1px
  xsmall: 4px
  small: 8px
  compact: 12px
  medium: 16px
  large: 20px
  xlarge: 24px
  section: 32px
  major: 40px
components:
  button-primary:
    backgroundColor: "{colors.accent}"
    textColor: "{colors.on-accent}"
    rounded: "{rounded.control}"
    padding: "{spacing.compact}"
    typography: "{typography.control}"
  button-primary-hover:
    backgroundColor: "{colors.accent-hover}"
    textColor: "{colors.on-accent}"
    rounded: "{rounded.control}"
  button-primary-pressed:
    backgroundColor: "{colors.accent-pressed}"
    textColor: "{colors.on-accent}"
    rounded: "{rounded.control}"
  button-secondary:
    rounded: "{rounded.control}"
    padding: "{spacing.compact}"
    typography: "{typography.control}"
  input:
    rounded: "{rounded.control}"
    padding: "{spacing.small}"
    typography: "{typography.body}"
  list-row:
    rounded: "{rounded.medium}"
    padding: "{spacing.small}"
    typography: "{typography.body}"
  dialog:
    rounded: "{rounded.dialog}"
    padding: "{spacing.xlarge}"
    typography: "{typography.body}"
---

## Overview

ZapFast should feel like a carefully made native desktop messaging app: calm, quick, legible, private, and quietly polished. The interface should feel cohesive across conversations, settings, media, account management, and dialogs. It should not look like a generic web dashboard placed inside a desktop window.

The visual direction combines:

- **Native restraint:** use the platform and the existing egui/fastframe architecture rather than imitating browser-based UI patterns.
- **Clear hierarchy:** make conversation content and the next useful action visually obvious.
- **Consistent rhythm:** use a shared spacing scale, repeated alignment lines, and component-specific shape rules.
- **Warm precision:** favor clear typography, balanced density, restrained color, and intentional details over decoration.
- **Functional polish:** loading, reconnecting, empty, error, hover, selected, and disabled states are designed—not left as afterthoughts.

Take inspiration from the craftsmanship and clarity of high-quality Apple, Google, and Microsoft products, but do not copy any product's branding or reproduce a web design system literally. ZapFast needs its own recognizable visual language.

### Source of truth and how to read this document

This document describes **visual intent and target token roles**. The existing application code, pinned dependency versions, and platform constraints determine how that intent is implemented. Before changing styling, inspect the current theme system, `fastframe-theme` usage, egui styles, custom widget helpers, and relevant screen code. Reuse or deliberately migrate existing values; do not duplicate or blindly overwrite them.

The token values above are a proposed starting reference for this fork, not a claim that every current ZapFast screen already uses these colors or dimensions. Compare them with the actual rendered baseline and current theme behavior before applying a broad migration. Preserve user-selectable themes and platform-specific behavior.

The dimensions in this file are design-scale values. When implementing them in egui, map the numeric scale to **logical UI points** and the current API; do not treat these as physical monitor pixels or copy CSS into Rust.

## Colors

### Color principles

- Use color to convey hierarchy, interaction, selection, and status—not to decorate every surface.
- Keep most of the interface neutral. Use the accent sparingly for primary actions, active states, links, and selected indicators when appropriate.
- Use semantic roles (`text-secondary`, `surface-hover`, `danger`, `focus`) rather than choosing arbitrary colors inside components.
- Avoid stacking several colored containers around the same content. One clear surface treatment is usually enough.
- Never communicate status by color alone; pair it with a label, icon, shape, or another distinguishable signal.
- Test contrast in both light and dark themes, including muted text, subtle borders, selected rows, and disabled states.

### Theme behavior

Treat light and dark variants as parallel themes with the same hierarchy. They do not need to be simple mathematical inverses, but they should provide equivalent emphasis and affordances.

- Light theme: use soft neutral window and panel surfaces, white or near-white content surfaces, dark primary text, and quiet separators.
- Dark theme: use layered charcoal/green-black surfaces rather than pure black everywhere; preserve readable text and visible boundaries without outlining every object.
- Keep accent intent consistent across themes while adjusting contrast as needed.
- Respect existing system-theme selection and any supported custom palette. Do not hardcode one of these sample colors into individual widgets.
- Use the current application's established palette as the implementation baseline unless a task explicitly approves a palette change.

### Semantic roles

- **Window:** the app's broadest background region.
- **Panel:** a secondary region such as a sidebar or settings rail.
- **Surface:** content that needs a distinct background, such as a menu, input, or conversation area.
- **Raised surface:** a dialog or floating surface that must read above its parent.
- **Hover surface:** temporary feedback for a pointer hovering an interactive target.
- **Selected surface:** a stable selected/navigation state; must remain visibly distinct from hover.
- **Primary text:** titles, message bodies, and key values.
- **Secondary text:** timestamps, descriptions, and supporting labels.
- **Tertiary text:** low-priority metadata only; never use it for essential instructions or critical status.
- **Border:** subtle separation when spacing or surface contrast alone is insufficient.
- **Focus:** a visible keyboard-focus indicator that is not dependent on hover.
- **Danger / success / warning:** consistent semantic feedback for errors, success, and caution.

## Typography

Typography must make conversations easy to read for long periods and let the user scan lists quickly. Prefer the existing bundled/platform font system. Inter is a design reference where supported; do not introduce a new font dependency or replace platform font behavior merely to match this document.

### Hierarchy

- Use one clear title for the current screen or dialog, not several competing headings.
- Use section headings to divide settings or grouped options.
- Use body text for messages and explanations; use stronger weight sparingly for names, labels, and important values.
- Use caption styles for timestamps and secondary metadata, but keep them readable at typical desktop scaling.
- Do not create hierarchy by making every label a different size or weight. Prefer a small, repeatable type scale.
- Avoid all-caps labels unless they are an established platform convention and remain easy to scan.

### Alignment and behavior

- Align related text baselines and icon centers deliberately.
- Keep timestamp, sender, message-preview, and status positions consistent across equivalent rows.
- Handle long names, URLs, filenames, and translated strings without clipping essential content. Prefer natural wrapping or deliberate truncation with access to the full value.
- Use correct text direction and Unicode shaping; never assume all message content is left-to-right.
- Preserve text selection and copy behavior in messages and other content users reasonably expect to select.
- Test long, short, mixed-script, and emoji-heavy content when editing text layout.

## Layout

### Spacing system

Use the spacing scale in the front matter as a common rhythm, not as a rigid grid that overrides content needs.

- **4:** small gap between tightly related items, such as an icon and its label.
- **8:** compact padding, metadata gaps, and closely related controls.
- **12:** comfortable control padding or separation inside a component.
- **16:** standard panel/content padding and separation between related groups.
- **20–24:** separation between major groups or a dialog's content regions.
- **32–40:** larger structural separation, used only when the available space supports it.

Prefer fewer, repeated spacing values over a large collection of near-duplicates. A value outside the scale is acceptable when it solves a demonstrated alignment, typography, density, or platform issue. Explain the exception when adding it to shared styling code.

### Proximity and grouping

- Items closer together should have a clear relationship; use larger gaps to separate unrelated groups.
- Use whitespace before adding divider lines or extra cards.
- Align repeated content to common edges. A consistent left edge often matters more than decorative separators.
- Group related controls together and keep help text close to the control it describes.
- Avoid nested panels/cards when a simple layout, heading, or spacing would communicate the same grouping.

### Desktop layout

- Treat the app as a resizable desktop window, not a fixed screenshot.
- Use the available space efficiently. Do not make every region expand equally when one region contains primary content and another contains navigation or metadata.
- Keep primary message content and the composer visually connected.
- Keep sidebars, conversation headers, toolbars, and message content aligned to intentional layout edges.
- Avoid both extremes: cramped rows that impair scanning and oversized padding that wastes space in a messaging client.
- At narrow window sizes, prioritize the active conversation and core actions. Reduce secondary spacing before allowing critical controls to clip or disappear.
- Do not assume a single fixed window size or display scale. Review a normal window, a narrow window, and a large window after structural layout changes.

### Density

ZapFast is a communication tool, so useful information density matters. Density should be deliberate, not accidental.

- Keep conversation lists scannable, with stable row rhythm and clear separation among name, preview, timestamp, unread status, and indicators.
- Use compact spacing for metadata and controls when they remain easy to distinguish.
- Give message text enough line length and breathing room to read comfortably.
- Avoid adding blank space simply to make a screen look luxurious. Space should establish relationships and hierarchy.
- Do not tighten every gap uniformly; text, icons, controls, and whole sections have different spacing needs.

## Elevation & Depth

Use elevation and layering only when they communicate a meaningful relationship in the interface.

- Prefer surface contrast and spacing over shadows for ordinary layout separation.
- Reserve stronger depth cues for truly floating or modal surfaces, such as dialogs, menus, popovers, and transient overlays.
- Keep borders and shadows subtle. Avoid placing both a strong border and a strong shadow around every surface.
- Use a consistent layering order for the main window, menus, popovers, dialogs, and temporary notifications.
- A popover or menu should visually connect to the control that opened it when the toolkit makes that possible.
- Do not invent a web-style card for each group of content. Native desktop interfaces often benefit from simpler, flatter composition.

## Shapes

Corner radii create a visual vocabulary. They should indicate component role, not merely make every element appear modern.

### Shape rules

Zeus rounds as generously as iOS 26's glass, and nests shapes the same way: a shape inside a rounded surface takes the surface's radius less the room between them, so the two curves run together (a menu item's 14 inside a menu's 20 with its 6-point margin). The values live in `src/theme.rs` (`RADIUS_SMALL`, `RADIUS_CONTROL`, `RADIUS`, `RADIUS_MENU`, `RADIUS_CARD`, `RADIUS_DIALOG`) and `chats::CARD_RADIUS`.

- **Small (8):** badges, thumbnails, swatches, and small tiles.
- **Control (12):** buttons and egui's own widgets.
- **Medium (14):** fields, selectable rows, toasts, and items within a menu.
- **Menu (20):** menus and popovers, over frosted glass.
- **Card (20):** settings groups, call tiles, and previews.
- **Panel (24):** the conversation card beside the list.
- **Dialog (28):** dialogs, the emoji pickers, and other large surfaces that float over the window, over frosted glass.
- Message bubbles: 10, and 18 for Nothing's grouped balloons (4 where two in a run meet); Messages keeps Apple's own balloon. Bubbles stay tighter than panels on purpose.
- **Pill:** tags, compact status chips, or genuinely pill-shaped controls only. Do not make all buttons pill-shaped by default.
- **Circle:** avatars and circular icon controls where established by the product or platform.

These are target roles, not instructions to change every current radius mechanically. Tune radii with border, padding, hit area, and neighboring shapes in view.

### Avoid shape noise

- Do not put a rounded rectangle around every icon, label, row, group, and section.
- Avoid a radius so large that a rectangular control becomes visually inflated or loses its alignment with neighboring controls.
- Adjacent controls should not look like unrelated floating pills unless that is the intended interaction model.
- Where components touch or share a background, account for their relationship so borders and radii do not create awkward double-rounded seams.
- Preserve native-looking focus and selection affordances.

## Components

Component rules are shared defaults. A screen may depart from them when the content, platform convention, or interaction genuinely requires it. Prefer a shared widget/helper over repeated local styling.

### Buttons and icon buttons

- Maintain consistent height, internal padding, icon size, label baseline, and corner radius across equivalent buttons.
- Make the primary action visually clear; use secondary, subtle, or destructive treatment for other actions as appropriate.
- Avoid making every action use the accent color.
- Icon-only controls need a recognizable icon, a useful tooltip or accessible name, a clear hover/focus state, and a practical interaction area.
- Do not use glyphs or icons whose meaning is ambiguous without context.
- Disabled buttons should remain legible enough to understand, while clearly appearing unavailable.

### Inputs and search

- Use consistent input height, padding, border treatment, radius, and text style.
- Keep placeholder text visually secondary without making it too faint to read.
- Distinguish idle, hover, keyboard focus, invalid, and disabled states.
- Keep search controls easy to find and avoid placing unrelated actions inside them without a clear interaction design.
- Error messages should explain what happened and, where possible, how to recover.

### Conversation list rows

- Use a stable row layout for avatar, display name, last-message preview, timestamp, unread count, pin/mute state, and delivery/connection indicators as applicable.
- Align equivalent items consistently from row to row.
- Make selected, hovered, and keyboard-focused rows distinguishable from each other.
- Do not rely only on a small color change to show selection.
- Keep unread emphasis noticeable but restrained; do not make every metadata item compete with the contact name.
- Treat long names and previews consistently, including mixed-script content and emoji.

### Message timeline and bubbles

- Message content is the primary visual content of the active conversation.
- Establish a consistent relationship between sender identity, message body, timestamp, reply context, reactions, status, and attachments.
- Use bubble shape, background, and alignment to clarify authorship without making the conversation visually noisy.
- Do not force every type of message into the exact same geometry. Media, voice messages, polls, replies, system events, and long text have different content needs.
- Maintain readable text line lengths, stable spacing between consecutive messages, and clear grouping when messages belong together.
- Make quoted/replied-to content visibly subordinate to the current message but still readable.
- Reactions, delivery indicators, and timestamps must not collide with text or become clipped at narrow widths.
- Support long URLs, RTL scripts, emoji sequences, and media dimensions without breaking the timeline layout.

### Composer and attachments

- Visually connect the composer to the active conversation without merging it into the message content.
- Keep text entry, attachment actions, emoji/sticker controls, voice controls, and send action spatially coherent.
- Ensure the send action is distinguishable when enabled and unavailable when there is no sendable content, according to the product's behavior.
- A multiline draft should grow or scroll predictably and must not obscure the latest conversation messages.
- Preserve keyboard shortcuts, focus order, text selection, clipboard operations, and the expected Enter/Shift+Enter behavior.
- Use tooltips or supporting labels for uncommon icon-only controls.

### Menus, popovers, and dialogs

- Menus should be compact, clearly grouped, and aligned to their anchor when practical.
- Use separators only where groups genuinely differ.
- Dialogs should have a clear title, concise explanatory text, and a clear primary action. Destructive actions must be explicit.
- Keep dialog width appropriate for the content. Do not turn a small confirmation into a large empty panel.
- Preserve keyboard focus, Escape behavior where appropriate, and predictable dismissal rules.
- Distinguish modal content from the underlying window without excessive borders and shadows.

### Settings and forms

- Group related settings into clear sections with consistent headings and spacing.
- Align labels, controls, descriptions, and validation messages consistently.
- Keep explanations close to the setting they explain.
- Do not create a card around every individual setting; use section spacing and headings first.
- Destructive, security-sensitive, and account-related settings should explain their consequences clearly.
- Controls that represent the same type of choice should behave and look alike across the application.

### Empty, loading, error, and connection states

- Every important view should have a considered empty state, loading state, error state, and unavailable/disconnected state where relevant.
- Prefer concise, actionable explanations over decorative illustrations or generic messages.
- Keep the layout stable when content loads; avoid unnecessary jumps and flashing.
- Do not display a success state unless the operation actually succeeded.
- Error states should distinguish between a recoverable issue, a retryable network problem, and an action requiring user intervention.
- Avoid exposing private message content, phone numbers, authentication material, QR payloads, or other sensitive information in logs or screenshots.

### Notifications and transient feedback

- Use transient feedback for actions whose outcome is otherwise not obvious, such as saving a preference or copying a value.
- Feedback must be noticeable without interrupting normal messaging work.
- Do not show repeated notifications for the same event or use animation that distracts from reading.
- Respect system permissions and platform behavior; the app must not imply it can override OS notification settings.

## Interaction and motion

- Every interactive element needs clear idle, hover, pressed, focus, disabled, and selected states as applicable.
- Hover must not be the only way to discover or operate an action; keyboard users need equivalent access.
- Focus must remain visible and distinct from selection and hover.
- Use motion only to explain a state change, establish continuity, or provide useful feedback. Avoid decorative animation that increases cognitive load or harms responsiveness.
- Keep animations short, restrained, and interruptible where relevant.
- Avoid sudden layout movement, jitter, repeated pulsing, or animation that makes scrolling and text selection feel unstable.
- Respect reduced-motion or platform preferences when supported by the current architecture.

## Accessibility and localization

- Maintain readable contrast for all essential text and interactive states in both themes. Aim for WCAG AA text contrast where the relevant measurements apply: 4.5:1 for normal text and 3:1 for large text. Treat these as a minimum check, not the whole of accessibility.
- Do not rely on color alone for selected, unread, error, success, or warning states.
- Preserve keyboard access, focus visibility, logical focus order, and established shortcuts.
- Keep icon hit areas practical for desktop use even where the visible icon itself is small.
- Respect user font/display scaling and ensure the UI remains legible at common OS scaling levels.
- Test translated and longer strings; do not size a control only for the English label.
- Ensure RTL message content and mixed bidirectional text do not corrupt alignment or truncation.
- Avoid unnecessary flashing and motion. Ensure interactive states are perceivable without precise pointer movement.

## Cross-platform and native implementation

ZapFast is a native Rust/egui application, not a browser-based application. Implement design intent using the actual APIs available in the pinned egui/eframe/fastframe versions.

- Inspect existing helpers and theme abstractions before writing new ones.
- Keep the existing system-theme, custom-theme, and platform-specific behavior working.
- Do not introduce CSS, Tailwind, browser-only concepts, or new UI frameworks for a task that belongs in the existing native UI.
- Do not replace an established widget with a custom-painted imitation solely to match a screenshot unless there is a concrete, reviewed benefit.
- Keep platform-specific behavior isolated and verify that shared components still work on Windows, macOS, and Linux.
- Avoid blocking the UI thread with network, disk, or media operations.
- Avoid unnecessary dependencies. New dependencies must have a concrete value that justifies their build, size, security, and maintenance costs.
- Keep personal message data and authentication information out of diagnostics and visual-review artifacts.

## Do's and Don'ts

### Do

- Inspect the existing implementation before changing it.
- Use shared semantic tokens and components for repeated design decisions.
- Fix alignment and spacing at the source when several components share the same underlying inconsistency.
- Use screenshots and concrete reference images to communicate visual intent.
- Compare before and after at the same window size and theme.
- Make the smallest coherent change that addresses the stated problem.
- Preserve working behavior and test functional paths affected by a UI change.
- Explain exceptions when a component needs a different spacing, radius, density, or interaction pattern.
- Distinguish a verified result from a visual assumption.

### Don't

- Blindly replace every numeric padding, margin, or radius value with one token.
- Give every control, row, panel, bubble, and dialog the same corner radius.
- Put every section into a rounded card or add shadows/borders by default.
- Add gradients, glow, blur, glass effects, or accent-colored containers without a product reason. Menus, dialogs and pickers float over frosted glass (`ui::glass`), and Settings' Blur turns it off.
- Change brand colors, fonts, or global theme behavior during a narrow layout fix.
- sacrifice readability or useful message density to make a screenshot look spacious.
- Remove keyboard behavior, text selection, accessibility, or platform conventions for visual reasons.
- Claim that a screen was visually tested if no rendered screenshot or live application was inspected.
- make unrelated code cleanups in the same patch as a UI change.

## Workflow for AI-assisted UI changes

Use this workflow when implementing or reviewing UI changes. It is intentionally designed to reduce broad, inconsistent edits.

### 1. Inspect

- Read `AGENTS.md`, this file, and any relevant product/architecture guidance.
- Inspect the relevant screen, style/theme code, custom widgets, and callers.
- Identify the existing component that is closest to the requested design.
- For a repository-wide consistency task, begin with a read-only audit; for a focused, unambiguous bug, work directly within the requested scope.

### 2. Define the task

For each change, identify:

- The current problem, with a file/component reference or screenshot if possible.
- The intended visual or behavioral outcome.
- The affected screens and component states.
- Functional constraints that must not change.
- How the result will be verified.

Prefer one screen or coherent component family per task. Do not start with a full-app redesign unless the task explicitly calls for a planned migration.

### 3. Plan proportionately

For a wide refactor, report findings and propose a prioritized plan before editing. For a small fix, state the approach briefly and proceed. Do not spend more effort on planning ceremony than the task warrants.

### 4. Implement

- Reuse existing tokens/helpers; if repeated styles have no shared home, propose a small design-system abstraction.
- Change one coherent area at a time.
- Avoid unrelated refactors, arbitrary new constants, blind search-and-replace, and dependency changes.
- Preserve existing interactions, platform behavior, and data handling.

### 5. Inspect the rendered result

Use the synthetic demo facilities documented by the repository. Typical entry points include:

```sh
cargo run --features demo -- --demo
```

Where supported, capture a headless screenshot with the application's documented `--demo-shot` options. Inspect the actual image; generating a screenshot file is not equivalent to reviewing it.

For meaningful visual changes, compare before and after with the same demo state, window dimensions, theme, and scaling. Review both light and dark themes and a narrow window when relevant. Use synthetic content only—never real conversations, phone numbers, QR codes, or personal media.

### 6. Verify behavior and code quality

Run the narrowest useful checks while iterating, then run the relevant full checks before declaring completion. The repository's current contributing guide is the source for the exact commands. The established baseline includes:

```sh
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets
cargo test --locked --all-targets --all-features
RUSTDOCFLAGS='-D warnings' cargo doc --locked --all-features --no-deps
```

Run platform-specific checks where code paths changed. Do not claim a check passed unless it was actually run and its result is known.

### 7. Review and report

Before finishing:

- Inspect the final diff for unrelated edits and duplicated styling decisions.
- Check that the same component still looks and behaves consistently in other screens.
- Review the rendered screenshots where applicable.
- Summarize files changed, design decisions, checks actually run, screenshots reviewed, and known limitations.
- If a visual requirement is subjective or under-specified, make a reasonable restrained choice and state the assumption rather than silently expanding the task.

## Definition of done

A visual change is complete when all applicable items are true:

- [ ] The stated problem is fixed in the intended component(s).
- [ ] Related components use consistent shared styling where appropriate.
- [ ] Spacing and alignment are intentional at common window sizes.
- [ ] Corner radii, borders, and elevation fit the role of each surface.
- [ ] Light and dark themes remain readable and coherent.
- [ ] Hover, focus, pressed, selected, disabled, error, and loading states are preserved or deliberately improved.
- [ ] Text, icon, timestamp, and control alignment remains correct with real-world content lengths.
- [ ] Keyboard behavior, text selection, and accessibility have not regressed.
- [ ] Relevant formatting, lint, and tests have been run.
- [ ] Before-and-after screenshots have been reviewed for significant visual changes.
- [ ] The diff contains no unrelated cleanup or unrequested redesign.
- [ ] The completion report is truthful about what was and was not verified.

## References and design influences

This specification synthesizes general design principles and AI-agent workflow practices from the following resources. It is an independent ZapFast-oriented adaptation, not an official design system from any of these organizations.

- [Google Stitch DESIGN.md format](https://github.com/google-labs-code/design.md) — machine-readable tokens paired with human-readable rationale; token linting and diffing.
- [Google Labs announcement of DESIGN.md](https://blog.google/innovation-and-ai/models-and-research/google-labs/stitch-design-md/) — portable design rules for coding agents.
- [AllenAI design systems](https://github.com/allenai/design) — shared foundations plus product-specific design overlays.
- [VoltAgent DESIGN.md collection](https://github.com/VoltAgent/design-md) — community analyses of product visual systems; inspiration rather than official source material.
- [Microsoft Fluent 2 layout](https://fluent2.microsoft.design/layout) — spacing, grouping, alignment, and visual hierarchy.
- [Microsoft Fluent 2 shapes](https://fluent2.microsoft.design/shapes) — using shape, fill, border, and radius with consistent intent.
- [Apple Human Interface Guidelines: Layout](https://developer.apple.com/design/human-interface-guidelines/layout) — hierarchy, alignment, and platform-aware composition.
- [Apple Human Interface Guidelines: macOS](https://developer.apple.com/design/human-interface-guidelines/designing-for-macos) — desktop interaction and layout considerations.
- [Anthropic frontend-design skill](https://github.com/anthropics/skills/blob/main/skills/frontend-design/SKILL.md) — choosing a deliberate aesthetic and reviewing visual implementation. Its web-specific instructions should not be copied directly into this native application.
