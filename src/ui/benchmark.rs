//! The benchmark in Settings > About: a button that runs it in a second
//! window, its progress, the results, saving them, and sharing them as a
//! prefilled GitHub issue after showing exactly what the issue will hold.

use egui::{CornerRadius, Frame, Margin, Rect, Sense, Stroke, Vec2, vec2};

use crate::app::App;
use crate::benchmark::{self, Report, Stage};
use crate::i18n::gettext;
use crate::model::Action;
use crate::theme::{self, Icon, Palette};

/// How often the window redraws while the benchmark runs: enough for the
/// progress bar, little enough not to weigh on what is measured.
const PROGRESS_REDRAW: std::time::Duration = std::time::Duration::from_millis(250);

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    let locale = app.locale;
    // A hairline sets the benchmark apart from the version and credits.
    let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(line, 0.0, palette.outline);
    ui.add_space(16.0);
    theme::text(
        ui,
        gettext(locale, "Performance benchmark"),
        theme::semibold(theme::TEXT_BODY),
        palette.text,
    );
    ui.add_space(4.0);
    theme::paragraph(
        ui,
        gettext(
            locale,
            "A two-minute test in a second Zeus window with built-in sample chats. It scrolls, \
             switches chats, opens the sticker picker and sits idle, and measures smoothness, \
             memory, CPU and GPU. Your chats and account are not used, and nothing leaves this \
             computer unless you share it.",
        ),
        theme::regular(theme::TEXT_CAPTION),
        palette.secondary,
    );
    ui.add_space(10.0);
    let stage = app.benchmark.as_ref().map(benchmark::Benchmark::stage);
    let running = matches!(stage, Some(Stage::Running { .. }));
    // The progress fades in as the run starts, the results as it ends.
    let shown = theme::animate(
        ui.ctx(),
        egui::Id::new("benchmark-running"),
        running,
        theme::MOTION_PANEL,
    );
    // Worked out on every frame, so the first results fade in too.
    let finished = theme::animate(
        ui.ctx(),
        egui::Id::new("benchmark-results"),
        matches!(stage, Some(Stage::Done(_))),
        theme::MOTION_PANEL,
    );
    match stage {
        Some(Stage::Running {
            phase,
            since: _,
            began,
        }) => {
            ui.ctx().request_repaint_after(PROGRESS_REDRAW);
            progress(ui, app, &palette, phase.as_deref(), began, shown);
        }
        Some(Stage::Done(report)) => results(ui, app, &palette, &report, finished),
        Some(Stage::Failed(reason)) => {
            theme::paragraph(
                ui,
                reason,
                theme::regular(theme::TEXT_CONTROL),
                palette.danger,
            );
            ui.add_space(8.0);
            run_button(ui, app, &palette, "Try again");
        }
        Some(Stage::Stopped) => {
            theme::paragraph(
                ui,
                gettext(locale, "Stopped before the end."),
                theme::regular(theme::TEXT_CONTROL),
                palette.secondary,
            );
            ui.add_space(8.0);
            run_button(ui, app, &palette, "Run benchmark");
        }
        None => run_button(ui, app, &palette, "Run benchmark"),
    }
    ui.add_space(6.0);
}

fn run_button(ui: &mut egui::Ui, app: &mut App, palette: &Palette, label: &'static str) {
    if theme::soft_button(
        ui,
        palette,
        Some(Icon::Timer),
        &gettext(app.locale, label),
        false,
    )
    .clicked()
    {
        app.actions.push(Action::StartBenchmark);
    }
}

/// The bar, what is under way, and Stop.
fn progress(
    ui: &mut egui::Ui,
    app: &mut App,
    palette: &Palette,
    phase: Option<&str>,
    began: Option<std::time::Instant>,
    shown: f32,
) {
    let locale = app.locale;
    ui.scope(|ui| {
        ui.multiply_opacity(shown);
        let elapsed = began.map_or(0.0, |began| began.elapsed().as_secs_f32());
        let length = benchmark::LENGTH.as_secs_f32();
        let target = (elapsed / length).clamp(0.0, 0.99);
        // Glides between redraws instead of stepping a quarter second at a time.
        let fraction = ui.ctx().animate_value_with_time(
            egui::Id::new("benchmark-progress"),
            target,
            theme::MOTION_PANEL,
        );
        let width = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(vec2(width, 6.0), Sense::hover());
        let painter = ui.painter();
        painter.rect_filled(rect, 3.0, palette.surface_active);
        let filled = Rect::from_min_size(rect.min, vec2(rect.width() * fraction, rect.height()));
        painter.rect_filled(filled, 3.0, palette.accent);
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            theme::spinner(ui, 14.0, palette.accent);
            let doing = gettext(locale, benchmark::phase_label(phase.unwrap_or_default()));
            let clock = |seconds: f32| {
                let seconds = seconds.max(0.0) as u32;
                format!("{}:{:02}", seconds / 60, seconds % 60)
            };
            let line = gettext(locale, "{doing} · {elapsed} of about {total}")
                .replace("{doing}", &doing)
                .replace("{elapsed}", &clock(elapsed))
                .replace("{total}", &clock(length));
            theme::text(ui, line, theme::medium(theme::TEXT_CONTROL), palette.text);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if theme::soft_button(ui, palette, Some(Icon::X), &gettext(locale, "Stop"), false)
                    .clicked()
                {
                    app.actions.push(Action::StopBenchmark);
                }
            });
        });
        ui.add_space(6.0);
        theme::paragraph(
            ui,
            gettext(
                locale,
                "Leave the benchmark window alone until it closes by itself. Using the computer \
                 meanwhile changes the numbers.",
            ),
            theme::regular(theme::TEXT_CAPTION),
            palette.secondary,
        );
    });
}

