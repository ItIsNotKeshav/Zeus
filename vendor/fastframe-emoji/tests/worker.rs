//! New pictures are drawn off the interface thread: the frame that first
//! shows an emoji uploads nothing, and the worker's repaint brings it.

use std::time::{Duration, Instant};

const NOTO_SUBSET: &[u8] = include_bytes!("fixtures/NotoColorEmoji-subset.ttf");

fn uploads(output: &mut egui::FullOutput) -> usize {
    let count = output.textures_delta.set.values().flatten().count();
    output.textures_delta.clear();
    count
}

#[test]
fn pictures_arrive_from_the_worker_without_blocking_a_frame() {
    // Both tests install the same setup; whichever runs first wins.
    fastframe_emoji::EmojiSetup::default()
        .system(false)
        .bundled(NOTO_SUBSET)
        .install();
    fastframe_emoji::warm_up();
    let ctx = egui::Context::default();
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(20.0, 20.0));
    let frame = |ctx: &egui::Context| {
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            for cluster in ["😀", "👍🏽", "🇩🇪"] {
                fastframe_emoji::paint_cluster(ui, cluster, rect);
            }
        });
        uploads(&mut output)
    };
    // The default fonts' own textures go up with the first frame.
    uploads(&mut ctx.run_ui(egui::RawInput::default(), |_| {}));
    assert_eq!(frame(&ctx), 0, "the first frame only queues the pictures");

    let mut uploaded = 0;
    let deadline = Instant::now() + Duration::from_secs(10);
    while uploaded < 3 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
        uploaded += frame(&ctx);
    }
    assert_eq!(uploaded, 3, "every picture arrived, once");
    assert_eq!(frame(&ctx), 0, "and is kept");
}

#[test]
fn prewarming_queues_pictures_before_any_frame_shows_them() {
    fastframe_emoji::EmojiSetup::default()
        .system(false)
        .bundled(NOTO_SUBSET)
        .install();
    let ctx = egui::Context::default();
    uploads(&mut ctx.run_ui(egui::RawInput::default(), |_| {}));
    fastframe_emoji::prewarm(&ctx, ["#️⃣", "❤️"]);
    let deadline = Instant::now() + Duration::from_secs(10);
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(20.0, 20.0));
    let mut uploaded = 0;
    while uploaded < 2 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            fastframe_emoji::paint_cluster(ui, "#️⃣", rect);
            fastframe_emoji::paint_cluster(ui, "❤️", rect);
        });
        uploaded += uploads(&mut output);
    }
    assert_eq!(uploaded, 2);
}
