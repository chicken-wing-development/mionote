//! Shared export geometry, independent of the live InkPresenter and viewport.
use crate::ink_io::PageSnapshot;
use crate::validation;
use windows::{Foundation::Point, UI::Input::Inking::InkStroke};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub trait Canvas {
    fn begin(&mut self, color: u32, width: f32, point: Point) -> Result<()>;
    fn line(&mut self, point: Point) -> Result<()>;
    fn curve(&mut self, a: Point, b: Point, end: Point) -> Result<()>;
    fn end(&mut self) -> Result<()>;
    fn dot(&mut self, color: u32, point: Point, radius: f32) -> Result<()>;
}

pub struct DrawBudget {
    remaining: u32,
}
impl Default for DrawBudget {
    fn default() -> Self {
        Self {
            remaining: validation::MAX_DRAW_COMMANDS,
        }
    }
}

pub fn draw_page(
    canvas: &mut impl Canvas,
    page: &PageSnapshot,
    budget: &mut DrawBudget,
) -> Result<()> {
    let mut guarded = GuardedCanvas {
        inner: canvas,
        budget,
    };
    let canvas = &mut guarded;
    let (w, h) = page.page.paper.dips();
    let mut y = 64.0;
    while y < h {
        canvas.begin(0xffd5e0ea, 0.5, Point { X: 0.0, Y: y })?;
        canvas.line(Point { X: w, Y: y })?;
        canvas.end()?;
        y += page.page.rule_spacing.size();
    }
    canvas.begin(0xffe4b5b5, 0.5, Point { X: 64.0, Y: 0.0 })?;
    canvas.line(Point { X: 64.0, Y: h })?;
    canvas.end()?;
    for stroke in &page.strokes {
        draw_stroke(canvas, stroke)?;
    }
    Ok(())
}

struct GuardedCanvas<'a, C> {
    inner: &'a mut C,
    budget: &'a mut DrawBudget,
}
impl<C> GuardedCanvas<'_, C> {
    fn charge(&mut self) -> Result<()> {
        self.budget.remaining = self
            .budget
            .remaining
            .checked_sub(1)
            .ok_or("描画処理の上限を超えました。")?;
        Ok(())
    }
    fn point(p: Point) -> Result<()> {
        if !validation::point(p) {
            return Err("筆跡の描画座標が不正です。".into());
        }
        Ok(())
    }
}
impl<C: Canvas> Canvas for GuardedCanvas<'_, C> {
    fn begin(&mut self, color: u32, width: f32, p: Point) -> Result<()> {
        self.charge()?;
        Self::point(p)?;
        if !validation::width(width) {
            return Err("筆跡の線幅が不正です。".into());
        }
        self.inner.begin(color, width, p)
    }
    fn line(&mut self, p: Point) -> Result<()> {
        self.charge()?;
        Self::point(p)?;
        self.inner.line(p)
    }
    fn curve(&mut self, a: Point, b: Point, p: Point) -> Result<()> {
        self.charge()?;
        Self::point(a)?;
        Self::point(b)?;
        Self::point(p)?;
        self.inner.curve(a, b, p)
    }
    fn end(&mut self) -> Result<()> {
        self.charge()?;
        self.inner.end()
    }
    fn dot(&mut self, color: u32, p: Point, radius: f32) -> Result<()> {
        self.charge()?;
        Self::point(p)?;
        if !radius.is_finite() || !(0.0..=validation::MAX_PEN_SIZE).contains(&radius) {
            return Err("筆跡の半径が不正です。".into());
        }
        self.inner.dot(color, p, radius)
    }
}

