// Proportional-size text adapter for the current SDK. Ui's Geist helpers are
// crate-private, so render the same public atlas glyph cells into HudBatch.
pub(crate) fn measure(ui: &horizon_client::ui_theme::Ui<'_>, text: &str, size: f32) -> f32 {
    horizon_client::hud::HudBatch::text_width(ui.atlas, text) * (size / 8.0)
}

pub(crate) fn text(
    ui: &mut horizon_client::ui_theme::Ui<'_>,
    text: &str,
    x: f32,
    y: f32,
    size: f32,
    line: f32,
    color: u32,
) {
    let density = ui.density;
    let scale = ui.scale;
    let glyph_scale = size * density / 8.0;
    let line_px = line * density;
    let ascent = 7.0 * glyph_scale;
    let baseline = ((line_px - 9.0 * glyph_scale) / 2.0 + ascent).round();
    let mut pen = 0.0_f32;
    let x_px = (x * density).round();
    let y_px = (y * density).round();
    for ch in text.chars() {
        let glyph = ui.atlas.glyph_cell(ch);
        if let Some(rect) = glyph.rect {
            let left = (pen + glyph.left * glyph_scale).round();
            let top = baseline + ((glyph.top - 7.0) * glyph_scale).round();
            let width = (glyph.right - glyph.left) * glyph_scale;
            let height = (glyph.bottom - glyph.top) * glyph_scale;
            ui.batch.rect_uv(
                (x_px + left) / scale,
                (y_px + top) / scale,
                width / scale,
                height / scale,
                rect,
                color,
            );
        }
        pen += glyph.advance * glyph_scale;
    }
}
