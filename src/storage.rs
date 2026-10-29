//! File-system boundaries shared by notes, preferences, diagnostics and exports.
use std::{
    ffi::OsString,
    fs::{self, File},
    io::{self, Write},
    os::windows::ffi::OsStrExt,
    path::{Component, Path, PathBuf, Prefix},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

mod security;

pub fn validate_path(path: &Path) -> io::Result<()> {
    let invalid = || {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "通常のファイル名を指定してください。",
        )
    };
    if path.file_name().is_none() || path.as_os_str().encode_wide().any(|c| c == 0) {
        return Err(invalid());
    }
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => match prefix.kind() {
                Prefix::DeviceNS(_) | Prefix::Verbatim(_) => return Err(invalid()),
                Prefix::Disk(_) | Prefix::VerbatimDisk(_) if !path.is_absolute() => {
                    return Err(invalid());
                }
                Prefix::UNC(_, share) | Prefix::VerbatimUNC(_, share)
                    if share.eq_ignore_ascii_case("pipe") || share.eq_ignore_ascii_case("IPC$") =>
                {
                    return Err(invalid());
                }
                _ => {}
            },
            Component::Normal(name) => {
                let text = name.to_string_lossy();
                let stem = text
                    .split('.')
                    .next()
                    .unwrap_or("")
                    .trim_end_matches(' ')
                    .to_uppercase();
                let device = matches!(
                    stem.as_str(),
                    "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$" | "CONIN$" | "CONOUT$"
                ) || ["COM", "LPT"].iter().any(|prefix| {
                    stem.strip_prefix(prefix).is_some_and(|n| {
                        matches!(
                            n,
                            "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                        )
                    })
                });
                if device
                    || text.ends_with(['.', ' '])
                    || text
                        .chars()
                        .any(|c| c < ' ' || matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|'))
                {
                    return Err(invalid());
                }
            }
            _ => {}
        }
    }
    Ok(())
}

pub fn validate_extension(path: &Path, extension: &str) -> io::Result<()> {
    validate_path(path)?;
    if !path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case(extension))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(".{extension} の拡張子を指定してください。"),
        ));
    }
    Ok(())
}

