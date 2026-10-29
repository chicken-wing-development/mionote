use crate::{
    drawing::{self, Canvas, Result},
    ink_io::PageSnapshot,
};
use std::io::Write;
use windows::Foundation::Point;

pub fn write_page(
    writer: &mut impl Write,
    page: &PageSnapshot,
    budget: &mut drawing::DrawBudget,
) -> Result<()> {
    let (mm_w, mm_h) = page.page.paper.millimeters();
    let (w, h) = page.page.paper.dips();
    writeln!(
        writer,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{mm_w}mm\" height=\"{mm_h}mm\" viewBox=\"0 0 {w:.5} {h:.5}\">\n<title>MioNote</title>\n<defs><clipPath id=\"page\"><rect width=\"{w:.5}\" height=\"{h:.5}\"/></clipPath></defs>\n<rect width=\"100%\" height=\"100%\" fill=\"white\"/>\n<g clip-path=\"url(#page)\" stroke-linecap=\"round\" stroke-linejoin=\"round\">"
    )?;
    drawing::draw_page(&mut SvgCanvas(writer), page, budget)?;
    writeln!(writer, "</g>\n</svg>")?;
    Ok(())
}

struct SvgCanvas<'a, W: Write>(&'a mut W);
impl<W: Write> Canvas for SvgCanvas<'_, W> {
    fn begin(&mut self, color: u32, width: f32, p: Point) -> Result<()> {
        writeln!(
            self.0,
            "<path fill=\"none\" stroke=\"#{:06x}\" stroke-opacity=\"{:.5}\" stroke-width=\"{width:.4}\" d=\"M {:.4} {:.4}",
            color & 0xffffff,
            (color >> 24) as f32 / 255.0,
            p.X,
            p.Y
        )?;
        Ok(())
    }
    fn line(&mut self, p: Point) -> Result<()> {
        writeln!(self.0, "L {:.4} {:.4}", p.X, p.Y)?;
        Ok(())
    }
    fn curve(&mut self, a: Point, b: Point, p: Point) -> Result<()> {
        writeln!(
            self.0,
            "C {:.4} {:.4} {:.4} {:.4} {:.4} {:.4}",
            a.X, a.Y, b.X, b.Y, p.X, p.Y
        )?;
        Ok(())
    }
    fn end(&mut self) -> Result<()> {
        writeln!(self.0, "\"/>")?;
        Ok(())
    }
    fn dot(&mut self, color: u32, p: Point, r: f32) -> Result<()> {
        writeln!(
            self.0,
            "<circle cx=\"{:.4}\" cy=\"{:.4}\" r=\"{r:.4}\" fill=\"#{:06x}\" fill-opacity=\"{:.5}\"/>",
            p.X,
            p.Y,
            color & 0xffffff,
            (color >> 24) as f32 / 255.0
        )?;
        Ok(())
    }
}
