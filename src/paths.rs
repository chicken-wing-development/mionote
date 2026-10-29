//! Keep package state out of the read-only install directory and make portable
//! state explicit. Never fall back to another location after a storage error.
#[cfg(any(not(feature = "store"), test))]
use std::path::Path;
use std::{ffi::OsString, io, path::PathBuf};

#[cfg(any(not(feature = "store"), test))]
pub const PORTABLE_MARKER: &str = "mio-note.portable";
#[cfg(any(not(feature = "store"), test))]
const MARKER_CONTENT: &[u8] = b"MioNote portable v1\n";

#[cfg(feature = "store")]
pub fn data_directory() -> io::Result<PathBuf> {
    let folder = windows::Storage::ApplicationData::Current()
        .and_then(|data| data.LocalFolder())
        .and_then(|folder| folder.Path())
        .map_err(|e| io::Error::other(e.to_string()))?;
    checked_directory(PathBuf::from(folder.to_os_string()))
}

#[cfg(not(feature = "store"))]
pub fn data_directory() -> io::Result<PathBuf> {
    let executable = std::env::current_exe()?;
    if let Some(directory) = portable_directory(&executable)? {
        return checked_directory(directory);
    }
    let directory = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "LOCALAPPDATA is unavailable"))?;
    // Preserve settings for unpackaged builds without the portable marker.
    checked_directory(directory.join("InkNote"))
}

fn checked_directory(directory: PathBuf) -> io::Result<PathBuf> {
    if !directory.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Data directory must be absolute",
        ));
    }
    crate::storage::validate_path(&directory.join("settings.ini"))?;
    Ok(directory)
}

#[cfg(any(not(feature = "store"), test))]
fn portable_directory(executable: &Path) -> io::Result<Option<PathBuf>> {
    let parent = executable
        .parent()
        .ok_or_else(|| io::Error::other("Missing executable directory"))?;
    let marker = parent.join(PORTABLE_MARKER);
    let metadata = match std::fs::symlink_metadata(&marker) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if !metadata.is_file() || metadata.len() != MARKER_CONTENT.len() as u64 {
        return Err(io::Error::other("Invalid portable marker"));
    }
    // Bound the read even if another process changes the file after metadata().
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(marker)?
        .take(MARKER_CONTENT.len() as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes != MARKER_CONTENT {
        return Err(io::Error::other("Invalid portable marker"));
    }
    Ok(Some(parent.join("data")))
}

pub fn startup_note() -> io::Result<Option<PathBuf>> {
    parse_startup(std::env::args_os().skip(1))
}

fn parse_startup(mut args: impl Iterator<Item = OsString>) -> io::Result<Option<PathBuf>> {
    let Some(first) = args.next() else {
        return Ok(None);
    };
    if args.next().is_some() || first.is_empty() || first.to_string_lossy().starts_with('-') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "起動時に指定できるのは1つの.inknoteファイルです。",
        ));
    }
    let path = PathBuf::from(first);
    crate::storage::validate_extension(&path, "inknote")?;
    let path = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()?.join(path)
    };
    crate::storage::validate_extension(&path, "inknote")?;
    Ok(Some(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn portable_marker_is_explicit_and_invalid_marker_does_not_fall_back() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/path-tests")
            .join(std::process::id().to_string());
        std::fs::create_dir_all(&root).unwrap();
        let exe = root.join("MioNote.exe");
        let marker = root.join(PORTABLE_MARKER);
        let _ = std::fs::remove_file(&marker);
        assert_eq!(portable_directory(&exe).unwrap(), None);
        std::fs::write(&marker, MARKER_CONTENT).unwrap();
        assert_eq!(portable_directory(&exe).unwrap(), Some(root.join("data")));
        std::fs::write(&marker, b"invalid").unwrap();
        assert!(portable_directory(&exe).is_err());
        assert!(checked_directory(PathBuf::from("relative")).is_err());
        std::fs::remove_file(marker).unwrap();
    }
    #[test]
    fn startup_accepts_one_note_and_rejects_switches_devices_and_extra_arguments() {
        for args in [
            vec!["--something"],
            vec!["NUL.inknote"],
            vec!["a.inknote", "b.inknote"],
            vec!["a.exe"],
            vec!["note.inknote:stream"],
        ] {
            assert!(parse_startup(args.into_iter().map(OsString::from)).is_err());
        }
        assert!(parse_startup(std::iter::empty()).unwrap().is_none());
        assert!(
            parse_startup([OsString::from("日本語.inknote")].into_iter())
                .unwrap()
                .unwrap()
                .is_absolute()
        );
    }
}
