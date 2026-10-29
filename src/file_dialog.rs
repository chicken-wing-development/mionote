use crate::export::Kind;
use std::{
    ffi::OsString,
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};
use windows::{
    Win32::{
        Foundation::{E_FAIL, HWND},
        UI::Controls::Dialogs::*,
    },
    core::{Error, PWSTR, Result, w},
};

pub fn choose(window: HWND, save: bool, current: Option<&Path>) -> Result<Option<PathBuf>> {
    choose_format(window, save, current, None)
}

pub fn export(window: HWND, current: Option<&Path>, kind: Kind) -> Result<Option<PathBuf>> {
    let initial = current.map(|p| p.with_extension(kind.extension()));
    choose_format(window, true, initial.as_deref(), Some(kind))
}

fn choose_format(
    window: HWND,
    save: bool,
    current: Option<&Path>,
    kind: Option<Kind>,
) -> Result<Option<PathBuf>> {
    let default_name = format!("無題.{}", kind.map_or("inknote", Kind::extension));
    let initial = current
        .map(|p| p.as_os_str())
        .unwrap_or(std::ffi::OsStr::new(&default_name));
    let mut buffer = vec![0u16; 32_768];
    if save {
        let name: Vec<_> = initial.encode_wide().collect();
        if name.len() < buffer.len() {
            buffer[..name.len()].copy_from_slice(&name);
        }
    }
    let mut options = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: window,
        lpstrFilter: match kind {
            Some(Kind::Pdf) => w!("PDF (*.pdf)\0*.pdf\0\0"),
            Some(Kind::Png) => w!("PNG (*.png)\0*.png\0\0"),
            Some(Kind::Svg) => w!("SVG (*.svg)\0*.svg\0\0"),
            Some(Kind::PngZip) => w!("PNG全ページ (*.zip)\0*.zip\0\0"),
            Some(Kind::SvgZip) => w!("SVG全ページ (*.zip)\0*.zip\0\0"),
            None => w!("mio note (*.inknote)\0*.inknote\0\0"),
        },
        nFilterIndex: 1,
        lpstrFile: PWSTR(buffer.as_mut_ptr()),
        nMaxFile: buffer.len() as u32,
        lpstrDefExt: match kind {
            Some(Kind::Pdf) => w!("pdf"),
            Some(Kind::Png) => w!("png"),
            Some(Kind::Svg) => w!("svg"),
            Some(Kind::PngZip | Kind::SvgZip) => w!("zip"),
            None => w!("inknote"),
        },
        lpstrTitle: match kind {
            Some(Kind::Pdf) => w!("全ページをPDFに出力"),
            Some(Kind::Png) => w!("現在のページをPNGに出力"),
            Some(Kind::Svg) => w!("現在のページをSVGに出力"),
            Some(Kind::PngZip) => w!("全ページのPNGをZIPに出力"),
            Some(Kind::SvgZip) => w!("全ページのSVGをZIPに出力"),
            None if save => w!("ノートを保存"),
            None => w!("ノートを開く"),
        },
        Flags: OFN_EXPLORER
            | OFN_NOCHANGEDIR
            | OFN_PATHMUSTEXIST
            | OFN_HIDEREADONLY
            | if save {
                OFN_OVERWRITEPROMPT
            } else {
                OFN_FILEMUSTEXIST
            },
        ..Default::default()
    };
    unsafe {
        let success = if save {
            GetSaveFileNameW(&mut options)
        } else {
            GetOpenFileNameW(&mut options)
        };
        let error = if success.as_bool() {
            0
        } else {
            CommDlgExtendedError().0
        };
        crate::ui::restore_keyboard_focus(window);
        if !success.as_bool() {
            return if error == 0 {
                Ok(None)
            } else {
                Err(Error::new(
                    E_FAIL,
                    format!("ファイル選択エラー: 0x{error:x}"),
                ))
            };
        }
    }
    let length = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    let path = PathBuf::from(OsString::from_wide(&buffer[..length]));
    crate::storage::validate_extension(&path, kind.map_or("inknote", Kind::extension))
        .map_err(|error| Error::new(E_FAIL, error.to_string()))?;
    Ok(Some(path))
}
