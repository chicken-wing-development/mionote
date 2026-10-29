//! File/stream work runs on a worker apartment, away from the live presenter.
//! Only agile InkStroke clones cross threads; the live stroke container does not.

use crate::export::Kind;
use crate::note::{self, Note, NotePage, Page};
use std::path::PathBuf;
use windows::{
    Storage::Streams::{DataReader, DataWriter, InMemoryRandomAccessStream},
    UI::Input::Inking::{InkPersistenceFormat, InkStroke, InkStrokeContainer},
    Win32::{
        Foundation::E_FAIL,
        System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize},
    },
    core::{Error, Result},
};

pub struct PageSnapshot {
    pub page: Page,
    pub strokes: Vec<InkStroke>,
    pub scale: f32,
}
pub enum Task {
    Save {
        path: PathBuf,
        pages: Vec<PageSnapshot>,
        active_page: usize,
    },
    Load {
        path: PathBuf,
    },
    Export {
        path: PathBuf,
        pages: Vec<PageSnapshot>,
        kind: Kind,
    },
}

pub enum Transfer {
    Saved {
        path: PathBuf,
    },
    Loaded {
        path: PathBuf,
        pages: Vec<PageSnapshot>,
        active_page: usize,
    },
    Exported {
        path: PathBuf,
        kind: Kind,
    },
}

pub fn perform(task: Task) -> std::result::Result<Transfer, String> {
    unsafe {
        RoInitialize(RO_INIT_MULTITHREADED).map_err(|e| e.to_string())?;
    }
    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe {
                RoUninitialize();
            }
        }
    }
    let _apartment = Apartment;
    perform_inner(task).map_err(|e| e.to_string())
}

fn perform_inner(task: Task) -> std::result::Result<Transfer, Box<dyn std::error::Error>> {
    match task {
        Task::Save {
            path,
            pages,
            active_page,
        } => {
            crate::storage::validate_extension(&path, "inknote")?;
            crate::validation::snapshots(&pages)?;
            let mut stored = Vec::with_capacity(pages.len());
            for snapshot in pages {
                let container = container(snapshot.strokes)?;
                scale_strokes(&container, 1.0 / snapshot.scale)?;
                stored.push(NotePage {
                    page: snapshot.page,
                    ink: encode(&container)?,
                });
            }
            note::save(
                &path,
                &Note {
                    pages: stored,
                    active_page,
                },
            )?;
            Ok(Transfer::Saved { path })
        }
        Task::Load { path } => {
            let note = note::load(&path)?;
            let mut pages = Vec::with_capacity(note.pages.len());
            let mut budget = crate::validation::InkBudget::default();
            for stored in note.pages {
                let container = decode(&stored.ink)?;
                budget.container(&container, 1.0)?;
                pages.push(PageSnapshot {
                    page: stored.page,
                    strokes: clone_strokes(&container)?,
                    scale: 1.0,
                });
            }
            Ok(Transfer::Loaded {
                path,
                pages,
                active_page: note.active_page,
            })
        }
        Task::Export { path, pages, kind } => {
            crate::storage::validate_extension(&path, kind.extension())?;
            crate::validation::snapshots(&pages)?;
            let mut normalized = Vec::with_capacity(pages.len());
            for snapshot in pages {
                let container = container(snapshot.strokes)?;
                scale_strokes(&container, 1.0 / snapshot.scale)?;
                normalized.push(PageSnapshot {
                    page: snapshot.page,
                    strokes: clone_strokes(&container)?,
                    scale: 1.0,
                });
            }
            crate::export::export(&path, &normalized, kind)?;
            Ok(Transfer::Exported { path, kind })
        }
    }
}

pub fn container(strokes: Vec<InkStroke>) -> Result<InkStrokeContainer> {
    let container = InkStrokeContainer::new()?;
    for stroke in strokes {
        container.AddStroke(&stroke)?;
    }
    Ok(container)
}

pub fn clone_strokes(container: &InkStrokeContainer) -> Result<Vec<InkStroke>> {
    let strokes = container.GetStrokes()?;
    let mut cloned = Vec::with_capacity(strokes.Size()? as usize);
    for index in 0..strokes.Size()? {
        cloned.push(strokes.GetAt(index)?.Clone()?);
    }
    Ok(cloned)
}

