//! Version 2: fixed physical paper sizes and one native ISF stream per page.
use crate::storage::atomic_write;
use crate::{paper::Paper, settings::RuleSpacing};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{self, Read, Write},
    path::Path,
};
use zip::{
    CompressionMethod, ZipWriter,
    read::{ZipFile, ZipFileEntry},
    unstable::stream::{ZipStreamReader, ZipStreamVisitor},
    write::SimpleFileOptions,
};

pub const MAX_INK_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_PAGE_INK_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_PAGES: usize = 1000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub paper: Paper,
    pub rule_spacing: RuleSpacing,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: String,
    version: u32,
    active_page: usize,
    pages: Vec<Page>,
}
pub struct NotePage {
    pub page: Page,
    pub ink: Vec<u8>,
}
pub struct Note {
    pub pages: Vec<NotePage>,
    pub active_page: usize,
}

pub fn load(path: &Path) -> io::Result<Note> {
    crate::storage::validate_extension(path, "inknote")?;
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > (MAX_INK_BYTES as u64 * 2) {
        return Err(invalid("ノートファイルが大きすぎます。"));
    }
    // Stream entries and cap actual counts before allocating central-directory
    // tables. Strict mode verifies both headers, duplicate names and the footer.
    let mut visitor = NoteReader::default();
    ZipStreamReader::new(file.take(MAX_INK_BYTES as u64 * 2 + 1))
        .enforce_header_consistency(true)
        .visit(&mut visitor)
        .map_err(zip_error)?;
    let json = visitor
        .files
        .remove("document.json")
        .ok_or_else(|| invalid("ノートの文書情報がありません。"))?;
    let manifest: Manifest = serde_json::from_slice(&json).map_err(|e| invalid(e.to_string()))?;
    if manifest.format != "MioNote" || manifest.version != 2 {
        return Err(invalid("対応していないノート形式・バージョンです。"));
    }
    validate_pages(manifest.pages.len(), manifest.active_page)?;
    if visitor.files.len() != manifest.pages.len() {
        return Err(invalid("ノートのページ構成が一致しません。"));
    }
    let mut pages = Vec::with_capacity(manifest.pages.len());
    for (index, page) in manifest.pages.into_iter().enumerate() {
        let ink = visitor
            .files
            .remove(&format!("pages/{index}.isf"))
            .ok_or_else(|| invalid("ページの筆跡データがありません。"))?;
        if ink.is_empty() {
            return Err(invalid("筆跡データがありません。"));
        }
        pages.push(NotePage { page, ink });
    }
    Ok(Note {
        pages,
        active_page: manifest.active_page,
    })
}
fn validate_pages(count: usize, active: usize) -> io::Result<()> {
    if count == 0 || count > MAX_PAGES || active >= count {
        return Err(invalid("ノートのページ数・表示ページが不正です。"));
    }
    Ok(())
}
#[derive(Default)]
struct NoteReader {
    files: BTreeMap<String, Vec<u8>>,
    total_ink: usize,
    metadata_count: usize,
}
fn valid_entry_name(name: &str) -> bool {
    name == "document.json"
        || name
            .strip_prefix("pages/")
            .and_then(|n| n.strip_suffix(".isf"))
            .and_then(|n| n.parse::<usize>().ok().map(|index| (n, index)))
            .is_some_and(|(name, index)| index < MAX_PAGES && name == index.to_string())
}
impl ZipStreamVisitor for NoteReader {
    fn visit_file<R: Read>(&mut self, file: &mut ZipFile<'_, R>) -> zip::result::ZipResult<()> {
        let name = file.name()?.into_owned();
        if self.files.len() > MAX_PAGES
            || !valid_entry_name(&name)
            || self.files.contains_key(&name)
            || file.compression() != CompressionMethod::Stored
            || file.encrypted()
            || !file.is_file()
        {
            return Err(invalid("ノートのZIPエントリーが不正です。").into());
        }
        let json = name == "document.json";
        let limit = if json {
            1024 * 1024
        } else {
            MAX_PAGE_INK_BYTES.min(MAX_INK_BYTES - self.total_ink)
        };
        if file.size() == 0 || file.size() > limit as u64 || file.compressed_size() != file.size() {
            return Err(invalid("ノートのデータサイズが不正です。").into());
        }
        let expected = file.size();
        let mut bytes = Vec::new();
        (&mut *file)
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 != expected {
            return Err(invalid("ノートのデータ長が一致しません。").into());
        }
        if !json {
            self.total_ink += bytes.len();
        }
        self.files.insert(name, bytes);
        Ok(())
    }
    fn visit_additional_metadata(
        &mut self,
        entry: &ZipFileEntry<'_>,
    ) -> zip::result::ZipResult<()> {
        self.metadata_count += 1;
        if self.metadata_count > MAX_PAGES + 1
            || !entry.is_file()
            || entry.compression() != CompressionMethod::Stored
        {
            return Err(invalid("ノートのZIPメタデータが不正です。").into());
        }
        Ok(())
    }
}
pub fn save(path: &Path, note: &Note) -> io::Result<()> {
    crate::storage::validate_extension(path, "inknote")?;
    validate_pages(note.pages.len(), note.active_page)?;
    let total = note
        .pages
        .iter()
        .try_fold(0usize, |total, page| total.checked_add(page.ink.len()));
    if total.is_none_or(|n| n > MAX_INK_BYTES)
        || note
            .pages
            .iter()
            .any(|p| p.ink.is_empty() || p.ink.len() > MAX_PAGE_INK_BYTES)
    {
        return Err(invalid("筆跡データのサイズが不正です。"));
    }
    let manifest = Manifest {
        format: "MioNote".into(),
        version: 2,
        active_page: note.active_page,
        pages: note.pages.iter().map(|p| p.page).collect(),
    };
    let json = serde_json::to_vec_pretty(&manifest).map_err(|e| invalid(e.to_string()))?;
    atomic_write(path, |file| {
        let mut archive = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        archive
            .start_file("document.json", options)
            .map_err(zip_error)?;
        archive.write_all(&json)?;
        for (index, page) in note.pages.iter().enumerate() {
            archive
                .start_file(format!("pages/{index}.isf"), options)
                .map_err(zip_error)?;
            archive.write_all(&page.ink)?;
        }
        archive.finish().map_err(zip_error)?;
        Ok(())
    })
}
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
fn zip_error(error: zip::result::ZipError) -> io::Error {
    invalid(format!("ノートのZIPデータを処理できません: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paper::{Orientation, PaperSize};
    use std::fs;
    use zip::ZipArchive;
    fn path(name: &str) -> std::path::PathBuf {
        let directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/page-note-tests")
            .join(std::process::id().to_string());
        fs::create_dir_all(&directory).unwrap();
        directory.join(name)
    }
    #[test]
    fn mixed_paper_pages_round_trip_and_failed_save_preserves_previous_file() {
        let path = path("複数ページ.inknote");
        let mut note = Note {
            active_page: 1,
            pages: vec![
                NotePage {
                    page: Page::default(),
                    ink: vec![1, 2, 3],
                },
                NotePage {
                    page: Page {
                        paper: Paper {
                            size: PaperSize::B5,
                            orientation: Orientation::Landscape,
                        },
                        rule_spacing: RuleSpacing::Wide,
                    },
                    ink: vec![4, 5, 6],
                },
            ],
        };
        save(&path, &note).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.active_page, 1);
        assert_eq!(loaded.pages.len(), 2);
        assert_eq!(loaded.pages[1].page, note.pages[1].page);
        assert_eq!(loaded.pages[0].ink, vec![1, 2, 3]);
        note.pages[0].ink = vec![9];
        save(&path, &note).unwrap();
        note.active_page = 2;
        assert!(save(&path, &note).is_err());
        assert_eq!(load(&path).unwrap().pages[0].ink, vec![9]);
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn rejects_old_format_corruption_invalid_paper_and_missing_page() {
        let path = path("不正なノート.inknote");
        for (name, json) in [
            ("page.json", r#"{"format":"InkNote","version":1}"#),
            (
                "document.json",
                r#"{"format":"MioNote","version":2,"active_page":0,"pages":[]}"#,
            ),
            (
                "document.json",
                r#"{"format":"MioNote","version":2,"active_page":0,"pages":[{"paper":{"size":"A3","orientation":"portrait"},"rule_spacing":"standard"}]}"#,
            ),
            (
                "document.json",
                r#"{"format":"MioNote","version":2,"active_page":0,"pages":[{"paper":{"size":"A4","orientation":"portrait"},"rule_spacing":"standard"}]}"#,
            ),
        ] {
            let mut archive = ZipWriter::new(File::create(&path).unwrap());
            archive
                .start_file(
                    name,
                    SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
                )
                .unwrap();
            archive.write_all(json.as_bytes()).unwrap();
            archive.finish().unwrap();
            assert!(load(&path).is_err());
        }
        fs::write(&path, b"broken").unwrap();
        assert!(load(&path).is_err());
        assert!(
            atomic_write(&path, |file| {
                file.write_all(b"partial")?;
                Err(io::Error::other("failed"))
            })
            .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), b"broken");
        fs::remove_file(path).unwrap();
    }

    fn archive(path: &Path, entries: &[(&str, &[u8])]) {
        let mut writer = ZipWriter::new(File::create(path).unwrap());
        for (name, bytes) in entries {
            writer
                .start_file(
                    *name,
                    SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
                )
                .unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap();
    }
    fn json() -> Vec<u8> {
        serde_json::to_vec(&Manifest {
            format: "MioNote".into(),
            version: 2,
            active_page: 0,
            pages: vec![Page::default()],
        })
        .unwrap()
    }

    #[test]
    fn rejects_duplicate_hidden_entries_dual_headers_and_false_entry_counts() {
        let path = path("hostile.inknote");
        let json = json();
        archive(
            &path,
            &[
                ("document.json", &json),
                ("pages/0.isf", b"ink"),
                ("pages/1.isf", b"other"),
            ],
        );
        let mut bytes = fs::read(&path).unwrap();
        for start in (0..bytes.len() - 11).collect::<Vec<_>>() {
            if &bytes[start..start + 11] == b"pages/1.isf" {
                bytes[start..start + 11].copy_from_slice(b"pages/0.isf");
            }
        }
        fs::write(&path, bytes).unwrap();
        assert!(load(&path).is_err());
        for name in ["../outside.txt", "pages/000.isf", "unexpected.txt"] {
            archive(
                &path,
                &[
                    ("document.json", &json),
                    ("pages/0.isf", b"ink"),
                    (name, b"extra"),
                ],
            );
            assert!(load(&path).is_err(), "{name}");
        }
        archive(&path, &[("document.json", &json), ("pages/0.isf", b"ink")]);
        let valid = fs::read(&path).unwrap();
        let mut inconsistent = valid.clone();
        let start = inconsistent
            .windows(11)
            .position(|p| p == b"pages/0.isf")
            .unwrap();
        inconsistent[start + 6] = b'1'; // Local header only; central header still names page zero.
        fs::write(&path, inconsistent).unwrap();
        assert!(load(&path).is_err());
        let mut false_count = valid.clone();
        let end = false_count
            .windows(4)
            .rposition(|p| p == b"PK\x05\x06")
            .unwrap();
        false_count[end + 8..end + 12].copy_from_slice(&[0xfe, 0xff, 0xfe, 0xff]);
        fs::write(&path, false_count).unwrap();
        assert!(load(&path).is_err());
        fs::write(&path, &valid[..valid.len() - 8]).unwrap();
        assert!(load(&path).is_err());
    }

    #[test]
    fn rejects_bad_crc_oversized_entries_and_mutated_metadata_without_panicking() {
        let path = path("mutations.inknote");
        archive(
            &path,
            &[("document.json", &json()), ("pages/0.isf", b"ink")],
        );
        let valid = fs::read(&path).unwrap();
        let offset = ZipArchive::new(File::open(&path).unwrap())
            .unwrap()
            .by_name("pages/0.isf")
            .unwrap()
            .data_start()
            .unwrap() as usize;
        let mut corrupt = valid.clone();
        corrupt[offset] ^= 0xff;
        fs::write(&path, corrupt).unwrap();
        assert!(load(&path).is_err());
        let mut huge = valid.clone();
        huge[18..22].copy_from_slice(&u32::MAX.to_le_bytes());
        huge[22..26].copy_from_slice(&u32::MAX.to_le_bytes());
        fs::write(&path, huge).unwrap();
        assert!(load(&path).is_err());
        for index in 0..valid.len().min(256) {
            let mut mutated = valid.clone();
            mutated[index] ^= 0x5a;
            fs::write(&path, mutated).unwrap();
            assert!(
                std::panic::catch_unwind(|| load(&path)).is_ok(),
                "mutation {index}"
            );
        }
    }
}
