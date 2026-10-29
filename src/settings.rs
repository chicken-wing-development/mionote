use std::{
    fs,
    io::{self, Read, Write},
    path::PathBuf,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PenWidth {
    #[default]
    Thin,
    Medium,
    Thick,
}

impl PenWidth {
    pub fn size(self) -> f32 {
        match self {
            Self::Thin => 1.2,
            Self::Medium => 1.8,
            Self::Thick => 2.4,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PenSettings {
    pub width: PenWidth,
    pub pressure: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleSpacing {
    Narrow,
    #[default]
    Standard,
    Wide,
}

impl RuleSpacing {
    pub fn size(self) -> f32 {
        match self {
            Self::Narrow => 28.0,
            Self::Standard => 36.0,
            Self::Wide => 44.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Preferences {
    pub pen: PenSettings,
    pub spacing: RuleSpacing,
}

impl Preferences {
    fn encode(self) -> String {
        let width = match self.pen.width {
            PenWidth::Thin => "thin",
            PenWidth::Medium => "medium",
            PenWidth::Thick => "thick",
        };
        let spacing = match self.spacing {
            RuleSpacing::Narrow => "narrow",
            RuleSpacing::Standard => "standard",
            RuleSpacing::Wide => "wide",
        };
        format!(
            "pen_width={width}\npressure={}\nrule_spacing={spacing}\n",
            self.pen.pressure
        )
    }

    fn decode(text: &str) -> io::Result<Self> {
        let invalid = || io::Error::new(io::ErrorKind::InvalidData, "Invalid preference value");
        let mut preferences = Self::default();
        for line in text.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (key, value) = line.split_once('=').ok_or_else(invalid)?;
            match key.trim() {
                "pen_width" => {
                    preferences.pen.width = match value.trim() {
                        "thin" => PenWidth::Thin,
                        "medium" => PenWidth::Medium,
                        "thick" => PenWidth::Thick,
                        _ => return Err(invalid()),
                    }
                }
                "pressure" => {
                    preferences.pen.pressure = match value.trim() {
                        "true" => true,
                        "false" => false,
                        _ => return Err(invalid()),
                    }
                }
                "rule_spacing" => {
                    preferences.spacing = match value.trim() {
                        "narrow" => RuleSpacing::Narrow,
                        "standard" => RuleSpacing::Standard,
                        "wide" => RuleSpacing::Wide,
                        _ => return Err(invalid()),
                    }
                }
                // Ignore future keys; omitted settings retain their defaults.
                _ => {}
            }
        }
        Ok(preferences)
    }
}

#[derive(Clone)]
pub struct SettingsStore {
    path: PathBuf,
}

mod writer;
pub use writer::SettingsWriter;

impl SettingsStore {
    pub fn for_user() -> io::Result<Self> {
        let path = crate::paths::data_directory()?.join("settings.ini");
        crate::storage::validate_path(&path)?;
        Ok(Self { path })
    }

    pub fn load(&self) -> io::Result<Preferences> {
        crate::storage::validate_path(&self.path)?;
        let file = match fs::File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(Preferences::default());
            }
            Err(error) => return Err(error),
        };
        let mut text = String::new();
        file.take(4097).read_to_string(&mut text)?;
        if text.len() > 4096 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Settings file is too large",
            ));
        }
        Preferences::decode(&text)
    }

    pub fn save(&self, preferences: Preferences) -> io::Result<()> {
        crate::storage::validate_path(&self.path)?;
        let directory = self.path.parent().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Missing settings directory")
        })?;
        fs::create_dir_all(directory)?;
        crate::storage::atomic_write(&self.path, |file| {
            file.write_all(preferences.encode().as_bytes())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    pub(super) fn lock(store: &SettingsStore) -> fs::File {
        use std::os::windows::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&store.path)
            .unwrap()
    }

    pub(super) fn store() -> SettingsStore {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/settings-tests")
            .join(format!(
                "{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ))
            .join("日本語の設定/settings.ini");
        SettingsStore { path }
    }

    pub(super) fn clean(store: &SettingsStore) {
        let directory = store.path.parent().unwrap();
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/settings-tests")
            .canonicalize()
            .unwrap();
        assert!(directory.canonicalize().unwrap().starts_with(root));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn saves_and_reloads_all_choices_replacing_previous_file() {
        let store = store();
        assert_eq!(store.load().unwrap(), Preferences::default());
        for width in [PenWidth::Thin, PenWidth::Medium, PenWidth::Thick] {
            for pressure in [false, true] {
                for spacing in [
                    RuleSpacing::Narrow,
                    RuleSpacing::Standard,
                    RuleSpacing::Wide,
                ] {
                    let preferences = Preferences {
                        pen: PenSettings { width, pressure },
                        spacing,
                    };
                    store.save(preferences).unwrap();
                    // A fresh store represents a subsequent process launch.
                    let reopened = SettingsStore {
                        path: store.path.clone(),
                    };
                    assert_eq!(reopened.load().unwrap(), preferences);
                }
            }
        }
        clean(&store);
    }

    #[test]
    fn rejects_corrupt_or_oversized_files_and_tolerates_new_keys() {
        let store = store();
        store.save(Preferences::default()).unwrap();
        for invalid in [
            "pen_width=huge",
            "pressure=yes",
            "rule_spacing=0",
            "broken",
            &"x".repeat(4097),
        ] {
            fs::write(&store.path, invalid).unwrap();
            assert_eq!(store.load().unwrap_err().kind(), io::ErrorKind::InvalidData);
        }
        fs::write(
            &store.path,
            "# settings\r\npressure=true\r\nfuture_key=value\r\n",
        )
        .unwrap();
        assert_eq!(
            store.load().unwrap(),
            Preferences {
                pen: PenSettings {
                    pressure: true,
                    ..Default::default()
                },
                ..Default::default()
            }
        );
        clean(&store);
    }

    #[test]
    fn failed_write_preserves_previous_settings() {
        let store = store();
        let previous = Preferences {
            spacing: RuleSpacing::Wide,
            ..Default::default()
        };
        store.save(previous).unwrap();
        let locked = lock(&store);
        assert!(store.save(Preferences::default()).is_err());
        drop(locked);
        assert_eq!(store.load().unwrap(), previous);
        clean(&store);
    }

    #[test]
    fn predictable_legacy_staging_hardlink_cannot_overwrite_another_file() {
        let store = store();
        store.save(Preferences::default()).unwrap();
        let victim = store.path.with_file_name("victim.txt");
        let legacy = store
            .path
            .with_extension(format!("{}.tmp", std::process::id()));
        fs::write(&victim, b"private data").unwrap();
        fs::hard_link(&victim, &legacy).unwrap();
        store
            .save(Preferences {
                spacing: RuleSpacing::Wide,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(fs::read(&victim).unwrap(), b"private data");
        assert_eq!(store.load().unwrap().spacing, RuleSpacing::Wide);
        clean(&store);
    }
}
