//! A small PDF vector writer. Export runs on the file worker; no PDF runtime or
//! font engine is loaded during startup or handwriting.
use crate::drawing::{self, Canvas};
use crate::{
    ink_io::PageSnapshot,
    note,
    storage::{self, LimitedWriter, MAX_EXPORT_BYTES},
};
use std::{
    io::{self, BufWriter, Seek, Write},
    path::Path,
};
use windows::Foundation::Point;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn export(path: &Path, pages: &[PageSnapshot]) -> Result<()> {
    storage::validate_extension(path, "pdf")?;
    if pages.is_empty() || pages.len() > note::MAX_PAGES {
        return Err("ページ数が不正です。".into());
    }
    storage::atomic_write(path, |file| {
        let mut writer = LimitedWriter::new(BufWriter::new(file), MAX_EXPORT_BYTES);
        write_pdf(&mut writer, pages).map_err(|e| io::Error::other(e.to_string()))?;
        writer.flush()
    })?;
    Ok(())
}

fn object(file: &mut (impl Write + Seek), offsets: &mut Vec<u64>, id: usize) -> io::Result<()> {
    if id != offsets.len() {
        return Err(io::Error::other("PDFオブジェクト番号が不正です。"));
    }
    offsets.push(file.stream_position()?);
    writeln!(file, "{id} 0 obj")
}

fn write_pdf(file: &mut (impl Write + Seek), pages: &[PageSnapshot]) -> Result<()> {
    let mut budget = drawing::DrawBudget::default();
    file.write_all(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n")?;
    let mut offsets = vec![0];
    object(file, &mut offsets, 1)?;
    writeln!(file, "<< /Type /Catalog /Pages 2 0 R >>\nendobj")?;
    object(file, &mut offsets, 2)?;
    write!(file, "<< /Type /Pages /Count {} /Kids [", pages.len())?;
    for i in 0..pages.len() {
        write!(file, "{} 0 R ", 3 + i * 3)?;
    }
    writeln!(file, "] >>\nendobj")?;
    for (i, page) in pages.iter().enumerate() {
        let id = 3 + i * 3;
        let (w, h) = page.page.paper.points();
        object(file, &mut offsets, id)?;
        writeln!(
            file,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w:.5} {h:.5}] /Resources << >> /Contents {} 0 R >>\nendobj",
            id + 1
        )?;
        object(file, &mut offsets, id + 1)?;
        writeln!(file, "<< /Length {} 0 R >>\nstream", id + 2)?;
        let start = file.stream_position()?;
        render_page(file, page, &mut budget)?;
        let length = file.stream_position()? - start;
        writeln!(file, "endstream\nendobj")?;
        object(file, &mut offsets, id + 2)?;
        writeln!(file, "{length}\nendobj")?;
    }
    let xref = file.stream_position()?;
    writeln!(file, "xref\n0 {}\n0000000000 65535 f ", offsets.len())?;
    for offset in offsets.iter().skip(1) {
        writeln!(file, "{offset:010} 00000 n ")?;
    }
    writeln!(
        file,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF",
        offsets.len()
    )?;
    Ok(())
}

fn render_page(
    file: &mut impl Write,
    page: &PageSnapshot,
    budget: &mut drawing::DrawBudget,
) -> Result<()> {
    let (w, h) = page.page.paper.dips();
    let (_, height_pt) = page.page.paper.points();
    writeln!(
        file,
        "q\n0.75 0 0 -0.75 0 {height_pt:.5} cm\n0 0 {w:.5} {h:.5} re W n\n1 J 1 j"
    )?;
    drawing::draw_page(&mut PdfCanvas(file), page, budget)?;
    writeln!(file, "Q")?;
    Ok(())
}

struct PdfCanvas<'a, W: Write>(&'a mut W);
fn rgb(color: u32) -> (f32, f32, f32) {
    let [a, r, g, b] = color.to_be_bytes();
    let alpha = a as f32 / 255.0;
    let component = |v: u8| 1.0 - alpha + alpha * v as f32 / 255.0;
    (component(r), component(g), component(b))
}
impl<W: Write> Canvas for PdfCanvas<'_, W> {
    fn begin(&mut self, color: u32, width: f32, p: Point) -> Result<()> {
        let (r, g, b) = rgb(color);
        writeln!(
            self.0,
            "{r:.5} {g:.5} {b:.5} RG\n{width:.4} w\n{:.4} {:.4} m",
            p.X, p.Y
        )?;
        Ok(())
    }
    fn line(&mut self, p: Point) -> Result<()> {
        writeln!(self.0, "{:.4} {:.4} l", p.X, p.Y)?;
        Ok(())
    }
    fn curve(&mut self, a: Point, b: Point, p: Point) -> Result<()> {
        writeln!(
            self.0,
            "{:.4} {:.4} {:.4} {:.4} {:.4} {:.4} c",
            a.X, a.Y, b.X, b.Y, p.X, p.Y
        )?;
        Ok(())
    }
    fn end(&mut self) -> Result<()> {
        writeln!(self.0, "S")?;
        Ok(())
    }
    fn dot(&mut self, color: u32, p: Point, radius: f32) -> Result<()> {
        let (r, g, b) = rgb(color);
        writeln!(self.0, "{r:.5} {g:.5} {b:.5} rg")?;
        circle(self.0, p, radius)?;
        Ok(())
    }
}

fn circle(file: &mut impl Write, p: Point, r: f32) -> io::Result<()> {
    let k = r * 0.5522848;
    let x = p.X;
    let y = p.Y;
    writeln!(
        file,
        "{:.4} {y:.4} m {:.4} {:.4} {:.4} {:.4} {x:.4} {:.4} c {:.4} {:.4} {:.4} {:.4} {:.4} {y:.4} c {:.4} {:.4} {:.4} {:.4} {x:.4} {:.4} c {:.4} {:.4} {:.4} {:.4} {:.4} {y:.4} c f",
        x + r,
        x + r,
        y + k,
        x + k,
        y + r,
        y + r,
        x - k,
        y + r,
        x - r,
        y + k,
        x - r,
        x - r,
        y - k,
        x - k,
        y - r,
        y - r,
        x + k,
        y - r,
        x + r,
        y - k,
        x + r
    )
}
