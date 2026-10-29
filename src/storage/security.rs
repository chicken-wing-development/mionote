//! Create replacement files with the existing DACL from their first byte.
//! Never weaken permissions or EFS encryption as a fallback after an error.
use std::{
    fs::{self, File},
    io,
    mem::size_of,
    os::windows::{fs::MetadataExt, io::FromRawHandle},
    path::Path,
};
use windows::{
    Win32::{
        Foundation::{ERROR_INSUFFICIENT_BUFFER, GENERIC_WRITE},
        Security::{
            ACL, DACL_SECURITY_INFORMATION, GetFileSecurityW, GetSecurityDescriptorControl,
            GetSecurityDescriptorDacl, PSECURITY_DESCRIPTOR, SE_DACL_PROTECTED,
            SECURITY_ATTRIBUTES,
        },
        Storage::FileSystem::{
            CREATE_NEW, CreateFileW, FILE_ATTRIBUTE_ENCRYPTED, FILE_ATTRIBUTE_NORMAL,
            FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        },
    },
    core::HSTRING,
};

pub(super) struct Destination {
    // u32 storage gives SECURITY_DESCRIPTOR its required alignment.
    descriptor: Vec<u32>,
    encrypted: bool,
}

impl Destination {
    pub(super) fn read(path: &Path) -> io::Result<Self> {
        let metadata = match fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(Self {
                    descriptor: Vec::new(),
                    encrypted: false,
                });
            }
            Err(error) => return Err(error),
        };
        if !metadata.is_file() || metadata.permissions().readonly() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "通常の書き込み可能なファイルを指定してください。",
            ));
        }
        let path = HSTRING::from(path.as_os_str());
        let mut needed = 0;
        // Only read the DACL; no privileges or system settings change.
        unsafe {
            let result = GetFileSecurityW(&path, DACL_SECURITY_INFORMATION.0, None, 0, &mut needed);
            let error = io::Error::last_os_error();
            if result.as_bool()
                || error.raw_os_error() != Some(ERROR_INSUFFICIENT_BUFFER.0 as i32)
                || !(20..=128 * 1024).contains(&needed)
            {
                return Err(io::Error::other(
                    "保存先のアクセス権を安全に取得できません。",
                ));
            }
            let mut descriptor = vec![0u32; (needed as usize).div_ceil(4)];
            GetFileSecurityW(
                &path,
                DACL_SECURITY_INFORMATION.0,
                Some(PSECURITY_DESCRIPTOR(descriptor.as_mut_ptr().cast())),
                needed,
                &mut needed,
            )
            .ok()
            .map_err(win_error)?;
            Ok(Self {
                descriptor,
                encrypted: metadata.file_attributes() & FILE_ATTRIBUTE_ENCRYPTED.0 != 0,
            })
        }
    }

    pub(super) fn create_new(&self, path: &Path) -> io::Result<File> {
        let path = HSTRING::from(path.as_os_str());
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.descriptor.as_ptr().cast_mut().cast(),
            bInheritHandle: false.into(),
        };
        let flags = if self.encrypted {
            FILE_ATTRIBUTE_ENCRYPTED
        } else {
            FILE_ATTRIBUTE_NORMAL
        };
        // Pointers stay valid through this synchronous call. CREATE_NEW never
        // opens/truncates an existing file, link or device at the staging name.
        let handle = unsafe {
            CreateFileW(
                &path,
                GENERIC_WRITE.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                (!self.descriptor.is_empty()).then_some(&attributes as *const _),
                CREATE_NEW,
                flags,
                None,
            )
        }
        .map_err(win_error)?;
        // The owned File closes exactly the handle returned above.
        Ok(unsafe { File::from_raw_handle(handle.0) })
    }

    pub(super) fn check_attributes(&self, path: &Path, file: &File) -> io::Result<()> {
        if self.encrypted && file.metadata()?.file_attributes() & FILE_ATTRIBUTE_ENCRYPTED.0 == 0 {
            return Err(io::Error::other(
                "保存先の暗号化属性を保持できません。元のファイルを保持します。",
            ));
        }
        // EFS or filesystem inheritance can affect a newly created file. Read
        // the actual DACL back before any private content is written.
        if !self.descriptor.is_empty()
            && dacl(&Self::read(path)?.descriptor)? != dacl(&self.descriptor)?
        {
            return Err(io::Error::other(
                "保存先のアクセス権を保持できません。元のファイルを保持します。",
            ));
        }
        Ok(())
    }
}

