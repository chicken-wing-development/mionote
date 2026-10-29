//! The original artwork is loaded only when this native modal dialog opens.

use super::*;

pub(super) fn show(owner: HWND) -> Result<()> {
    struct Artwork(HICON);
    impl Drop for Artwork {
        fn drop(&mut self) {
            unsafe {
                let _ = DestroyIcon(self.0);
            }
        }
    }

    unsafe {
        let instance = HINSTANCE(GetModuleHandleW(None)?.0);
        let artwork = Artwork(HICON(
            LoadImageW(
                Some(instance),
                PCWSTR(std::ptr::without_provenance(2)),
                IMAGE_ICON,
                256,
                256,
                IMAGE_FLAGS::default(),
            )?
            .0,
        ));
        // The resource and callback parameter outlive the modal message loop.
        let result = DialogBoxParamW(
            Some(instance),
            PCWSTR(std::ptr::without_provenance(3)),
            Some(owner),
            Some(dialog_proc),
            LPARAM(artwork.0.0 as isize),
        );
        let result = if result == -1 {
            Err(Error::from_thread())
        } else {
            Ok(())
        };
        super::restore_keyboard_focus(owner);
        result?;
    }
    Ok(())
}

unsafe extern "system" fn dialog_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> isize {
    unsafe {
        match message {
            WM_INITDIALOG => {
                SetWindowLongPtrW(window, GWLP_USERDATA, lparam.0);
                1
            }
            WM_DRAWITEM if lparam.0 != 0 => {
                let item = &*(lparam.0 as *const DRAWITEMSTRUCT);
                if item.CtlID != 201 {
                    return 0;
                }
                FillRect(item.hDC, &item.rcItem, GetSysColorBrush(COLOR_3DFACE));
                let width = item.rcItem.right - item.rcItem.left;
                let height = item.rcItem.bottom - item.rcItem.top;
                let size = width.min(height).max(1);
                let artwork = HICON(GetWindowLongPtrW(window, GWLP_USERDATA) as _);
                let _ = DrawIconEx(
                    item.hDC,
                    item.rcItem.left + (width - size) / 2,
                    item.rcItem.top + (height - size) / 2,
                    artwork,
                    size,
                    size,
                    0,
                    None,
                    DI_NORMAL,
                );
                1
            }
            WM_COMMAND if matches!(wparam.0 & 0xffff, 1 | 2) => {
                let _ = EndDialog(window, (wparam.0 & 0xffff) as isize);
                1
            }
            WM_CLOSE => {
                let _ = EndDialog(window, 0);
                1
            }
            _ => 0,
        }
    }
}
