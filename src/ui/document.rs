use super::*;
use crate::export::Kind;
use crate::file_dialog;

#[derive(Clone, Copy)]
pub(super) enum Action {
    Open,
    Close,
}

pub(super) fn changed(window: HWND, app: &App) {
    app.revision.set(app.revision.get().wrapping_add(1));
    app.dirty.set(true);
    title(window, app);
}

fn title(window: HWND, app: &App) {
    let path = app.path.borrow();
    let name = path
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy())
        .unwrap_or_else(|| "無題".into());
    let title = HSTRING::from(format!(
        "{name}{} — {}/{}ページ · {}画 · mio note {}",
        if app.dirty.get() { " *" } else { "" },
        app.page_index.get() + 1,
        app.page_total.get(),
        app.stroke_count.get(),
        env!("CARGO_PKG_VERSION")
    ));
    unsafe {
        let _ = SetWindowTextW(window, &title);
    }
}

fn status(app: &App, text: &str) {
    unsafe {
        let _ = SetWindowTextW(app.label.get(), &HSTRING::from(text));
    }
}

fn busy(app: &App, value: bool) {
    app.busy.set(value);
    unsafe {
        enable_ink_controls(app, !value);
    }
}

pub(super) fn error(window: HWND, app: &App, text: impl std::fmt::Display) {
    let text: String = text.to_string().chars().take(2048).collect();
    crate::log!("note error: {text}");
    app.pending.set(None);
    busy(app, false);
    status(app, "ノートの処理に失敗しました。");
    unsafe {
        MessageBoxW(
            Some(window),
            &HSTRING::from(format!("ノートを処理できませんでした。\n\n{text}")),
            w!("mio note"),
            MB_OK | MB_ICONERROR,
        );
    }
}

pub(super) fn command(window: HWND, app: &App, command: usize) {
    if app.busy.get() || !app.ready.get() || app.closing.get() {
        return;
    }
    if command == OPEN {
        request(window, app, Action::Open);
    } else if command == EXPORT {
        if let Err(err) = export_menu(window, app) {
            error(window, app, err);
        }
    } else if let Some(kind) = export_kind(command) {
        if let Err(err) = export(window, app, kind) {
            error(window, app, err);
        }
    } else if matches!(command, SAVE | SAVE_AS)
        && let Err(err) = save(window, app, command == SAVE_AS)
    {
        error(window, app, err);
    }
}

// false means that the user cancelled the filename dialog.
fn save(window: HWND, app: &App, save_as: bool) -> Result<bool> {
    let current = app.path.borrow().clone();
    let path = if save_as || current.is_none() {
        file_dialog::choose(window, true, current.as_deref())?
    } else {
        current
    };
    let Some(path) = path else {
        return Ok(false);
    };
    {
        app.saved_revision.set(app.revision.get());
        busy(app, true);
        status(app, "保存中…");
        if let Some(ink) = app.ink.get() {
            ink.save(path)?;
        }
    }
    Ok(true)
}

fn export_kind(command: usize) -> Option<Kind> {
    match command {
        EXPORT_PDF => Some(Kind::Pdf),
        EXPORT_PNG => Some(Kind::Png),
        EXPORT_SVG => Some(Kind::Svg),
        EXPORT_PNG_ZIP => Some(Kind::PngZip),
        EXPORT_SVG_ZIP => Some(Kind::SvgZip),
        _ => None,
    }
}

fn export_menu(window: HWND, app: &App) -> Result<()> {
    struct Menu(HMENU);
    impl Drop for Menu {
        fn drop(&mut self) {
            unsafe {
                let _ = DestroyMenu(self.0);
            }
        }
    }
    unsafe {
        let menu = Menu(CreatePopupMenu()?);
        for (id, label) in [
            (EXPORT_PDF, w!("PDF（全ページ）  Ctrl+E")),
            (EXPORT_PNG, w!("PNG（現在のページ・300dpi）")),
            (EXPORT_SVG, w!("SVG（現在のページ）")),
            (EXPORT_PNG_ZIP, w!("PNG（全ページをZIPへ）")),
            (EXPORT_SVG_ZIP, w!("SVG（全ページをZIPへ）")),
        ] {
            AppendMenuW(menu.0, MF_STRING, id, label)?;
        }
        let mut rect = RECT::default();
        GetWindowRect(app.export_button.get(), &mut rect)?;
        let selected = TrackPopupMenu(
            menu.0,
            TPM_RETURNCMD | TPM_NONOTIFY | TPM_LEFTALIGN | TPM_TOPALIGN,
            rect.left,
            rect.bottom,
            None,
            window,
            None,
        )
        .0 as usize;
        if let Some(kind) = export_kind(selected) {
            export(window, app, kind)?;
        }
    }
    Ok(())
}

fn export(window: HWND, app: &App, kind: Kind) -> Result<()> {
    let current = app.path.borrow().clone();
    let Some(path) = file_dialog::export(window, current.as_deref(), kind)? else {
        return Ok(());
    };
    busy(app, true);
    status(app, "書き出し中…");
    if let Some(ink) = app.ink.get() {
        ink.export(path, kind)?;
    }
    Ok(())
}

