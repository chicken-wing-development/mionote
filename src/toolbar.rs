//! Small vector icons and flat buttons, drawn only when the toolbar repaints.

use windows::Win32::{
    Foundation::{COLORREF, POINT, RECT},
    Graphics::Gdi::*,
    UI::Controls::{DRAWITEMSTRUCT, ODS_DISABLED, ODS_FOCUS, ODS_SELECTED},
};

pub enum Content {
    Text(&'static str),
    Percent(u32),
    Pressure,
    Eraser,
    Spacing(f32),
    Zoom(bool),
}

pub unsafe fn draw(item: &DRAWITEMSTRUCT, scale: f32, font: HFONT, content: Content, active: bool) {
    unsafe {
        let dc = item.hDC;
        let saved = SaveDC(dc);
        let disabled = item.itemState.0 & ODS_DISABLED.0 != 0;
        let pressed = item.itemState.0 & ODS_SELECTED.0 != 0;
        let px = |value: f32| (value * scale).round() as i32;
        let color = if disabled {
            rgb(159, 169, 179)
        } else if active {
            rgb(28, 83, 143)
        } else {
            rgb(64, 80, 96)
        };
        let fill = if pressed {
            rgb(201, 221, 242)
        } else if active {
            rgb(225, 238, 252)
        } else {
            rgb(255, 255, 255)
        };
        let background = CreateSolidBrush(rgb(245, 246, 247));
        FillRect(dc, &item.rcItem, background);
        let brush = CreateSolidBrush(fill);
        let border = CreatePen(
            PS_SOLID,
            px(1.0).max(1),
            if active {
                rgb(111, 157, 207)
            } else {
                rgb(218, 225, 232)
            },
        );
        SelectObject(dc, brush.into());
        SelectObject(dc, border.into());
        let rect = RECT {
            left: item.rcItem.left + 1,
            top: item.rcItem.top + 1,
            right: item.rcItem.right - 1,
            bottom: item.rcItem.bottom - 1,
        };
        let _ = RoundRect(
            dc,
            rect.left,
            rect.top,
            rect.right,
            rect.bottom,
            px(8.0),
            px(8.0),
        );

        let stroke = CreatePen(PS_SOLID, px(1.6).max(1), color);
        SelectObject(dc, stroke.into());
        let cx = (rect.left + rect.right) / 2;
        let cy = (rect.top + rect.bottom) / 2;
        let point = |x: f32, y: f32| POINT {
            x: cx + px(x),
            y: cy + px(y),
        };
        let line = |a: POINT, b: POINT| {
            let _ = MoveToEx(dc, a.x, a.y, None);
            let _ = LineTo(dc, b.x, b.y);
        };
        match content {
            Content::Text(text) => {
                draw_text(dc, font, color, rect, text);
            }
            Content::Percent(value) => draw_text(dc, font, color, rect, &format!("{value}%")),
            Content::Pressure => {
                let points = [
                    point(-5.0, -9.0),
                    point(5.0, -9.0),
                    point(8.0, -1.0),
                    point(0.0, 9.0),
                    point(-8.0, -1.0),
                ];
                let _ = Polygon(dc, &points);
                line(point(0.0, 0.0), point(0.0, 8.0));
                let a = point(-1.5, -4.0);
                let b = point(1.5, -1.0);
                let _ = Ellipse(dc, a.x, a.y, b.x, b.y);
            }
            Content::Eraser => {
                let points = [
                    point(-1.0, -9.0),
                    point(10.0, 2.0),
                    point(2.0, 10.0),
                    point(-9.0, -1.0),
                ];
                let _ = Polygon(dc, &points);
                line(point(-4.0, 4.0), point(5.0, -5.0));
            }
            Content::Spacing(gap) => {
                for y in [-gap, 0.0, gap] {
                    line(point(-9.0, y), point(9.0, y));
                }
            }
            Content::Zoom(plus) => {
                line(point(-7.0, 0.0), point(7.0, 0.0));
                if plus {
                    line(point(0.0, -7.0), point(0.0, 7.0));
                }
            }
        }
        if item.itemState.0 & ODS_FOCUS.0 != 0 {
            let focus = RECT {
                left: rect.left + px(4.0),
                top: rect.top + px(4.0),
                right: rect.right - px(4.0),
                bottom: rect.bottom - px(4.0),
            };
            let _ = DrawFocusRect(dc, &focus);
        }
        let _ = RestoreDC(dc, saved);
        for object in [
            background.into(),
            brush.into(),
            border.into(),
            stroke.into(),
        ] {
            let _ = DeleteObject(object);
        }
    }
}

unsafe fn draw_text(dc: HDC, font: HFONT, color: COLORREF, mut bounds: RECT, text: &str) {
    unsafe {
        SelectObject(dc, font.into());
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, color);
        let mut text: Vec<u16> = text.encode_utf16().collect();
        DrawTextW(
            dc,
            &mut text,
            &mut bounds,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
    }
}

fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(r as u32 | ((g as u32) << 8) | ((b as u32) << 16))
}
