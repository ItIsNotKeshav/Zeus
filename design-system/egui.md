# Building it in egui

How Zeus implements this system, and how to carry it into another egui app. Values are egui logical points.

## Where everything lives

| What | Zeus source |
| --- | --- |
| Palettes, radii, fonts, base widgets | `src/theme.rs`: `Palette`, `RADIUS_*`, `regular`/`medium`/`semibold`/`bold`, `pill_button`, `soft_button`, `icon_button`, `circle_button`, focus rings |
| egui style | `theme::apply(ctx, &palette)`: visuals, text styles, spacing, scroll bars, `animation_time` |
| Composite widgets | `src/ui/widgets.rs`: menus, `FrostedPopup`, `Frost`, `switch`, `glass_slider`, badges, chips, bubbles, `setting_row`, `empty_state`, `search_field` |
| Form controls | `src/ui/controls.rs`: `dropdown`, `option`, `text_field` (one height, one radius) |
| Frosted glass | `src/ui/glass.rs`: an OpenGL paint callback (`glass::shape`, `glass::rounded`) |
| Reduced motion | `src/motion.rs`: `request_frame`, `STEP` |

## Port it in five steps

1. **Copy the palette.** Take `Palette` and its constructors (`nothing_dark`, `nothing_light`, and any others you want), `with_accent`, `on_bubble`, the glass helpers (`frosted`, `menu_glass`, `glass`, `tint`, `glass_edge`) and the shadows (`bubble_shadow`, `float_shadow`, `modal_shadow`, `raised_edge`). Keep the colour maths helpers `blend`, `contrast` and `readable_on`.
2. **Copy the constants.** `RADIUS_SMALL` 8, `RADIUS_CONTROL` 12, `RADIUS` 14, `RADIUS_MENU` 20, `RADIUS_CARD` 20, `RADIUS_DIALOG` 28, panel 24; `POPUP_OPEN` 0.14; `TINT_FIELD` 0.08, `TINT_HOVER` 0.06, `TINT_SELECTED` 0.12; `FOCUS_STROKE_WIDTH` 1.
3. **Apply the style once per palette change** with a copy of `theme::apply`. The parts that make it Zeus:

```rust
visuals.window_corner_radius = CornerRadius::same(RADIUS_DIALOG);
visuals.menu_corner_radius = CornerRadius::same(RADIUS_MENU);
visuals.window_shadow = palette.modal_shadow();
visuals.popup_shadow = palette.float_shadow();
for w in [&mut visuals.widgets.inactive, &mut visuals.widgets.hovered,
          &mut visuals.widgets.active, &mut visuals.widgets.open] {
    w.corner_radius = CornerRadius::same(RADIUS_CONTROL);
    w.bg_stroke = Stroke::NONE;          // flat, no outlines
    w.expansion = 0.0;                   // no grow on hover
}
visuals.selection.bg_fill = palette.accent.gamma_multiply(0.35);
visuals.text_cursor.stroke = Stroke::new(2.0, palette.accent);
visuals.handle_shape = egui::style::HandleShape::Circle;
style.spacing.item_spacing = Vec2::new(8.0, 6.0);
style.spacing.button_padding = Vec2::new(12.0, 6.0);
style.spacing.interact_size = Vec2::new(40.0, 28.0);
style.spacing.menu_margin = egui::Margin::same(6);
style.spacing.window_margin = egui::Margin::same(16);
style.interaction.tooltip_delay = 0.4;
style.animation_time = 0.12;
```

4. **Install the fonts.** Bundle `GoogleSansFlex.ttf` and register it per weight with the `wght` axis at the named weight minus 30, `opsz` 14 and `wdth` 100 (`add_variable` in `theme.rs`). Register Inter as a separate tabular family for timers.
5. **Copy the widgets you need** from `widgets.rs` and `controls.rs`. They only depend on `Palette`, the radius constants and the font helpers.

## Patterns to keep

- **Animate with egui's own clock.** `ctx.animate_bool_with_time_and_easing(id, on, theme::POPUP_OPEN, egui::emath::easing::quadratic_out)` for opens and hovers; `cubic_out` for slides. Only ask for an animation while it can run: a closed popup that keeps animating repaints every frame.
- **Frost under a surface.** Reserve the frost at the bottom of the layer before painting (`widgets::Frost::reserve(ctx, layer, radius)`), paint the surface, then `frost.place(rect)`. For popups use `popup.show_frosted(ctx, |ui| ...)`, which also scales the layer 0.92 to 1 from its anchor (`ctx.transform_layer_shapes`).
- **Glass falls back.** Every glass helper checks `backdrop::blur()`. Without the OpenGL backend or with Blur off, return the solid colour.
- **Focus rings.** Call `theme::reveal_focus(&response)` and register the ring with `theme::focus_outline(ui, id, rect, radius)` so it is painted once, inside the control, in the control's own layer.
- **Accessibility info.** `response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, ui.is_enabled(), label))` on every custom widget; `selected` for toggles.
- **Decorative frames.** Use `motion::request_frame(ctx)` for typing dots, spinners and the recording light, so they step at 10 fps while a screen reader is attached.
- **Readable on anything.** When a colour sits on a fill you don't control (a bubble, an accent), use `readable_on(fill, color, palette.text, 4.5)`.
- **Nest radii.** Inside a surface of radius R with margin m, use R − m.

## Don'ts

- Don't add strokes to egui widgets; Zeus's controls are flat fills.
- Don't hard-code colours in a widget: take a `&Palette`.
- Don't make every button a pill or every surface glass.
- Don't add a font, CSS or a web view to an egui app to match this document.
