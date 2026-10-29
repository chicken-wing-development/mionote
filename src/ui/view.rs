use super::*;
use windows::Win32::{Foundation::POINT, UI::Controls::SetScrollInfo};

fn available(app: &App) -> bool {
    app.ready.get() && !app.busy.get() && !app.closing.get()
}

fn toolbar_height(width: i32, dpi: f32) -> i32 {
    ((if width as f32 / dpi < 800.0 {
        TOOLBAR_HEIGHT + 44.0
    } else {
        TOOLBAR_HEIGHT
    }) * dpi)
        .round() as i32
}

unsafe fn geometry(window: HWND, app: &App, state: State) -> Result<Layout> {
    unsafe {
        let dpi = GetDpiForWindow(window);
        let scale = dpi as f32 / 96.0;
        let mut bounds = RECT::default();
        GetClientRect(window, &mut bounds)?;
        Ok(Layout::new(
            app.paper.get(),
            (
                bounds.right,
                bounds.bottom - toolbar_height(bounds.right, scale),
            ),
            scale,
            (12.0 * scale).round() as i32,
            (
                GetSystemMetricsForDpi(SM_CXVSCROLL, dpi),
                GetSystemMetricsForDpi(SM_CYHSCROLL, dpi),
            ),
            state,
        ))
    }
}

pub(super) unsafe fn create(window: HWND, instance: HINSTANCE, app: &App) -> Result<()> {
    unsafe {
        let create = |class, text, parent, style| {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class,
                text,
                style,
                0,
                0,
                1,
                1,
                Some(parent),
                None,
                Some(instance),
                Some((app as *const App).cast()),
            )
        };
        app.viewport.set(create(
            w!("RustMioNoteViewport"),
            w!("ページの表示領域"),
            window,
            WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN,
        )?);
        app.page.set(create(
            w!("RustMioNotePage"),
            w!("ノート"),
            app.viewport.get(),
            WS_CHILD | WS_VISIBLE,
        )?);
        app.scrollbars.set([
            create(
                w!("SCROLLBAR"),
                w!("横スクロール"),
                window,
                WS_CHILD | WINDOW_STYLE(SBS_HORZ as u32),
            )?,
            create(
                w!("SCROLLBAR"),
                w!("縦スクロール"),
                window,
                WS_CHILD | WINDOW_STYLE(SBS_VERT as u32),
            )?,
        ]);
        layout(window, app)
    }
}

pub(super) unsafe fn create_controls(window: HWND, instance: HINSTANCE, app: &App) -> Result<()> {
    unsafe {
        let style = WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32);
        let button = |text, id| create_control(window, instance, w!("BUTTON"), text, id, style);
        app.view_controls.set([
            button(w!("縮小"), ZOOM_OUT)?,
            button(w!("拡大"), ZOOM_IN)?,
            button(w!("横幅に合わせる"), FIT_WIDTH)?,
            button(w!("全体を表示"), FIT_PAGE)?,
        ]);
        app.zoom_button.set(create_control(
            window,
            instance,
            w!("BUTTON"),
            w!("100%に戻す"),
            RESET_ZOOM,
            style,
        )?);
        Ok(())
    }
}

pub(super) unsafe fn layout_controls(window: HWND, app: &App) -> Result<()> {
    unsafe {
        let dpi = GetDpiForWindow(window) as f32 / 96.0;
        let px = |n: f32| (n * dpi).round() as i32;
        let mut bounds = RECT::default();
        GetClientRect(window, &mut bounds)?;
        let compact = bounds.right as f32 / dpi < 800.0;
        let (x, y) = if compact {
            (12.0, 142.0)
        } else {
            (516.0, 10.0)
        };
        for (control, offset, width) in [
            (app.view_controls.get()[0], 0.0, 32.0),
            (app.view_controls.get()[1], 92.0, 32.0),
            (app.view_controls.get()[2], 132.0, 68.0),
            (app.view_controls.get()[3], 204.0, 68.0),
        ] {
            MoveWindow(control, px(x + offset), px(y), px(width), px(36.0), true)?;
        }
        MoveWindow(
            app.zoom_button.get(),
            px(x + 36.0),
            px(y),
            px(52.0),
            px(36.0),
            true,
        )?;
        if let Some(view) = app.view_layout.get() {
            update_zoom_button(app, view)?;
        }
        Ok(())
    }
}

pub(super) unsafe fn layout(window: HWND, app: &App) -> Result<()> {
    unsafe {
        let view = geometry(window, app, app.view.get())?;
        let mut bounds = RECT::default();
        GetClientRect(window, &mut bounds)?;
        let top = toolbar_height(bounds.right, view.dpi);
        MoveWindow(
            app.viewport.get(),
            0,
            top,
            view.client.0,
            view.client.1,
            true,
        )?;
        let bars = app.scrollbars.get();
        MoveWindow(
            bars[0],
            0,
            top + view.client.1,
            view.client.0,
            (view.outer.1 - view.client.1).max(1),
            true,
        )?;
        MoveWindow(
            bars[1],
            view.client.0,
            top,
            (view.outer.0 - view.client.0).max(1),
            view.client.1,
            true,
        )?;
        let _ = ShowWindow(bars[0], if view.bars.0 { SW_SHOWNA } else { SW_HIDE });
        let _ = ShowWindow(bars[1], if view.bars.1 { SW_SHOWNA } else { SW_HIDE });
        apply(app, view)
    }
}

