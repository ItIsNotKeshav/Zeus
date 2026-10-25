//! With pictures drawn off the interface thread, the plugin shows the
//! monochrome glyph until the worker's picture arrives, then the picture.

use std::time::{Duration, Instant};

const NOTO_SUBSET: &[u8] = include_bytes!("fixtures/NotoColorEmoji-subset.ttf");

fn picture_count(output: &egui::FullOutput) -> usize {
    fn walk(shape: &egui::Shape) -> usize {
        match shape {
            egui::Shape::Vec(inner) => inner.iter().map(walk).sum(),
            egui::Shape::Mesh(mesh) if mesh.texture_id != egui::TextureId::default() => 1,
            _ => 0,
        }
    }
    output
        .shapes
        .iter()
        .map(|clipped| walk(&clipped.shape))
        .sum()
}

#[test]
fn a_label_shows_its_glyph_until_the_picture_arrives() {
    fastframe_emoji::EmojiSetup::default()
        .system(false)
        .bundled(NOTO_SUBSET)
        .install();
    fastframe_emoji::warm_up();
    let ctx = egui::Context::default();
    ctx.add_plugin(fastframe_emoji::EmojiPlugin::default());
    let frame = |ctx: &egui::Context| {
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.label("soon 😀");
        });
        let count = picture_count(&output);
        output.textures_delta.clear();
        count
    };
    assert_eq!(frame(&ctx), 0, "the first frame waits for nothing");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if frame(&ctx) == 1 {
            break;
        }
        assert!(Instant::now() < deadline, "the picture never arrived");
        std::thread::sleep(Duration::from_millis(5));
    }
}