/// The numbers, then Save, Share and Run again, and the share preview.
fn results(ui: &mut egui::Ui, app: &mut App, palette: &Palette, report: &Report, shown: f32) {
    let locale = app.locale;
    ui.scope(|ui| {
        ui.multiply_opacity(shown);
        // Rises into place as it fades in.
        ui.add_space((1.0 - shown) * 6.0);
        theme::paragraph(
            ui,
            report.machine_line(),
            theme::regular(theme::TEXT_CAPTION),
            palette.secondary,
        );
        ui.add_space(10.0);
        smoothness_table(ui, palette, locale, report);
        ui.add_space(12.0);
        phase_table(ui, palette, locale, report);
        ui.add_space(8.0);
        let mut notes = vec![
            gettext(locale, "Peak memory {peak} MB.").replace("{peak}", &number(report.peak_mb)),
            gettext(locale, "CPU 100% is one core busy.").to_string(),
        ];
        if report.gpu_metric == "whole_gpu" {
            notes.push(
                gettext(
                    locale,
                    "GPU is the whole graphics chip, as macOS reports it.",
                )
                .into(),
            );
        }
        theme::paragraph(
            ui,
            notes.join(" "),
            theme::regular(theme::TEXT_CAPTION),
            palette.secondary,
        );
        if !report.checks_passed() {
            ui.add_space(6.0);
            theme::paragraph(
                ui,
                gettext(
                    locale,
                    "Some of the test's checks failed, so these numbers may not compare with \
                     other runs.",
                ),
                theme::regular(theme::TEXT_CAPTION),
                palette.warning,
            );
        }
        ui.add_space(12.0);
        let sharing = app.benchmark_share;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            if theme::soft_button(
                ui,
                palette,
                Some(Icon::Download),
                &gettext(locale, "Save results…"),
                false,
            )
            .clicked()
            {
                app.actions.push(Action::SaveBenchmark);
            }
            if theme::soft_button(
                ui,
                palette,
                Some(Icon::ExternalLink),
                &gettext(locale, "Share on GitHub…"),
                sharing,
            )
            .clicked()
            {
                app.benchmark_share = !sharing;
            }
            run_button(ui, app, palette, "Run again");
        });
        if let Some(saved) = app.benchmark.as_ref().and_then(benchmark::Benchmark::saved) {
            ui.add_space(6.0);
            let (text, color) = match saved {
                Ok(path) => (
                    gettext(locale, "Saved to {path}.")
                        .replace("{path}", &path.display().to_string()),
                    palette.secondary,
                ),
                Err(error) => (
                    gettext(locale, "Could not save the results: {error}")
                        .replace("{error}", &error),
                    palette.danger,
                ),
            };
            theme::paragraph(ui, text, theme::regular(theme::TEXT_CAPTION), color);
        }
        share_preview(ui, app, palette, report, sharing);
    });
}

/// Exactly what the GitHub issue will hold, and the buttons that open it or
/// copy it. Opens and closes with the panels' motion.
fn share_preview(ui: &mut egui::Ui, app: &mut App, palette: &Palette, report: &Report, open: bool) {
    let locale = app.locale;
    let shown = theme::animate(
        ui.ctx(),
        egui::Id::new("benchmark-share"),
        open,
        theme::MOTION_PANEL,
    );
    if shown <= 0.0 {
        return;
    }
    let text = report.markdown();
    ui.add_space(12.0 * shown);
    ui.scope(|ui| {
        ui.multiply_opacity(shown);
        Frame::new()
            .fill(palette.surface)
            .stroke(Stroke::new(1.0, palette.outline))
            .corner_radius(CornerRadius::same(theme::RADIUS_CONTROL))
            .inner_margin(Margin::same(14))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                theme::paragraph(
                    ui,
                    gettext(
                        locale,
                        "Opening the issue sends nothing by itself: GitHub shows this text in \
                         your browser, and it is shared only if you submit it there. The issue \
                         is public and shows your GitHub name. It holds only what is below.",
                    ),
                    theme::regular(theme::TEXT_CAPTION),
                    palette.text,
                );
                ui.add_space(10.0);
                egui::ScrollArea::vertical()
                    .id_salt("benchmark-share-text")
                    .max_height(220.0 * shown)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(&text)
                                    .font(egui::FontId::monospace(11.5))
                                    .color(palette.secondary),
                            )
                            .wrap_mode(egui::TextWrapMode::Wrap),
                        );
                    });
                ui.add_space(10.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                    if theme::soft_button(
                        ui,
                        palette,
                        Some(Icon::ExternalLink),
                        &gettext(locale, "Open GitHub issue"),
                        true,
                    )
                    .clicked()
                    {
                        app.actions.push(Action::OpenUrl(report.issue_url()));
                    }
                    if theme::soft_button(
                        ui,
                        palette,
                        Some(Icon::Copy),
                        &gettext(locale, "Copy text"),
                        false,
                    )
                    .clicked()
                    {
                        app.actions.push(Action::CopyText(text.clone()));
                    }
                });
            });
    });
}