pub(super) fn page_command(window: HWND, app: &App, id: usize) {
    if app.busy.get() || !app.ready.get() || app.closing.get() {
        return;
    }
    let action = match id {
        ADD_PAGE => ink::PageAction::Add,
        DELETE_PAGE => {
            if app.page_total.get() <= 1 {
                return;
            }
            let answer = unsafe {
                MessageBoxW(
                    Some(window),
                    &HSTRING::from(format!(
                        "{} / {} ページを削除しますか？\n\nこのページの筆跡も削除されます。この操作は取り消せません。",
                        app.page_index.get() + 1,
                        app.page_total.get()
                    )),
                    w!("ページを削除 — mio note"),
                    MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2,
                )
            };
            if answer != IDYES {
                return;
            }
            ink::PageAction::Delete
        }
        PREVIOUS if app.page_index.get() > 0 => ink::PageAction::Go(app.page_index.get() - 1),
        NEXT if app.page_index.get() + 1 < app.page_total.get() => {
            ink::PageAction::Go(app.page_index.get() + 1)
        }
        A4 | A5 | B4 | B5 | ROTATE => {
            let mut paper = app.paper.get();
            match id {
                A4 => paper.size = PaperSize::A4,
                A5 => paper.size = PaperSize::A5,
                B4 => paper.size = PaperSize::B4,
                B5 => paper.size = PaperSize::B5,
                ROTATE => {
                    paper.orientation = if paper.orientation == Orientation::Portrait {
                        Orientation::Landscape
                    } else {
                        Orientation::Portrait
                    }
                }
                _ => {}
            }
            if paper == app.paper.get() {
                return;
            }
            ink::PageAction::Paper(paper)
        }
        _ => return,
    };
    busy(app, true);
    if let Some(ink) = app.ink.get()
        && let Err(err) = ink.page(action)
    {
        error(window, app, err);
    }
}

fn page_state(window: HWND, app: &App, state: ink::PageInfo) {
    let mut view = app.view.get();
    view.scroll = (0, 0);
    app.view.set(view);
    app.stroke_count.set(state.count);
    app.paper.set(state.page.paper);
    app.rule_spacing.set(state.page.rule_spacing);
    app.page_index.set(state.index);
    app.page_total.set(state.total);
    app.active_tool.set(Tool::Pen);
    refresh_toolbar(app);
    unsafe {
        if let Err(error) = layout(window, app) {
            crate::log!("page layout error: {error}");
        }
        enable_ink_controls(app, true);
        let _ = InvalidateRect(Some(window), None, true);
    }
}

pub(super) fn load(window: HWND, app: &App, path: PathBuf) {
    busy(app, true);
    status(app, "読み込み中…");
    if let Some(ink) = app.ink.get()
        && let Err(err) = ink.load(path)
    {
        error(window, app, err);
    }
}

pub(super) fn request(window: HWND, app: &App, action: Action) {
    if app.closing.get() {
        return;
    }
    if app.busy.get() {
        if matches!(action, Action::Close) {
            app.pending.set(Some(action));
        }
        return;
    }
    if app.dirty.get() {
        let answer = unsafe {
            MessageBoxW(
                Some(window),
                w!("このノートの変更を保存しますか？"),
                w!("mio note"),
                MB_YESNOCANCEL | MB_ICONQUESTION,
            )
        };
        if answer == IDYES {
            app.pending.set(Some(action));
            match save(window, app, false) {
                Ok(true) => {}
                Ok(false) => app.pending.set(None),
                Err(err) => error(window, app, err),
            }
            return;
        }
        if answer != IDNO {
            return;
        }
    }
    match action {
        Action::Open => match file_dialog::choose(window, false, None) {
            Ok(Some(path)) => load(window, app, path),
            Ok(None) => {}
            Err(err) => error(window, app, err),
        },
        Action::Close => unsafe {
            finish_preferences(window, app);
            app.closing.set(true);
            enable_ink_controls(app, false);
            if let Some(ink) = app.ink.get() {
                if let Err(err) = ink.stop() {
                    crate::log!("stop error: {err}");
                    let _ = DestroyWindow(window);
                }
            } else {
                let _ = DestroyWindow(window);
            }
        },
    }
}

pub(super) fn completed(window: HWND, app: &App) {
    let Some(result) = app.ink.get().and_then(Controller::take_completion) else {
        return;
    };
    busy(app, false);
    match result {
        Ok(ink::Completion::Saved { path }) => {
            *app.path.borrow_mut() = Some(path);
            if app.revision.get() == app.saved_revision.get() {
                app.dirty.set(false);
            }
            status(app, "保存しました。");
        }
        Ok(ink::Completion::Loaded { path, state }) => {
            *app.path.borrow_mut() = Some(path);
            app.dirty.set(false);
            app.revision.set(app.revision.get().wrapping_add(1));
            page_state(window, app, state);
            status(app, "読み込みました。");
        }
        Ok(ink::Completion::Page {
            state,
            changed: modified,
        }) => {
            page_state(window, app, state);
            if modified {
                changed(window, app);
            }
            status(app, "Ctrl+S 保存／Ctrl+E PDF出力");
        }
        Ok(ink::Completion::Exported { path: _path, kind }) => {
            crate::log!("{kind:?} exported");
            status(app, kind.completed());
        }
        Err(err) => {
            error(window, app, err);
            return;
        }
    }
    title(window, app);
    if let Some(action) = app.pending.take() {
        request(window, app, action);
    }
}
