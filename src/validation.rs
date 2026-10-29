//! Validate untrusted native ink on the file worker, never on the pen hot path.
use crate::ink_io::PageSnapshot;
use windows::{
    Foundation::Point,
    UI::Input::Inking::{InkStroke, InkStrokeContainer},
    Win32::Foundation::E_FAIL,
    core::{Error, Result},
};

pub const MAX_STROKES: u32 = 100_000;
pub const MAX_POINTS: u32 = 2_000_000;
pub const MAX_POINTS_PER_STROKE: u32 = 100_000;
pub const MAX_COORDINATE: f32 = 16_384.0;
pub const MAX_PEN_SIZE: f32 = 256.0;
pub const MAX_DRAW_COMMANDS: u32 = 2_000_000;

pub fn invalid() -> Error {
    Error::new(
        E_FAIL,
        "筆跡の数値・座標・データ量が許容範囲を超えています。",
    )
}
pub fn scale(value: f32) -> Result<()> {
    if !value.is_finite() || value <= 0.0 || !(0.000001..=1_000_000.0).contains(&value) {
        return Err(invalid());
    }
    Ok(())
}
pub fn point(point: Point) -> bool {
    [point.X, point.Y]
        .iter()
        .all(|v| v.is_finite() && v.abs() <= MAX_COORDINATE)
}
pub fn width(value: f32) -> bool {
    value.is_finite() && value > 0.0 && value <= MAX_PEN_SIZE
}

#[derive(Default)]
pub struct InkBudget {
    strokes: u32,
    points: u32,
    segments: u32,
}
impl InkBudget {
    fn charge(current: &mut u32, count: u32, maximum: u32) -> Result<()> {
        *current = current
            .checked_add(count)
            .filter(|n| *n <= maximum)
            .ok_or_else(invalid)?;
        Ok(())
    }
    pub fn container(&mut self, container: &InkStrokeContainer, factor: f32) -> Result<()> {
        scale(factor)?;
        let strokes = container.GetStrokes()?;
        Self::charge(&mut self.strokes, strokes.Size()?, MAX_STROKES)?;
        for index in 0..strokes.Size()? {
            self.stroke(&strokes.GetAt(index)?, factor)?;
        }
        Ok(())
    }
    fn stroke(&mut self, stroke: &InkStroke, factor: f32) -> Result<()> {
        let transform = stroke.PointTransform()?;
        let values = [
            transform.M11,
            transform.M12,
            transform.M21,
            transform.M22,
            transform.M31,
            transform.M32,
        ];
        if !values
            .iter()
            .all(|v| v.is_finite() && v.abs() <= 1_000_000.0)
        {
            return Err(invalid());
        }
        let attributes = stroke.DrawingAttributes()?;
        let size = attributes.Size()?;
        if !width(size.Width / factor) || !width(size.Height / factor) {
            return Err(invalid());
        }
        let tip = attributes.PenTipTransform()?;
        if ![tip.M11, tip.M12, tip.M21, tip.M22, tip.M31, tip.M32]
            .iter()
            .all(|v| v.is_finite() && v.abs() <= 16.0)
        {
            return Err(invalid());
        }
        let points = stroke.GetInkPoints()?;
        let count = points.Size()?;
        if count > MAX_POINTS_PER_STROKE {
            return Err(invalid());
        }
        Self::charge(&mut self.points, count, MAX_POINTS)?;
        for i in 0..count {
            let input = points.GetAt(i)?;
            let p = input.Position()?;
            let normalized = Point {
                X: (p.X * transform.M11 + p.Y * transform.M21 + transform.M31) / factor,
                Y: (p.X * transform.M12 + p.Y * transform.M22 + transform.M32) / factor,
            };
            let pressure = input.Pressure()?;
            let tilt = [input.TiltX()?, input.TiltY()?];
            if !point(normalized)
                || !pressure.is_finite()
                || !(0.0..=1.0).contains(&pressure)
                || !tilt
                    .iter()
                    .all(|v| v.is_finite() && (-90.0..=90.0).contains(v))
            {
                return Err(invalid());
            }
        }
        // Check native curve output too, before any GDI+/PDF/SVG consumer sees it.
        let segments = stroke.GetRenderingSegments()?;
        Self::charge(&mut self.segments, segments.Size()?, MAX_POINTS)?;
        for i in 0..segments.Size()? {
            let segment = segments.GetAt(i)?;
            for p in [
                segment.Position()?,
                segment.BezierControlPoint1()?,
                segment.BezierControlPoint2()?,
            ] {
                if !point(Point {
                    X: p.X / factor,
                    Y: p.Y / factor,
                }) {
                    return Err(invalid());
                }
            }
            let pressure = segment.Pressure()?;
            if !pressure.is_finite() || !(0.0..=1.0).contains(&pressure) {
                return Err(invalid());
            }
        }
        Ok(())
    }
}

pub fn snapshots(pages: &[PageSnapshot]) -> Result<()> {
    if pages.is_empty() || pages.len() > crate::note::MAX_PAGES {
        return Err(invalid());
    }
    let mut budget = InkBudget::default();
    for page in pages {
        scale(page.scale)?;
        InkBudget::charge(
            &mut budget.strokes,
            u32::try_from(page.strokes.len()).map_err(|_| invalid())?,
            MAX_STROKES,
        )?;
        for stroke in &page.strokes {
            budget.stroke(stroke, page.scale)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_nonfinite_geometry_and_work_counter_overflow() {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.0, -1.0] {
            assert!(scale(value).is_err());
            assert!(!width(value));
        }
        assert!(!point(Point {
            X: f32::NAN,
            Y: 0.0
        }));
        assert!(!point(Point {
            X: MAX_COORDINATE + 1.0,
            Y: 0.0
        }));
        let mut count = u32::MAX;
        assert!(InkBudget::charge(&mut count, 1, u32::MAX).is_err());
        let mut count = MAX_POINTS;
        assert!(InkBudget::charge(&mut count, 1, MAX_POINTS).is_err());
    }
}
