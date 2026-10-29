use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=assets/mio-note.ico");
    println!("cargo:rerun-if-changed=assets/branding/mio-note-v1.ico");
    println!("cargo:rerun-if-env-changed=RC");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is missing"));
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("Manifest path is missing"));
    let icon = root.join("assets/mio-note.ico");
    let icon = icon
        .to_str()
        .expect("Icon path must be UTF-8")
        .replace('\\', "\\\\");
    let artwork = root.join("assets/branding/mio-note-v1.ico");
    let artwork = artwork
        .to_str()
        .expect("Artwork path must be UTF-8")
        .replace('\\', "\\\\");
    let version = env::var("CARGO_PKG_VERSION").expect("Package version is missing");
    let mut parts: Vec<u16> = version
        .split('.')
        .map(|p| p.parse().expect("Invalid version"))
        .collect();
    parts.resize(4, 0);
    let numeric = parts
        .iter()
        .map(u16::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let resource = output.join("mio-note.rc");
    let compiled = output.join("mio-note.res");
    let manifest = output.join("mio-note.manifest");
    fs::write(&manifest, r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3"><security><requestedPrivileges>
    <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
  </requestedPrivileges></security></trustInfo>
  <application xmlns="urn:schemas-microsoft-com:asm.v3"><windowsSettings>
    <longPathAware xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">true</longPathAware>
  </windowsSettings></application>
</assembly>"#).expect("Could not write the Windows manifest");
    let manifest = manifest
        .to_str()
        .expect("Manifest path must be UTF-8")
        .replace('\\', "\\\\");
    fs::write(
        &resource,
        format!(
            r#"
#pragma code_page(65001)
1 ICON "{icon}"
2 ICON "{artwork}"
1 24 "{manifest}"
3 DIALOGEX 0, 0, 280, 280
// DS_SETFONT | DS_MODALFRAME | DS_CENTER | WS_POPUP | WS_CAPTION | WS_SYSMENU
STYLE 0x80c808c0
CAPTION "バージョン情報・コピーライト"
FONT 9, "Yu Gothic UI"
BEGIN
  CONTROL "", 201, "STATIC", 0x0000000d, 12, 8, 256, 168
  CTEXT "MioNote {version}", -1, 12, 184, 256, 14
  CTEXT "水筆（みお）— 軽量手書きノート", -1, 12, 204, 256, 14
  CTEXT "Copyright (c) 2026 Ink Note (mio note) contributors", -1, 8, 224, 264, 12
  CTEXT "MIT License / 発行元: テバ開発", -1, 12, 240, 256, 12
  DEFPUSHBUTTON "閉じる", 1, 106, 260, 68, 16
END
1 VERSIONINFO
FILEVERSION {numeric}
PRODUCTVERSION {numeric}
FILEFLAGSMASK 0x3fL
FILEFLAGS 0x0L
FILEOS 0x40004L
FILETYPE 0x1L
FILESUBTYPE 0x0L
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "ProductName", "mio note\0"
      VALUE "FileDescription", "mio note - 水筆（みお）\0"
      VALUE "InternalName", "mio-note\0"
      VALUE "OriginalFilename", "MioNote.exe\0"
      VALUE "FileVersion", "{version}\0"
      VALUE "ProductVersion", "{version}\0"
      VALUE "LegalCopyright", "Copyright (c) 2026 Ink Note (mio note) contributors\0"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x0409, 1200
  END
END
"#
        ),
    )
    .expect("Could not write the Windows resource script");

    let compiler = env::var_os("RC").unwrap_or_else(|| "rc.exe".into());
    let status = Command::new(compiler)
        .arg("/nologo")
        .arg("/fo")
        .arg(&compiled)
        .arg(&resource)
        .status()
        .expect("Windows SDK rc.exe is required; use Developer PowerShell or set RC to its path");
    assert!(status.success(), "Windows resource compilation failed");
    println!("cargo:rustc-link-arg-bin=mio-note={}", compiled.display());
}