fn encode(container: &InkStrokeContainer) -> Result<Vec<u8>> {
    let stream = InMemoryRandomAccessStream::new()?;
    container
        .SaveWithFormatAsync(&stream, InkPersistenceFormat::Isf)?
        .join()?;
    let size = stream.Size()?;
    if size > note::MAX_PAGE_INK_BYTES as u64 {
        return Err(Error::new(E_FAIL, "筆跡データが大きすぎます。"));
    }
    let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0)?)?;
    if reader.LoadAsync(size as u32)?.join()? != size as u32 {
        return Err(Error::new(
            E_FAIL,
            "筆跡データを最後まで読み取れませんでした。",
        ));
    }
    let mut bytes = vec![0; size as usize];
    reader.ReadBytes(&mut bytes)?;
    crate::isf::validate(&bytes)?;
    Ok(bytes)
}

fn decode(bytes: &[u8]) -> Result<InkStrokeContainer> {
    crate::isf::validate(bytes)?;
    let stream = InMemoryRandomAccessStream::new()?;
    let writer = DataWriter::CreateDataWriter(&stream.GetOutputStreamAt(0)?)?;
    writer.WriteBytes(bytes)?;
    writer.StoreAsync()?.join()?;
    writer.DetachStream()?;
    let container = InkStrokeContainer::new()?;
    container.LoadAsync(&stream.GetInputStreamAt(0)?)?.join()?;
    Ok(container)
}