fn number(value: Option<f64>) -> String {
    value.map_or_else(|| "–".to_owned(), |value| format!("{value:.1}"))
}

/// A small table: a header row, then rows of a label and values, the
/// values right-aligned in equal columns.
fn table(ui: &mut egui::Ui, palette: &Palette, header: &[String], rows: &[(String, Vec<String>)]) {
    let width = ui.available_width();
    let columns = header.len().saturating_sub(1).max(1) as f32;
    let label_width = (width * 0.42).max(120.0);
    let column = ((width - label_width) / columns).max(56.0);
    let row = |ui: &mut egui::Ui, cells: Vec<String>, strong: bool| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 22.0), Sense::hover());
        let painter = ui.painter();
        let font = if strong {
            theme::medium(theme::TEXT_CAPTION)
        } else {
            theme::regular(theme::TEXT_CAPTION)
        };
        let color = if strong {
            palette.secondary
        } else {
            palette.text
        };
        for (index, cell) in cells.into_iter().enumerate() {
            if index == 0 {
                painter.text(
                    egui::pos2(rect.left(), rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    cell,
                    font.clone(),
                    color,
                );
            } else {
                let right = rect.left() + label_width + column * index as f32;
                painter.text(
                    egui::pos2(right, rect.center().y),
                    egui::Align2::RIGHT_CENTER,
                    cell,
                    font.clone(),
                    color,
                );
            }
        }
    };
    row(ui, header.to_vec(), true);
    let (line, _) = ui.allocate_exact_size(Vec2::new(width, 1.0), Sense::hover());
    ui.painter().rect_filled(line, 0.0, palette.outline);
    for (label, values) in rows {
        let mut cells = vec![label.clone()];
        cells.extend(values.iter().cloned());
        row(ui, cells, false);
    }
}

fn smoothness_table(
    ui: &mut egui::Ui,
    palette: &Palette,
    locale: crate::i18n::Locale,
    report: &Report,
) {
    let header: Vec<String> = [
        "Smoothness",
        "Scrolling",
        "Switching chats",
        "Sticker picker",
    ]
    .into_iter()
    .map(|cell| gettext(locale, cell).into_owned())
    .collect();
    let smooth: Vec<_> = benchmark::TIMED
        .iter()
        .map(|phase| report.smoothness(phase))
        .collect();
    let line = |label: &'static str, cell: fn(&benchmark::Smoothness) -> String| {
        (
            gettext(locale, label).into_owned(),
            smooth.iter().map(cell).collect::<Vec<_>>(),
        )
    };
    let rows = vec![
        line("Frames a second", |s| number(s.fps)),
        line("Frame time, median (ms)", |s| number(s.median_ms)),
        line("Frame time, 99th percentile (ms)", |s| number(s.p99_ms)),
        line("Jitter (ms)", |s| number(s.jitter_ms)),
        line("Missed frames", |s| {
            s.missed.map_or_else(|| "–".to_owned(), |n| n.to_string())
        }),
        line("Slowest frame (ms)", |s| number(s.slowest_ms)),
        line("CPU per frame, 99th percentile (ms)", |s| {
            number(s.cpu_p99_ms)
        }),
    ];
    table(ui, palette, &header, &rows);
}

fn phase_table(ui: &mut egui::Ui, palette: &Palette, locale: crate::i18n::Locale, report: &Report) {
    let header: Vec<String> = ["Phase", "Memory (MB)", "CPU %", "GPU %"]
        .into_iter()
        .map(|cell| gettext(locale, cell).into_owned())
        .collect();
    let rows: Vec<_> = report
        .phases
        .iter()
        .map(|phase| {
            (
                gettext(locale, benchmark::phase_label(&phase.name)).into_owned(),
                vec![
                    number(phase.end.as_ref().map(|end| end.mb)),
                    number(phase.cpu_pct),
                    number(phase.gpu_pct),
                ],
            )
        })
        .collect();
    table(ui, palette, &header, &rows);
}