struct StagedFile {
    file: Option<File>,
    path: PathBuf,
    committed: bool,
}
impl Drop for StagedFile {
    fn drop(&mut self) {
        // Close before cleanup, including when the writer returns an error or panics.
        self.file.take();
        if !self.committed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

pub fn replace_file(
    path: &Path,
    write: impl FnOnce(&mut File) -> io::Result<()>,
) -> io::Result<File> {
    validate_path(path)?;
    let security = security::Destination::read(path)?;
    let mut staged = None;
    for _ in 0..8 {
        let mut temporary_name = OsString::from(".mio-note");
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        temporary_name.push(format!(
            ".{}.{}.{}.tmp",
            std::process::id(),
            timestamp,
            NEXT_FILE.fetch_add(1, Ordering::Relaxed)
        ));
        let temporary = path.with_file_name(temporary_name);
        // Atomic exclusive creation is essential: never truncate a pre-existing
        // file, hard link or symlink at a predictable staging path.
        match security.create_new(&temporary) {
            Ok(file) => {
                staged = Some(StagedFile {
                    file: Some(file),
                    path: temporary,
                    committed: false,
                });
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    let mut staged = staged.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::AlreadyExists,
            "一時ファイルを作成できません。",
        )
    })?;
    let file = staged
        .file
        .as_mut()
        .ok_or_else(|| io::Error::other("一時ファイルがありません。"))?;
    security.check_attributes(&staged.path, file)?;
    write(file)?;
    file.sync_all()?;
    // Rust's Windows File handles share DELETE; keep our original handle open
    // across replacement so callers such as the logger never reopen by name.
    fs::rename(&staged.path, path)?;
    staged.committed = true;
    staged
        .file
        .take()
        .ok_or_else(|| io::Error::other("一時ファイルがありません。"))
}

pub fn atomic_write(
    path: &Path,
    write: impl FnOnce(&mut File) -> io::Result<()>,
) -> io::Result<()> {
    replace_file(path, write).map(drop)
}

pub const MAX_EXPORT_BYTES: u64 = 512 * 1024 * 1024;

pub struct LimitedWriter<W> {
    inner: W,
    written: u64,
    cursor: u64,
    limit: u64,
}
impl<W> LimitedWriter<W> {
    pub fn new(inner: W, limit: u64) -> Self {
        Self {
            inner,
            written: 0,
            cursor: 0,
            limit,
        }
    }
}
impl<W: Write> Write for LimitedWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() as u64 > self.limit.saturating_sub(self.written)
            || bytes.len() as u64 > self.limit.saturating_sub(self.cursor)
        {
            return Err(io::Error::other("出力データが上限を超えました。"));
        }
        let count = self.inner.write(bytes)?;
        self.written += count as u64;
        self.cursor += count as u64;
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
impl<W: io::Seek> io::Seek for LimitedWriter<W> {
    fn seek(&mut self, from: io::SeekFrom) -> io::Result<u64> {
        // Used by ZipWriter for rewriting headers; charge rewrites to the same
        // total byte budget rather than increasing the allowable output.
        let position = self.inner.seek(from)?;
        if position > self.limit {
            self.inner.seek(io::SeekFrom::Start(self.cursor))?;
            return Err(io::Error::other("出力位置が上限を超えました。"));
        }
        self.cursor = position;
        Ok(position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn directory(name: &str) -> PathBuf {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/security-storage-tests")
            .join(format!("{}-{name}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        path
    }
    #[test]
    fn replacement_breaks_links_and_cleans_partial_files_even_on_panic() {
        let dir = directory("atomic");
        let victim = dir.join("victim.txt");
        let output = dir.join("output.txt");
        fs::write(&victim, b"keep").unwrap();
        let _ = fs::remove_file(&output);
        fs::hard_link(&victim, &output).unwrap();
        atomic_write(&output, |file| file.write_all(b"replacement")).unwrap();
        assert_eq!(fs::read(&victim).unwrap(), b"keep");
        assert!(
            std::panic::catch_unwind(|| atomic_write(&output, |file| {
                file.write_all(b"partial")?;
                panic!("test panic");
            }))
            .is_err()
        );
        assert_eq!(fs::read(&output).unwrap(), b"replacement");
        assert!(!fs::read_dir(&dir).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp")
        }));
    }
    #[test]
    fn rejects_device_paths_alternate_streams_and_wrong_extensions() {
        for path in [
            "NUL.inknote",
            "COM1.txt",
            "CON .inknote",
            "CONOUT$.inknote",
            "LPT².txt",
            "note.inknote:secret",
            r"\\.\pipe\test",
            r"\\?\GLOBALROOT\Device\test",
            "note.inknote.",
            "note.inknote ",
            "bad\0.inknote",
        ] {
            assert!(validate_path(Path::new(path)).is_err(), "{path}");
        }
        assert!(validate_extension(Path::new("日本語.INKNOTE"), "inknote").is_ok());
        assert!(validate_extension(Path::new("note.exe"), "inknote").is_err());
    }
    #[test]
    fn output_budget_errors_before_writing_excess_data() {
        let mut writer = LimitedWriter::new(Vec::new(), 4);
        writer.write_all(b"1234").unwrap();
        assert!(writer.write_all(b"5").is_err());
        assert_eq!(writer.inner, b"1234");
        assert_eq!(writer.written, 4);
        use io::Seek;
        let mut writer = LimitedWriter::new(io::Cursor::new(Vec::new()), 4);
        assert!(writer.seek(io::SeekFrom::Start(5)).is_err());
        writer.seek(io::SeekFrom::Start(3)).unwrap();
        assert!(writer.write_all(b"12").is_err());
        writer.write_all(b"1").unwrap();
    }
}