// Compare the ACL and its inheritance protection, not unrelated owner/default
// flags in the SECURITY_DESCRIPTOR header, which Windows may normalize.
fn dacl(descriptor: &[u32]) -> io::Result<(bool, bool, Option<Vec<u8>>)> {
    let security = PSECURITY_DESCRIPTOR(descriptor.as_ptr().cast_mut().cast());
    let mut control = 0;
    let mut revision = 0;
    let mut present = windows::core::BOOL::default();
    let mut defaulted = windows::core::BOOL::default();
    let mut acl = std::ptr::null_mut::<ACL>();
    unsafe {
        // The aligned buffer comes from GetFileSecurityW and remains alive.
        GetSecurityDescriptorControl(security, &mut control, &mut revision).map_err(win_error)?;
        GetSecurityDescriptorDacl(security, &mut present, &mut acl, &mut defaulted)
            .map_err(win_error)?;
    }
    let protected = control & SE_DACL_PROTECTED.0 != 0;
    if acl.is_null() {
        return Ok((protected, present.as_bool(), None));
    }
    let start = descriptor.as_ptr() as usize;
    let end = start
        .checked_add(std::mem::size_of_val(descriptor))
        .ok_or_else(|| io::Error::other("Invalid security descriptor size"))?;
    let address = acl as usize;
    if address < start
        || address
            .checked_add(size_of::<ACL>())
            .is_none_or(|n| n > end)
    {
        return Err(io::Error::other("Invalid DACL location"));
    }
    // Header and full ACL both lie within the bounds checked above/below.
    let length = unsafe { (*acl).AclSize as usize };
    if length < size_of::<ACL>() || address.checked_add(length).is_none_or(|n| n > end) {
        return Err(io::Error::other("Invalid DACL length"));
    }
    let bytes = unsafe { std::slice::from_raw_parts(acl.cast::<u8>(), length) }.to_vec();
    Ok((protected, present.as_bool(), Some(bytes)))
}
fn win_error(error: windows::core::Error) -> io::Error {
    io::Error::from_raw_os_error(error.code().0 & 0xffff)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use windows::{
        Win32::{
            Foundation::{HLOCAL, LocalFree},
            Security::{
                Authorization::{
                    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
                },
                PROTECTED_DACL_SECURITY_INFORMATION, SetFileSecurityW,
            },
        },
        core::w,
    };

    #[test]
    fn replacement_and_staging_keep_the_existing_private_dacl() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/security-acl-tests")
            .join(std::process::id().to_string());
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("private.txt");
        fs::write(&path, b"private").unwrap();
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                w!("D:P(A;;FA;;;OW)"),
                SDDL_REVISION_1,
                &mut descriptor,
                None,
            )
            .unwrap();
            let applied = SetFileSecurityW(
                &HSTRING::from(path.as_os_str()),
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                descriptor,
            )
            .ok();
            let _ = LocalFree(Some(HLOCAL(descriptor.0)));
            applied.unwrap();
        }
        let before = Destination::read(&path).unwrap().descriptor;
        crate::storage::atomic_write(&path, |file| {
            let staged = fs::read_dir(&dir)?
                .filter_map(Result::ok)
                .find(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".mio-note.")
                })
                .unwrap()
                .path();
            assert_eq!(
                Destination::read(&staged)?.descriptor,
                before,
                "staging ACL changed"
            );
            file.write_all(b"replacement")
        })
        .unwrap();
        assert_eq!(
            Destination::read(&path).unwrap().descriptor,
            before,
            "replacement ACL changed"
        );
        assert_eq!(fs::read(path).unwrap(), b"replacement");
    }
}