fn draw_stroke(canvas: &mut impl Canvas, stroke: &InkStroke) -> Result<()> {
    let attributes = stroke.DrawingAttributes()?;
    let c = attributes.Color()?;
    let color = u32::from_be_bytes([c.A, c.R, c.G, c.B]);
    let segments = stroke.GetRenderingSegments()?;
    let count = segments.Size()?;
    if count == 0 {
        return Ok(());
    }
    // Windows already applies PointTransform to rendering segments.
    let width = attributes.Size()?.Width.max(0.01);
    let first = segments.GetAt(0)?;
    let mut from = first.Position()?;
    let fixed = attributes.IgnorePressure()?;
    if count == 1 {
        let pressure = if fixed {
            1.0
        } else {
            first.Pressure()?.clamp(0.0, 1.0)
        };
        return canvas.dot(color, from, width * pressure * 0.5);
    }
    let curved = attributes.FitToCurve()?;
    if fixed {
        canvas.begin(color, width, from)?;
        for i in 1..count {
            let segment = segments.GetAt(i)?;
            let to = segment.Position()?;
            if curved {
                canvas.curve(
                    segment.BezierControlPoint1()?,
                    segment.BezierControlPoint2()?,
                    to,
                )?;
            } else {
                canvas.line(to)?;
            }
        }
        canvas.end()?;
    } else {
        let mut pressure = first.Pressure()?;
        for i in 1..count {
            let segment = segments.GetAt(i)?;
            let to = segment.Position()?;
            let a = if curved {
                segment.BezierControlPoint1()?
            } else {
                from
            };
            let b = if curved {
                segment.BezierControlPoint2()?
            } else {
                to
            };
            let end_pressure = segment.Pressure()?;
            let distance = |a: Point, b: Point| (a.X - b.X).hypot(a.Y - b.Y);
            let steps = ((distance(from, a) + distance(a, b) + distance(b, to)) / 1.5)
                .ceil()
                .clamp(1.0, 128.0) as u32;
            let mut previous = from;
            for step in 1..=steps {
                let t = step as f32 / steps as f32;
                let u = 1.0 - t;
                let point = Point {
                    X: u * u * u * from.X
                        + 3.0 * u * u * t * a.X
                        + 3.0 * u * t * t * b.X
                        + t * t * t * to.X,
                    Y: u * u * u * from.Y
                        + 3.0 * u * u * t * a.Y
                        + 3.0 * u * t * t * b.Y
                        + t * t * t * to.Y,
                };
                let pen_width =
                    (width * (pressure + (end_pressure - pressure) * t).clamp(0.0, 1.0)).max(0.01);
                canvas.begin(color, pen_width, previous)?;
                canvas.line(point)?;
                canvas.end()?;
                previous = point;
            }
            from = to;
            pressure = end_pressure;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct Counter(u32);
    impl Canvas for Counter {
        fn begin(&mut self, _: u32, _: f32, _: Point) -> Result<()> {
            self.0 += 1;
            Ok(())
        }
        fn line(&mut self, _: Point) -> Result<()> {
            self.0 += 1;
            Ok(())
        }
        fn curve(&mut self, _: Point, _: Point, _: Point) -> Result<()> {
            self.0 += 1;
            Ok(())
        }
        fn end(&mut self) -> Result<()> {
            self.0 += 1;
            Ok(())
        }
        fn dot(&mut self, _: u32, _: Point, _: f32) -> Result<()> {
            self.0 += 1;
            Ok(())
        }
    }
    #[test]
    fn drawing_guard_blocks_invalid_values_and_excess_work_before_native_calls() {
        let mut counter = Counter::default();
        {
            let mut budget = DrawBudget { remaining: 4 };
            let mut canvas = GuardedCanvas {
                inner: &mut counter,
                budget: &mut budget,
            };
            assert!(canvas.begin(0, f32::NAN, Point::default()).is_err());
            assert!(
                canvas
                    .line(Point {
                        X: f32::INFINITY,
                        Y: 0.0
                    })
                    .is_err()
            );
            canvas.line(Point::default()).unwrap();
            canvas.end().unwrap();
            assert!(canvas.end().is_err());
        }
        assert_eq!(counter.0, 2);
        let mut budget = DrawBudget { remaining: 1 };
        draw_page(
            &mut counter,
            &PageSnapshot {
                page: Default::default(),
                strokes: vec![],
                scale: 1.0,
            },
            &mut budget,
        )
        .unwrap_err();
        assert_eq!(budget.remaining, 0);
        assert!(
            draw_page(
                &mut counter,
                &PageSnapshot {
                    page: Default::default(),
                    strokes: vec![],
                    scale: 1.0
                },
                &mut budget
            )
            .is_err()
        );
        assert_eq!(counter.0, 3); // Starting another page does not reset the export budget.
    }
}
