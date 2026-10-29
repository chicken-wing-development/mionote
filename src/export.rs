use crate::{
    drawing::Result,
    ink_io::PageSnapshot,
    note,
    storage::{self, LimitedWriter, MAX_EXPORT_BYTES},
};
use std::{
    io::{BufWriter, Write},
    path::Path,
};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Pdf,
    Png,
    Svg,
    PngZip,
    SvgZip,
}

impl Kind {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Png => "png",
            Self::Svg => "svg",
            Self::PngZip | Self::SvgZip => "zip",
        }
    }
    pub fn current_only(self) -> bool {
        matches!(self, Self::Png | Self::Svg)
    }
    pub fn completed(self) -> &'static str {
        match self {
            Self::Pdf => "全ページをPDFに出力しました。",
            Self::Png => "現在のページをPNGに出力しました。",
            Self::Svg => "現在のページをSVGに出力しました。",
            Self::PngZip => "全ページのPNGをZIPに出力しました。",
            Self::SvgZip => "全ページのSVGをZIPに出力しました。",
        }
    }
}

pub fn export(path: &Path, pages: &[PageSnapshot], kind: Kind) -> Result<()> {
    storage::validate_extension(path, kind.extension())?;
    if pages.is_empty()
        || pages.len() > note::MAX_PAGES
        || (kind.current_only() && pages.len() != 1)
    {
        return Err("出力ページ数が不正です。".into());
    }
    if kind == Kind::Pdf {
        return crate::pdf::export(path, pages);
    }
    let png = if matches!(kind, Kind::Png | Kind::PngZip) {
        Some(crate::png::Encoder::new()?)
    } else {
        None
    };
    storage::atomic_write(path, |file| {
        let mut budget = crate::drawing::DrawBudget::default();
        let mut writer = LimitedWriter::new(BufWriter::new(file), MAX_EXPORT_BYTES);
        let result: Result<()> = (|| {
            match kind {
                Kind::Png => writer.write_all(
                    &png.as_ref()
                        .ok_or("PNGエンコーダーがありません。")?
                        .encode(&pages[0], &mut budget)?,
                )?,
                Kind::Svg => crate::svg::write_page(&mut writer, &pages[0], &mut budget)?,
                Kind::PngZip | Kind::SvgZip => {
                    let mut archive = ZipWriter::new(&mut writer);
                    let options =
                        SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
                    for (index, page) in pages.iter().enumerate() {
                        let extension = if kind == Kind::PngZip { "png" } else { "svg" };
                        archive
                            .start_file(format!("page-{:04}.{extension}", index + 1), options)?;
                        if let Some(png) = &png {
                            archive.write_all(&png.encode(page, &mut budget)?)?;
                        } else {
                            crate::svg::write_page(&mut archive, page, &mut budget)?;
                        }
                    }
                    archive.finish()?;
                }
                Kind::Pdf => unreachable!(),
            }
            writer.flush()?;
            Ok(())
        })();
        result.map_err(|e| std::io::Error::other(e.to_string()))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_single_page_request_keeps_existing_export() {
        let root =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/image-export-tests");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("previous.svg");
        std::fs::write(&path, b"previous export").unwrap();
        let pages: Vec<_> = (0..2)
            .map(|_| PageSnapshot {
                page: Default::default(),
                strokes: vec![],
                scale: 1.0,
            })
            .collect();
        assert!(export(&path, &pages, Kind::Svg).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"previous export");
        std::fs::remove_file(path).unwrap();
    }
}
