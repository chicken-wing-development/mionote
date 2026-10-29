use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaperSize {
    #[default]
    A4,
    A5,
    B4,
    B5,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Orientation {
    #[default]
    Portrait,
    Landscape,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Paper {
    pub size: PaperSize,
    pub orientation: Orientation,
}

impl Paper {
    pub fn millimeters(self) -> (f32, f32) {
        let (w, h) = match self.size {
            PaperSize::A4 => (210.0, 297.0),
            PaperSize::A5 => (148.0, 210.0),
            PaperSize::B4 => (257.0, 364.0),
            PaperSize::B5 => (182.0, 257.0),
        };
        match self.orientation {
            Orientation::Portrait => (w, h),
            Orientation::Landscape => (h, w),
        }
    }
    pub fn dips(self) -> (f32, f32) {
        let (w, h) = self.millimeters();
        (w * 96.0 / 25.4, h * 96.0 / 25.4)
    }
    pub fn points(self) -> (f32, f32) {
        let (w, h) = self.millimeters();
        (w * 72.0 / 25.4, h * 72.0 / 25.4)
    }
    pub fn fit_content(self, right: f32, bottom: f32) -> f32 {
        let (w, h) = self.dips();
        (w / right.max(w)).min(h / bottom.max(h))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rotating_paper_only_shrinks_ink_that_exceeds_the_new_bounds() {
        let portrait = Paper::default();
        let landscape = Paper {
            orientation: Orientation::Landscape,
            ..portrait
        };
        let (w, h) = portrait.dips();
        let first = landscape.fit_content(w, h);
        assert!(first < 1.0);
        assert_eq!(portrait.fit_content(w * first, h * first), 1.0);
        assert_eq!(landscape.fit_content(w * first, h * first), 1.0);
        assert_eq!(portrait.fit_content(0.0, 0.0), 1.0);
        assert_eq!(landscape.fit_content(100.0, 100.0), 1.0);
    }
    #[test]
    fn jis_sizes_and_orientation_preserve_physical_dimensions() {
        for (size, mm) in [
            (PaperSize::A4, (210.0, 297.0)),
            (PaperSize::A5, (148.0, 210.0)),
            (PaperSize::B4, (257.0, 364.0)),
            (PaperSize::B5, (182.0, 257.0)),
        ] {
            for orientation in [Orientation::Portrait, Orientation::Landscape] {
                let paper = Paper { size, orientation };
                let expected = if orientation == Orientation::Portrait {
                    mm
                } else {
                    (mm.1, mm.0)
                };
                assert_eq!(paper.millimeters(), expected);
                let (w, h) = paper.dips();
                assert!((w / h - expected.0 / expected.1).abs() < 0.0001);
            }
        }
    }
}