pub fn scale_strokes(container: &InkStrokeContainer, ratio: f32) -> Result<()> {
    crate::validation::scale(ratio)?;
    if (ratio - 1.0).abs() < 0.0001 {
        return Ok(());
    }
    let strokes = container.GetStrokes()?;
    for index in 0..strokes.Size()? {
        let stroke = strokes.GetAt(index)?;
        let mut transform = stroke.PointTransform()?;
        transform.M11 *= ratio;
        transform.M12 *= ratio;
        transform.M21 *= ratio;
        transform.M22 *= ratio;
        transform.M31 *= ratio;
        transform.M32 *= ratio;
        stroke.SetPointTransform(transform)?;
        let attributes = stroke.DrawingAttributes()?;
        let mut size = attributes.Size()?;
        size.Width *= ratio;
        size.Height *= ratio;
        attributes.SetSize(size)?;
        stroke.SetDrawingAttributes(&attributes)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::RuleSpacing;
    use windows::{
        Foundation::{Point, Size},
        UI::{
            Color,
            Input::Inking::{InkDrawingAttributes, InkPoint, InkStrokeBuilder},
        },
    };
    use windows_collections::IIterable;

    // The app permits one note operation at a time; test the native codec the same way.
    static NATIVE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    struct Apartment {
        _guard: std::sync::MutexGuard<'static, ()>,
    }
    impl Apartment {
        fn new() -> Result<Self> {
            let guard = NATIVE.lock().unwrap();
            unsafe {
                RoInitialize(RO_INIT_MULTITHREADED)?;
            }
            Ok(Self { _guard: guard })
        }
    }
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe {
                RoUninitialize();
            }
        }
    }

    fn sample() -> Result<InkStroke> {
        let builder = InkStrokeBuilder::new()?;
        let attributes = InkDrawingAttributes::new()?;
        attributes.SetColor(Color {
            A: 255,
            R: 31,
            G: 49,
            B: 69,
        })?;
        attributes.SetSize(Size {
            Width: 3.0,
            Height: 3.0,
        })?;
        attributes.SetIgnorePressure(false)?;
        builder.SetDefaultDrawingAttributes(&attributes)?;
        let points: IIterable<InkPoint> = vec![
            Some(InkPoint::CreateInkPoint(Point { X: 10.0, Y: 20.0 }, 0.25)?),
            Some(InkPoint::CreateInkPoint(Point { X: 60.0, Y: 30.0 }, 0.75)?),
            Some(InkPoint::CreateInkPoint(Point { X: 110.0, Y: 40.0 }, 0.50)?),
        ]
        .into();
        builder.CreateStrokeFromInkPoints(&points, windows_numerics::Matrix3x2::identity())
    }

    #[test]
    fn native_isf_preserves_points_pressure_attributes_and_empty_notes() -> Result<()> {
        let _apartment = Apartment::new()?;
        let original = container(vec![sample()?])?;
        let bytes = encode(&original)?;
        assert!(!bytes.starts_with(b"GIF"));
        let restored = decode(&bytes)?;
        assert_eq!(restored.GetStrokes()?.Size()?, 1);
        let stroke = restored.GetStrokes()?.GetAt(0)?;
        let attributes = stroke.DrawingAttributes()?;
        assert_eq!(attributes.Color()?.B, 69);
        assert!(!attributes.IgnorePressure()?);
        assert!((attributes.Size()?.Width - 3.0).abs() < 0.05);
        let points = stroke.GetInkPoints().expect("loaded stroke ink points");
        assert_eq!(points.Size()?, 3);
        assert!((points.GetAt(0)?.Pressure()? - 0.25).abs() < 0.01);
        assert!((points.GetAt(1)?.Pressure()? - 0.75).abs() < 0.01);
        assert!((points.GetAt(2)?.Position()?.X - 110.0).abs() < 0.05);
        let fixed = sample()?;
        let attributes = fixed.DrawingAttributes()?;
        attributes.SetIgnorePressure(true)?;
        fixed.SetDrawingAttributes(&attributes)?;
        let fixed = decode(&encode(&container(vec![fixed])?)?)?;
        assert!(
            fixed
                .GetStrokes()?
                .GetAt(0)?
                .DrawingAttributes()?
                .IgnorePressure()?
        );
        let empty = encode(&InkStrokeContainer::new()?).expect("encode empty ISF");
        assert!(!empty.is_empty());
        assert_eq!(
            decode(&empty)
                .expect("decode empty ISF")
                .GetStrokes()?
                .Size()?,
            0
        );
        assert!(decode(b"broken ISF").is_err());
        Ok(())
    }

    #[test]
    fn dpi_scaling_rescales_geometry_and_restores_it() -> Result<()> {
        let _apartment = Apartment::new()?;
        let strokes = container(vec![sample()?])?;
        let original = strokes.BoundingRect()?;
        scale_strokes(&strokes, 2.0)?;
        let scaled = strokes.BoundingRect()?;
        assert!(
            (scaled.Width - original.Width * 2.0).abs() < 0.1,
            "{original:?} -> {scaled:?}"
        );
        assert!(
            (scaled.Height - original.Height * 2.0).abs() < 0.1,
            "{original:?} -> {scaled:?}"
        );
        scale_strokes(&strokes, 0.5)?;
        assert!((strokes.BoundingRect()?.Width - original.Width).abs() < 0.1);
        Ok(())
    }

    #[test]
    fn repeated_zoom_preserves_saved_ink_coordinates_width_and_pressure() -> Result<()> {
        let _apartment = Apartment::new()?;
        let stroke = sample()?;
        let mut transform = stroke.PointTransform()?;
        transform.M31 = 80.0;
        transform.M32 = 100.0;
        stroke.SetPointTransform(transform)?;
        let strokes = container(vec![stroke])?;
        let before = strokes.BoundingRect()?;
        let mut scale = 1.0;
        for _ in 0..12 {
            // Manual zoom, fit modes and moving to another monitor all use
            // the same native scale operation, without a scroll translation.
            for next in [0.25, 0.6, 1.2, 4.0, 1.55, 2.0, 0.45] {
                scale_strokes(&strokes, next / scale)?;
                scale = next;
            }
        }
        let saved = container(clone_strokes(&strokes)?)?;
        scale_strokes(&saved, 1.0 / scale)?;
        let restored = decode(&encode(&saved)?)?;
        let after = restored.BoundingRect()?;
        assert!((after.X - before.X).abs() < 0.1);
        assert!((after.Y - before.Y).abs() < 0.1);
        assert!((after.Width - before.Width).abs() < 0.1);
        assert!((after.Height - before.Height).abs() < 0.1);
        let restored = restored.GetStrokes()?.GetAt(0)?;
        let point = restored.GetRenderingSegments()?.GetAt(0)?.Position()?;
        assert!((point.X - 90.0).abs() < 0.05, "{point:?}");
        assert!((point.Y - 120.0).abs() < 0.05, "{point:?}");
        assert!((restored.DrawingAttributes()?.Size()?.Width - 3.0).abs() < 0.05);
        assert!((restored.GetInkPoints()?.GetAt(1)?.Pressure()? - 0.75).abs() < 0.01);
        assert!(
            (strokes
                .GetStrokes()?
                .GetAt(0)?
                .DrawingAttributes()?
                .Size()?
                .Width
                - 3.0 * scale)
                .abs()
                < 0.05
        );
        Ok(())
    }

    #[test]
    fn exports_mixed_sizes_orientation_pressure_and_blank_page_to_pdf() -> Result<()> {
        let _apartment = Apartment::new()?;
        use crate::paper::{Orientation, Paper, PaperSize};
        let mut pages = Vec::new();
        for size in [PaperSize::A4, PaperSize::A5, PaperSize::B4, PaperSize::B5] {
            for orientation in [Orientation::Portrait, Orientation::Landscape] {
                let fixed = sample()?;
                let attributes = fixed.DrawingAttributes()?;
                attributes.SetIgnorePressure(true)?;
                attributes.SetSize(Size {
                    Width: 2.0,
                    Height: 2.0,
                })?;
                fixed.SetDrawingAttributes(&attributes)?;
                let mut transform = windows_numerics::Matrix3x2::identity();
                transform.M31 = 80.0;
                transform.M32 = 100.0;
                fixed.SetPointTransform(transform)?;
                pages.push(PageSnapshot {
                    page: Page {
                        paper: Paper { size, orientation },
                        rule_spacing: RuleSpacing::Standard,
                    },
                    strokes: vec![sample()?, fixed],
                    scale: 1.0,
                });
            }
        }
        pages.push(PageSnapshot {
            page: Page::default(),
            strokes: vec![],
            scale: 1.0,
        });
        let output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/pdf-tests");
        std::fs::create_dir_all(&output).unwrap();
        let path = output.join("用紙サイズと筆圧の検証.pdf");
        perform(Task::Export {
            path: path.clone(),
            pages,
            kind: Kind::Pdf,
        })
        .unwrap();
        let bytes = std::fs::read(path).unwrap();
        assert!(bytes.starts_with(b"%PDF-1.4"));
        assert!(bytes.ends_with(b"%%EOF\n"));
        // The fixed stroke starts at (10,20), translated by (80,100).
        // PDF must include that translation exactly once, independent of DPI.
        let position = b"90.0000 120.0000 m";
        assert!(bytes.windows(position.len()).any(|part| part == position));
        Ok(())
    }

    #[test]
    fn exports_png_svg_and_all_pages_archives_with_canonical_coordinates() -> Result<()> {
        let _apartment = Apartment::new()?;
        use crate::paper::{Orientation, Paper, PaperSize};
        let output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/image-export-tests");
        std::fs::create_dir_all(&output).unwrap();
        let original = sample()?;
        let attributes = original.DrawingAttributes()?;
        attributes.SetIgnorePressure(true)?;
        original.SetDrawingAttributes(&attributes)?;
        let mut transform = windows_numerics::Matrix3x2::identity();
        transform.M31 = 80.0;
        transform.M32 = 100.0;
        original.SetPointTransform(transform)?;
        for (kind, name) in [
            (Kind::Png, "現在のページ.png"),
            (Kind::Svg, "現在のページ.svg"),
            (Kind::PngZip, "全ページPNG.zip"),
            (Kind::SvgZip, "全ページSVG.zip"),
        ] {
            let mut pages = Vec::new();
            for size in [PaperSize::A4, PaperSize::A5, PaperSize::B4, PaperSize::B5] {
                for orientation in [Orientation::Portrait, Orientation::Landscape] {
                    if kind.current_only() && !pages.is_empty() {
                        continue;
                    }
                    // Simulate a page fitted to half its physical size. Export
                    // must undo viewport scaling exactly once on clones only.
                    let strokes = container(vec![original.Clone()?])?;
                    scale_strokes(&strokes, 0.5)?;
                    pages.push(PageSnapshot {
                        page: Page {
                            paper: Paper { size, orientation },
                            rule_spacing: RuleSpacing::Standard,
                        },
                        strokes: clone_strokes(&strokes)?,
                        scale: 0.5,
                    });
                }
            }
            if !kind.current_only() {
                pages.push(PageSnapshot {
                    page: Page::default(),
                    strokes: vec![],
                    scale: 1.0,
                });
            }
            let path = output.join(name);
            perform(Task::Export {
                path: path.clone(),
                pages,
                kind,
            })
            .unwrap();
            let bytes = std::fs::read(&path).unwrap();
            match kind {
                Kind::Png => {
                    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
                    assert_eq!(u32::from_be_bytes(bytes[16..20].try_into().unwrap()), 2480);
                    assert_eq!(u32::from_be_bytes(bytes[20..24].try_into().unwrap()), 3508);
                }
                Kind::Svg => {
                    let text = std::str::from_utf8(&bytes).unwrap();
                    assert!(text.contains("width=\"210mm\" height=\"297mm\""));
                    assert!(text.contains("M 90.0000 120.0000"));
                }
                Kind::PngZip | Kind::SvgZip => {
                    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
                    assert_eq!(archive.len(), 9);
                    let ext = if kind == Kind::PngZip { "png" } else { "svg" };
                    assert!(archive.by_name(&format!("page-0001.{ext}")).is_ok());
                    assert!(archive.by_name(&format!("page-0009.{ext}")).is_ok());
                }
                Kind::Pdf => unreachable!(),
            }
        }
        assert_eq!(original.PointTransform()?, transform);
        assert!((original.DrawingAttributes()?.Size()?.Width - 3.0).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn page_deletion_round_trip_preserves_ink_and_exports_remaining_pages() -> Result<()> {
        let _apartment = Apartment::new()?;
        use crate::paper::{Orientation, Paper, PaperSize};
        use std::io::Read;
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/page-delete-tests");
        std::fs::create_dir_all(&directory).unwrap();
        let mut pages = Vec::new();
        for (index, size) in [PaperSize::A4, PaperSize::A5, PaperSize::B4, PaperSize::B5]
            .into_iter()
            .enumerate()
        {
            let stroke = sample()?;
            let mut transform = stroke.PointTransform()?;
            transform.M31 = index as f32 * 80.0;
            stroke.SetPointTransform(transform)?;
            pages.push(PageSnapshot {
                page: Page {
                    paper: Paper {
                        size,
                        orientation: if index == 3 {
                            Orientation::Landscape
                        } else {
                            Orientation::Portrait
                        },
                    },
                    rule_spacing: RuleSpacing::Wide,
                },
                strokes: vec![stroke],
                scale: 1.0,
            });
        }
        let mut active = 1;
        crate::ink::delete_current_page(&mut pages, &mut active, |next| {
            assert_eq!(next.page.paper.size, PaperSize::B4);
            Ok(())
        })?;
        let path = directory.join("削除後の3ページ.inknote");
        perform(Task::Save {
            path: path.clone(),
            pages,
            active_page: active,
        })
        .unwrap();
        let Transfer::Loaded {
            mut pages,
            active_page,
            ..
        } = perform(Task::Load { path }).unwrap()
        else {
            panic!("Expected a loaded note")
        };
        assert_eq!(active_page, 1);
        assert_eq!(pages.len(), 3);
        for (page, (size, x)) in pages.iter().zip([
            (PaperSize::A4, 10.0),
            (PaperSize::B4, 170.0),
            (PaperSize::B5, 250.0),
        ]) {
            assert_eq!(page.page.paper.size, size);
            assert_eq!(page.page.rule_spacing, RuleSpacing::Wide);
            assert_eq!(page.strokes.len(), 1);
            let stroke = &page.strokes[0];
            let point = stroke.GetRenderingSegments()?.GetAt(0)?.Position()?;
            assert!((point.X - x).abs() < 0.05 && (point.Y - 20.0).abs() < 0.05);
            assert!((stroke.DrawingAttributes()?.Size()?.Width - 3.0).abs() < 0.05);
            assert!((stroke.GetInkPoints()?.GetAt(1)?.Pressure()? - 0.75).abs() < 0.01);
        }
        let exported = pages
            .iter()
            .map(|page| {
                Ok(PageSnapshot {
                    page: page.page,
                    strokes: page
                        .strokes
                        .iter()
                        .map(InkStroke::Clone)
                        .collect::<Result<Vec<_>>>()?,
                    scale: page.scale,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let archive_path = directory.join("削除後の3ページSVG.zip");
        perform(Task::Export {
            path: archive_path.clone(),
            pages: exported,
            kind: Kind::SvgZip,
        })
        .unwrap();
        let mut archive = zip::ZipArchive::new(std::fs::File::open(archive_path).unwrap()).unwrap();
        assert_eq!(archive.len(), 3);
        for (index, (w, h, expected_x)) in
            [(210, 297, 10.0f32), (257, 364, 170.0), (257, 182, 250.0)]
                .into_iter()
                .enumerate()
        {
            let mut entry = archive.by_index(index).unwrap();
            assert_eq!(entry.name().unwrap(), format!("page-{:04}.svg", index + 1));
            let mut text = String::new();
            entry.read_to_string(&mut text).unwrap();
            assert!(text.contains(&format!("width=\"{w}mm\" height=\"{h}mm\"")));
            assert!(text.split("M ").skip(1).any(|part| {
                let mut values = part.split_whitespace();
                let x = values
                    .next()
                    .and_then(|v| v.parse::<f32>().ok())
                    .unwrap_or(f32::NAN);
                let y = values
                    .next()
                    .and_then(|v| v.parse::<f32>().ok())
                    .unwrap_or(f32::NAN);
                (x - expected_x).abs() < 0.05 && (y - 20.0).abs() < 0.05
            }));
        }
        active = 0;
        crate::ink::delete_current_page(&mut pages, &mut active, |_| Ok(()))?;
        assert_eq!(pages[active].page.paper.size, PaperSize::B4);
        active = 1;
        crate::ink::delete_current_page(&mut pages, &mut active, |_| Ok(()))?;
        assert_eq!(pages.len(), 1);
        assert_eq!(active, 0);
        assert!(
            crate::ink::delete_current_page(&mut pages, &mut active, |_| panic!(
                "must retain the final page"
            ))
            .is_err()
        );
        assert_eq!(pages[0].page.paper.size, PaperSize::B4);
        let final_path = directory.join("最後の1ページ.inknote");
        perform(Task::Save {
            path: final_path.clone(),
            pages,
            active_page: active,
        })
        .unwrap();
        let Transfer::Loaded {
            pages, active_page, ..
        } = perform(Task::Load { path: final_path }).unwrap()
        else {
            panic!("Expected a loaded note")
        };
        assert_eq!(pages.len(), 1);
        assert_eq!(active_page, 0);
        assert_eq!(pages[0].page.paper.size, PaperSize::B4);
        assert!(
            (pages[0].strokes[0]
                .GetRenderingSegments()?
                .GetAt(0)?
                .Position()?
                .X
                - 170.0)
                .abs()
                < 0.05
        );
        Ok(())
    }

    #[test]
    fn rejects_invalid_geometry_and_scale_before_replacing_output_or_loading_ink() -> Result<()> {
        let _apartment = Apartment::new()?;
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/security-ink-tests");
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("previous.svg");
        std::fs::write(&path, b"previous output").unwrap();
        for scale in [0.0, f32::NAN, f32::INFINITY, -1.0] {
            assert!(
                perform(Task::Export {
                    path: path.clone(),
                    kind: Kind::Svg,
                    pages: vec![PageSnapshot {
                        page: Page::default(),
                        strokes: vec![sample()?],
                        scale
                    }]
                })
                .is_err()
            );
            assert_eq!(std::fs::read(&path).unwrap(), b"previous output");
        }
        let stroke = sample()?;
        let mut transform = stroke.PointTransform()?;
        transform.M31 = crate::validation::MAX_COORDINATE * 2.0;
        stroke.SetPointTransform(transform)?;
        assert!(
            perform(Task::Export {
                path: path.clone(),
                kind: Kind::Svg,
                pages: vec![PageSnapshot {
                    page: Page::default(),
                    strokes: vec![stroke.Clone()?],
                    scale: 1.0
                }]
            })
            .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"previous output");
        let bad_note = directory.join("invalid-geometry.inknote");
        note::save(
            &bad_note,
            &Note {
                active_page: 0,
                pages: vec![NotePage {
                    page: Page::default(),
                    ink: encode(&container(vec![stroke])?)?,
                }],
            },
        )
        .unwrap();
        assert!(perform(Task::Load { path: bad_note }).is_err());
        let wrong = directory.join("do-not-overwrite.exe");
        std::fs::write(&wrong, b"application").unwrap();
        assert!(
            perform(Task::Save {
                path: wrong.clone(),
                pages: vec![PageSnapshot {
                    page: Page::default(),
                    strokes: vec![sample()?],
                    scale: 1.0
                }],
                active_page: 0
            })
            .is_err()
        );
        assert_eq!(std::fs::read(wrong).unwrap(), b"application");
        Ok(())
    }

    #[test]
    fn reads_golden_version_two_note_written_before_security_changes() -> Result<()> {
        let _apartment = Apartment::new()?;
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/security-ink-tests");
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("version-010.inknote");
        std::fs::write(
            &path,
            include_bytes!("../tests/fixtures/v2-mixed-paper.inknote"),
        )
        .unwrap();
        let Transfer::Loaded {
            pages, active_page, ..
        } = perform(Task::Load { path }).unwrap()
        else {
            panic!("Expected a loaded note");
        };
        assert_eq!(pages.len(), 3);
        assert_eq!(active_page, 1);
        assert_eq!(pages[1].page.paper.size, crate::paper::PaperSize::B4);
        assert_eq!(
            pages[2].page.paper.orientation,
            crate::paper::Orientation::Landscape
        );
        for (page, x) in pages.iter().zip([10.0, 170.0, 250.0]) {
            assert_eq!(page.strokes.len(), 1);
            assert!(
                (page.strokes[0]
                    .GetRenderingSegments()?
                    .GetAt(0)?
                    .Position()?
                    .X
                    - x)
                    .abs()
                    < 0.05
            );
        }
        Ok(())
    }

    #[test]
    fn note_file_can_be_loaded_appended_and_saved_again() -> Result<()> {
        let _apartment = Apartment::new()?;
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/native-note-tests");
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join(format!("筆跡の保存-{}.inknote", std::process::id()));
        let page = Page {
            rule_spacing: RuleSpacing::Wide,
            ..Default::default()
        };
        perform(Task::Save {
            path: path.clone(),
            pages: vec![PageSnapshot {
                page,
                strokes: vec![sample()?],
                scale: 1.0,
            }],
            active_page: 0,
        })
        .unwrap();
        let Transfer::Loaded { mut pages, .. } =
            perform(Task::Load { path: path.clone() }).unwrap()
        else {
            panic!("Expected a loaded note")
        };
        assert_eq!(pages[0].page, page);
        assert_eq!(pages[0].strokes.len(), 1);
        pages[0].strokes.push(sample()?);
        pages.push(PageSnapshot {
            page: Page::default(),
            strokes: vec![],
            scale: 1.0,
        });
        perform(Task::Save {
            path: path.clone(),
            pages,
            active_page: 1,
        })
        .unwrap();
        let Transfer::Loaded {
            pages, active_page, ..
        } = perform(Task::Load { path: path.clone() }).unwrap()
        else {
            panic!("Expected a loaded note")
        };
        assert_eq!(active_page, 1);
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].strokes.len(), 2);
        assert!(pages[1].strokes.is_empty());
        // Keep the generated note under target/ for UI round-trip checks.
        Ok(())
    }
}