unsafe fn apply(app: &App, view: Layout) -> Result<()> {
    unsafe {
        let old = app.view_layout.replace(Some(view));
        let mut state = app.view.get();
        state.scroll = view.scroll;
        app.view.set(state);
        app.render_scale.set(view.scale);
        let size_changed = old.is_none_or(|old| old.page != view.page || old.scale != view.scale);
        SetWindowPos(
            app.page.get(),
            None,
            view.origin.0,
            view.origin.1,
            view.page.0 as i32,
            view.page.1 as i32,
            SWP_NOZORDER
                | SWP_NOACTIVATE
                | if size_changed {
                    SWP_NOCOPYBITS
                } else {
                    SWP_NOSIZE
                },
        )?;
        for (bar, extent, client, pos) in [
            (
                app.scrollbars.get()[0],
                view.extent.0,
                view.client.0,
                view.scroll.0,
            ),
            (
                app.scrollbars.get()[1],
                view.extent.1,
                view.client.1,
                view.scroll.1,
            ),
        ] {
            SetScrollInfo(
                bar,
                SB_CTL,
                &SCROLLINFO {
                    cbSize: size_of::<SCROLLINFO>() as u32,
                    fMask: SIF_RANGE | SIF_PAGE | SIF_POS,
                    nMin: 0,
                    nMax: extent - 1,
                    nPage: client as u32,
                    nPos: pos,
                    ..Default::default()
                },
                true,
            );
        }
        if size_changed {
            if let Some(ink) = app.ink.get() {
                ink.resize(view.page.0, view.page.1, view.scale)?;
            }
            if !app.zoom_button.get().is_invalid() {
                update_zoom_button(app, view)?;
            }
            let _ = InvalidateRect(Some(app.page.get()), None, false);
        }
        Ok(())
    }
}

unsafe fn update_zoom_button(app: &App, view: Layout) -> Result<()> {
    unsafe {
        SetWindowTextW(
            app.zoom_button.get(),
            &HSTRING::from(format!("100%に戻す（現在 {:.0}%）", view.zoom() * 100.0)),
        )?;
        let _ = InvalidateRect(Some(app.zoom_button.get()), None, false);
        Ok(())
    }
}

pub(super) fn command(
    window: HWND,
    app: &App,
    id: usize,
    anchor: Option<(i32, i32)>,
) -> Result<()> {
    if !available(app) {
        return Ok(());
    }
    let Some(old) = app.view_layout.get() else {
        return Ok(());
    };
    let mut state = app.view.get();
    state.mode = match id {
        FIT_WIDTH => Mode::Width,
        FIT_PAGE => Mode::Page,
        RESET_ZOOM => Mode::Zoom(1.0),
        ZOOM_IN | ZOOM_OUT => {
            let zoom = old.zoom() * if id == ZOOM_IN { 1.2 } else { 1.0 / 1.2 };
            Mode::Zoom(zoom.clamp(crate::view::MIN_ZOOM, crate::view::MAX_ZOOM))
        }
        _ => return Ok(()),
    };
    unsafe {
        if matches!(id, FIT_WIDTH | FIT_PAGE) {
            state.scroll = (0, 0);
        } else {
            let anchor = anchor.unwrap_or((old.client.0 / 2, old.client.1 / 2));
            let point = old.point(anchor);
            state.scroll = geometry(window, app, state)?.anchored_scroll(point, anchor);
        }
        app.view.set(state);
        layout(window, app)?;
        for control in app
            .view_controls
            .get()
            .into_iter()
            .chain([app.zoom_button.get()])
        {
            let _ = InvalidateRect(Some(control), None, false);
        }
    }
    Ok(())
}

fn scroll(window: HWND, app: &App, delta: (i32, i32)) -> Result<()> {
    if !available(app) {
        return Ok(());
    }
    let mut state = app.view.get();
    state.scroll = (
        state.scroll.0.saturating_add(delta.0),
        state.scroll.1.saturating_add(delta.1),
    );
    app.view.set(state);
    // No toolbar layout, stroke transform or Ink queue item is needed to pan.
    unsafe { apply(app, geometry(window, app, state)?) }
}

