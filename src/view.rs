//! Page layout in screen pixels. Scrolling only moves the page HWND; it never
//! translates ink coordinates or changes the document.
use crate::paper::Paper;

pub const MIN_ZOOM: f32 = 0.25;
pub const MAX_ZOOM: f32 = 4.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Mode {
    #[default]
    Width,
    Page,
    Zoom(f32),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct State {
    pub mode: Mode,
    pub scroll: (i32, i32),
}

#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub outer: (i32, i32),
    pub client: (i32, i32),
    pub page: (u32, u32),
    pub origin: (i32, i32),
    pub scroll: (i32, i32),
    pub extent: (i32, i32),
    pub bars: (bool, bool),
    pub scale: f32,
    pub dpi: f32,
    margin: i32,
}

impl Layout {
    pub fn new(
        paper: Paper,
        outer: (i32, i32),
        dpi: f32,
        margin: i32,
        bar_size: (i32, i32),
        state: State,
    ) -> Self {
        let outer = (outer.0.max(1), outer.1.max(1));
        let (w, h) = paper.dips();
        let mut bars = (false, false);
        let mut client = outer;
        let mut scale = 1.0;
        let mut page = (1, 1);
        // Adding a scrollbar can require the other one. Keep that reservation
        // for this layout pass so near-exact fits cannot oscillate on resize.
        for _ in 0..3 {
            client = (
                (outer.0 - if bars.1 { bar_size.0 } else { 0 }).max(1),
                (outer.1 - if bars.0 { bar_size.1 } else { 0 }).max(1),
            );
            let width_scale = (client.0 - margin * 2).max(1) as f32 / w;
            let height_scale = (client.1 - margin * 2).max(1) as f32 / h;
            scale = match state.mode {
                Mode::Width => width_scale,
                Mode::Page => width_scale.min(height_scale),
                Mode::Zoom(zoom) => zoom.clamp(MIN_ZOOM, MAX_ZOOM) * dpi,
            };
            page = (
                (w * scale).floor().max(1.0) as u32,
                (h * scale).floor().max(1.0) as u32,
            );
            let required = (
                page.0 as i32 + margin * 2 > client.0,
                page.1 as i32 + margin * 2 > client.1,
            );
            let next = (bars.0 || required.0, bars.1 || required.1);
            if next == bars {
                break;
            }
            bars = next;
        }
        let extent = (
            (page.0 as i32 + margin * 2).max(client.0),
            (page.1 as i32 + margin * 2).max(client.1),
        );
        let scroll = (
            state.scroll.0.clamp(0, extent.0 - client.0),
            state.scroll.1.clamp(0, extent.1 - client.1),
        );
        Self {
            outer,
            client,
            page,
            origin: (
                ((client.0 - page.0 as i32) / 2).max(margin) - scroll.0,
                ((client.1 - page.1 as i32) / 2).max(margin) - scroll.1,
            ),
            scroll,
            extent,
            bars,
            scale,
            dpi,
            margin,
        }
    }

    pub fn zoom(self) -> f32 {
        self.scale / self.dpi
    }

    pub fn point(self, screen: (i32, i32)) -> (f32, f32) {
        (
            (screen.0 - self.origin.0) as f32 / self.scale,
            (screen.1 - self.origin.1) as f32 / self.scale,
        )
    }

    pub fn anchored_scroll(self, paper_point: (f32, f32), screen: (i32, i32)) -> (i32, i32) {
        (
            (((self.client.0 - self.page.0 as i32) / 2).max(self.margin) as f32
                + paper_point.0 * self.scale
                - screen.0 as f32)
                .round() as i32,
            (((self.client.1 - self.page.1 as i32) / 2).max(self.margin) as f32
                + paper_point.1 * self.scale
                - screen.1 as f32)
                .round() as i32,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paper::{Orientation, PaperSize};

    #[test]
    fn every_paper_fits_width_or_whole_page_at_each_screen_dpi() {
        for size in [PaperSize::A4, PaperSize::A5, PaperSize::B4, PaperSize::B5] {
            for orientation in [Orientation::Portrait, Orientation::Landscape] {
                for dpi in [1.0, 1.5, 2.0] {
                    let paper = Paper { size, orientation };
                    let outer = ((840.0 * dpi) as i32, (580.0 * dpi) as i32);
                    let margin = (12.0 * dpi) as i32;
                    let bars = ((17.0 * dpi) as i32, (17.0 * dpi) as i32);
                    let width = Layout::new(paper, outer, dpi, margin, bars, State::default());
                    assert!(!width.bars.0);
                    assert!((width.page.0 as i32 + margin * 2 - width.client.0).abs() <= 1);
                    let whole = Layout::new(
                        paper,
                        outer,
                        dpi,
                        margin,
                        bars,
                        State {
                            mode: Mode::Page,
                            scroll: (99999, 99999),
                        },
                    );
                    assert_eq!(whole.bars, (false, false));
                    assert_eq!(whole.scroll, (0, 0));
                    assert!(whole.origin.0 >= margin && whole.origin.1 >= margin);
                    assert!(whole.origin.0 + whole.page.0 as i32 <= outer.0 - margin);
                    assert!(whole.origin.1 + whole.page.1 as i32 <= outer.1 - margin);
                    let (w, h) = paper.dips();
                    assert!((width.page.0 as f32 / width.page.1 as f32 - w / h).abs() < 0.005);
                }
            }
        }
    }

    #[test]
    fn zoom_keeps_the_anchor_and_scroll_can_reach_every_page_edge() {
        let paper = Paper::default();
        let make = |state| Layout::new(paper, (840, 580), 1.0, 12, (17, 17), state);
        let initial = make(State {
            scroll: (0, 360),
            ..State::default()
        });
        let anchor = (400, 300);
        let point = initial.point(anchor);
        let mode = Mode::Zoom(initial.zoom() * 1.2);
        let enlarged = make(State {
            mode,
            scroll: (0, 0),
        });
        let enlarged = make(State {
            mode,
            scroll: enlarged.anchored_scroll(point, anchor),
        });
        let after = enlarged.point(anchor);
        assert!((point.0 - after.0).abs() < 1.0);
        assert!((point.1 - after.1).abs() < 1.0);
        assert_eq!(enlarged.bars, (true, true));
        let bottom = make(State {
            mode: Mode::Zoom(4.0),
            scroll: (i32::MAX, i32::MAX),
        });
        assert_eq!(bottom.origin.0 + bottom.page.0 as i32, bottom.client.0 - 12);
        assert_eq!(bottom.origin.1 + bottom.page.1 as i32, bottom.client.1 - 12);
        let top = make(State {
            mode: Mode::Zoom(4.0),
            scroll: (-100, -100),
        });
        assert_eq!(top.origin, (12, 12));
    }

    #[test]
    fn manual_zoom_is_bounded_and_small_windows_have_valid_dimensions() {
        for zoom in [-1.0, 0.25, 1.0, 4.0, 100.0] {
            let layout = Layout::new(
                Paper::default(),
                (12, 8),
                2.0,
                12,
                (34, 34),
                State {
                    mode: Mode::Zoom(zoom),
                    scroll: (0, 0),
                },
            );
            assert!(layout.client.0 > 0 && layout.client.1 > 0);
            assert_eq!(layout.zoom(), zoom.clamp(MIN_ZOOM, MAX_ZOOM));
        }
    }
}
