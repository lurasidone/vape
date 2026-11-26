// BedBreaker's screen-space indicator: one box at each target bed's
// projected centre, the chosen target highlighted, plus the Hypixel cover's
// tool icon.
use super::overlay::{Draw, Frame, Item};
use super::super::super::core::bed_breaker_runtime::BedBreaker;

pub fn draws(bed_breaker: &BedBreaker, frame: &Frame) -> Vec<Draw> {
    let selected = bed_breaker.selected();
    let mut out = Vec::new();
    for (index, target) in bed_breaker.targets().iter().enumerate() {
        let position = [
            target.position.x + 0.5,
            target.position.y + 0.5,
            target.position.z + 0.5,
        ];
        let (Some(at), Some(per_block)) = (
            frame.projection.project(position),
            frame.projection.units_per_block(position),
        ) else {
            continue;
        };
        let visibility = target.visibility.clamp(0.0, 1.0);
        let size = (per_block * (0.25 + 0.55 * visibility)).clamp(3.0, 64.0);
        let chosen = selected == Some(index);
        let color = if chosen { 0xffff_ff20 } else { 0xff40_40ff };
        let (x, y) = (at[0] - size / 2.0, at[1] - size / 2.0);
        out.push(Draw::fill(x - 1.0, y - 1.0, size + 2.0, size + 2.0, 0xc000_0000));
        out.push(Draw::fill(x, y, size, size, 0x3000_0000));
        for (from, to) in [
            ([x, y], [x + size, y]),
            ([x + size, y], [x + size, y + size]),
            ([x + size, y + size], [x, y + size]),
            ([x, y + size], [x, y]),
        ] {
            out.push(Draw::Line {
                from,
                to,
                width: 1.0,
                color,
            });
        }
        if let Some(cover) = &target.cover
            && let Some(tool) = &cover.tool
        {
            out.push(Draw::Item {
                x: x + size * 0.25,
                y: y + size * 0.25,
                scale: (size * 0.5 / 18.0).max(0.1),
                item: Item {
                    id: tool.clone(),
                    count: 1,
                    ..Item::default()
                },
            });
        }
    }
    out
}