pub(super) fn scrollbar(window: HWND, app: &App, message: u32, wparam: WPARAM) -> Result<()> {
    if !available(app) {
        return Ok(());
    }
    let axis = usize::from(message == WM_VSCROLL);
    let Some(view) = app.view_layout.get() else {
        return Ok(());
    };
    let (current, page, extent) = if axis == 0 {
        (view.scroll.0, view.client.0, view.extent.0)
    } else {
        (view.scroll.1, view.client.1, view.extent.1)
    };
    let line = (32.0 * view.dpi).round() as i32;
    let target = match SCROLLBAR_COMMAND((wparam.0 & 0xffff) as i32) {
        SB_LINEUP => current - line,
        SB_LINEDOWN => current + line,
        SB_PAGEUP => current - (page - line).max(line),
        SB_PAGEDOWN => current + (page - line).max(line),
        SB_TOP => 0,
        SB_BOTTOM => extent - page,
        SB_THUMBTRACK | SB_THUMBPOSITION => {
            let mut info = SCROLLINFO {
                cbSize: size_of::<SCROLLINFO>() as u32,
                fMask: SIF_TRACKPOS,
                ..Default::default()
            };
            unsafe {
                GetScrollInfo(app.scrollbars.get()[axis], SB_CTL, &mut info)?;
            }
            info.nTrackPos
        }
        _ => return Ok(()),
    };
    scroll(
        window,
        app,
        if axis == 0 {
            (target - current, 0)
        } else {
            (0, target - current)
        },
    )
}

pub(super) fn key(window: HWND, app: &App, key: usize) -> Result<bool> {
    let Some(view) = app.view_layout.get() else {
        return Ok(false);
    };
    let delta = match key {
        0x21 => -(view.client.1 - (32.0 * view.dpi) as i32).max(1),
        0x22 => (view.client.1 - (32.0 * view.dpi) as i32).max(1),
        0x24 => -view.scroll.1,
        0x23 => view.extent.1 - view.client.1 - view.scroll.1,
        _ => return Ok(false),
    };
    scroll(window, app, (0, delta))?;
    Ok(true)
}

pub(super) fn wheel(
    window: HWND,
    app: &App,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> Result<()> {
    if !available(app) {
        return Ok(());
    }
    let Some(view) = app.view_layout.get() else {
        return Ok(());
    };
    unsafe {
        let zoom = GetKeyState(VK_CONTROL.0 as i32) < 0;
        let horizontal = message == WM_MOUSEHWHEEL || GetKeyState(VK_SHIFT.0 as i32) < 0;
        let axis = if zoom { 2 } else { usize::from(!horizontal) };
        let delta = ((wparam.0 >> 16) as u16 as i16) as i32;
        let mut remainders = app.wheel_remainder.get();
        remainders[axis] += delta;
        let steps = remainders[axis] / WHEEL_DELTA as i32;
        remainders[axis] %= WHEEL_DELTA as i32;
        app.wheel_remainder.set(remainders);
        if steps == 0 {
            return Ok(());
        }
        if zoom {
            let mut point = POINT {
                x: lparam.0 as u16 as i16 as i32,
                y: (lparam.0 >> 16) as u16 as i16 as i32,
            };
            let _ = ScreenToClient(app.viewport.get(), &mut point);
            let anchor = (
                point.x.clamp(0, view.client.0),
                point.y.clamp(0, view.client.1),
            );
            for _ in 0..steps.unsigned_abs().min(20) {
                command(
                    window,
                    app,
                    if steps > 0 { ZOOM_IN } else { ZOOM_OUT },
                    Some(anchor),
                )?;
            }
        } else {
            let mut lines = 3u32;
            let action = if horizontal {
                SPI_GETWHEELSCROLLCHARS
            } else {
                SPI_GETWHEELSCROLLLINES
            };
            let _ = SystemParametersInfoW(
                action,
                0,
                Some((&mut lines as *mut u32).cast()),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS::default(),
            );
            let page = if horizontal {
                view.client.0
            } else {
                view.client.1
            };
            let distance = if lines == u32::MAX {
                page
            } else {
                (lines.min(100) as f32 * 32.0 * view.dpi).round() as i32
            };
            let direction = if message == WM_MOUSEHWHEEL { 1 } else { -1 };
            let pixels = steps.saturating_mul(distance).saturating_mul(direction);
            scroll(
                window,
                app,
                if horizontal { (pixels, 0) } else { (0, pixels) },
            )?;
        }
    }
    Ok(())
}

pub(super) unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if message == WM_NCCREATE {
            let create = &*(lparam.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize);
        }
        let ptr = GetWindowLongPtrW(window, GWLP_USERDATA) as *const App;
        if !ptr.is_null() && matches!(message, WM_MOUSEWHEEL | WM_MOUSEHWHEEL) {
            if let Err(error) = wheel(GetAncestor(window, GA_ROOT), &*ptr, message, wparam, lparam)
            {
                crate::log!("viewport wheel error: {error}");
            }
            return LRESULT(0);
        }
        if message == WM_ERASEBKGND {
            let mut rect = RECT::default();
            let _ = GetClientRect(window, &mut rect);
            let brush = CreateSolidBrush(rgb(235, 238, 241));
            FillRect(HDC(wparam.0 as _), &rect, brush);
            let _ = DeleteObject(brush.into());
            return LRESULT(1);
        }
        DefWindowProcW(window, message, wparam, lparam)
    }
}
