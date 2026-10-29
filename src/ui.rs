use crate::ink::{self, Controller, Tool};
use crate::note::Page;
use crate::paper::{Orientation, Paper, PaperSize};
use crate::settings::{
    PenSettings, PenWidth, Preferences, RuleSpacing, SettingsStore, SettingsWriter,
};
use crate::toolbar::{self, Content};
use crate::view::{Layout, Mode, State};
use std::{
    cell::{Cell, OnceCell, RefCell},
    mem::size_of,
    path::PathBuf,
};
use windows::{
    Win32::{
        Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::*,
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Controls::{
                DRAWITEMSTRUCT, ICC_WIN95_CLASSES, INITCOMMONCONTROLSEX, InitCommonControlsEx,
                TTF_IDISHWND, TTF_SUBCLASS, TTM_ADDTOOLW, TTS_ALWAYSTIP, TTS_NOPREFIX, TTTOOLINFOW,
            },
            HiDpi::*,
            Input::KeyboardAndMouse::{
                EnableWindow, GetFocus, GetKeyState, SetFocus, VK_CONTROL, VK_SHIFT,
            },
            WindowsAndMessaging::*,
        },
    },
    core::{Error, HSTRING, PCWSTR, PWSTR, Result, w},
};

const PEN: usize = 101;
const ERASER: usize = 102;
const THIN: usize = 111;
const MEDIUM: usize = 112;
const THICK: usize = 113;
const PRESSURE: usize = 114;
const NARROW: usize = 121;
const STANDARD: usize = 122;
const WIDE: usize = 123;
const OPEN: usize = 131;
const SAVE: usize = 132;
const SAVE_AS: usize = 133;
const EXPORT: usize = 134;
const EXPORT_PDF: usize = 135;
const EXPORT_PNG: usize = 136;
const EXPORT_SVG: usize = 137;
const EXPORT_PNG_ZIP: usize = 138;
const EXPORT_SVG_ZIP: usize = 139;
const PREVIOUS: usize = 141;
const NEXT: usize = 142;
const ADD_PAGE: usize = 143;
const DELETE_PAGE: usize = 144;
const A4: usize = 151;
const A5: usize = 152;
const B4: usize = 153;
const B5: usize = 154;
const ROTATE: usize = 155;
const ZOOM_OUT: usize = 161;
const ZOOM_IN: usize = 162;
const FIT_WIDTH: usize = 163;
const FIT_PAGE: usize = 164;
const RESET_ZOOM: usize = 165;
const ABOUT: usize = 166;
const TOOLBAR_HEIGHT: f32 = 140.0;
const SETTINGS_SAVED: u32 = 0x8006;

struct App {
    page: Cell<HWND>,
    viewport: Cell<HWND>,
    scrollbars: Cell<[HWND; 2]>,
    view_controls: Cell<[HWND; 4]>,
    zoom_button: Cell<HWND>,
    view: Cell<State>,
    view_layout: Cell<Option<Layout>>,
    wheel_remainder: Cell<[i32; 3]>,
    eraser: Cell<HWND>,
    label: Cell<HWND>,
    width_label: Cell<HWND>,
    widths: Cell<[HWND; 3]>,
    pressure: Cell<HWND>,
    spacing_label: Cell<HWND>,
    spacings: Cell<[HWND; 3]>,
    file_buttons: Cell<[HWND; 3]>,
    export_button: Cell<HWND>,
    about_button: Cell<HWND>,
    page_controls: Cell<[HWND; 9]>,
    page_label: Cell<HWND>,
    paper: Cell<Paper>,
    page_index: Cell<usize>,
    page_total: Cell<usize>,
    render_scale: Cell<f32>,
    path: RefCell<Option<PathBuf>>,
    dirty: Cell<bool>,
    revision: Cell<u64>,
    saved_revision: Cell<u64>,
    stroke_count: Cell<u32>,
    busy: Cell<bool>,
    pending: Cell<Option<document::Action>>,
    pen_settings: Cell<PenSettings>,
    rule_spacing: Cell<RuleSpacing>,
    active_tool: Cell<Tool>,
    settings_store: Option<SettingsStore>,
    settings_writer: RefCell<Option<SettingsWriter>>,
    settings_issue: bool,
    font: Cell<HFONT>,
    ink: OnceCell<Controller>,
    ready: Cell<bool>,
    closing: Cell<bool>,
}

impl App {
    fn new() -> Self {
        let settings_store = SettingsStore::for_user()
            .inspect_err(|error| {
                crate::log!("settings location error: {error}");
            })
            .ok();
        let (preferences, settings_issue) = match settings_store.as_ref().map(SettingsStore::load) {
            Some(Ok(preferences)) => (preferences, false),
            Some(Err(error)) => {
                crate::log!("settings load error: {error}");
                (Preferences::default(), true)
            }
            None => (Preferences::default(), true),
        };
        crate::log!("preferences loaded: {preferences:?}");
        Self {
            page: Cell::new(HWND::default()),
            viewport: Cell::new(HWND::default()),
            scrollbars: Cell::new([HWND::default(); 2]),
            view_controls: Cell::new([HWND::default(); 4]),
            zoom_button: Cell::new(HWND::default()),
            view: Cell::new(State::default()),
            view_layout: Cell::new(None),
            wheel_remainder: Cell::new([0; 3]),
            eraser: Cell::new(HWND::default()),
            label: Cell::new(HWND::default()),
            width_label: Cell::new(HWND::default()),
            widths: Cell::new([HWND::default(); 3]),
            pressure: Cell::new(HWND::default()),
            spacing_label: Cell::new(HWND::default()),
            spacings: Cell::new([HWND::default(); 3]),
            file_buttons: Cell::new([HWND::default(); 3]),
            export_button: Cell::new(HWND::default()),
            about_button: Cell::new(HWND::default()),
            page_controls: Cell::new([HWND::default(); 9]),
            page_label: Cell::new(HWND::default()),
            paper: Cell::new(Paper::default()),
            page_index: Cell::new(0),
            page_total: Cell::new(1),
            render_scale: Cell::new(1.0),
            path: RefCell::new(None),
            dirty: Cell::new(false),
            revision: Cell::new(0),
            saved_revision: Cell::new(0),
            stroke_count: Cell::new(0),
            busy: Cell::new(false),
            pending: Cell::new(None),
            pen_settings: Cell::new(preferences.pen),
            rule_spacing: Cell::new(preferences.spacing),
            active_tool: Cell::new(Tool::Pen),
            settings_store,
            settings_writer: RefCell::new(None),
            settings_issue,
            font: Cell::new(HFONT::default()),
            ink: OnceCell::new(),
            ready: Cell::new(false),
            closing: Cell::new(false),
        }
    }

    fn controls(&self) -> impl Iterator<Item = HWND> {
        let widths = self.widths.get();
        let spacings = self.spacings.get();
        let files = self.file_buttons.get();
        [
            self.eraser.get(),
            self.label.get(),
            self.width_label.get(),
            widths[0],
            widths[1],
            widths[2],
            self.pressure.get(),
            self.spacing_label.get(),
            spacings[0],
            spacings[1],
            spacings[2],
            files[0],
            files[1],
            files[2],
        ]
        .into_iter()
        .chain(self.page_controls.get())
        .chain(self.view_controls.get())
        .chain([
            self.export_button.get(),
            self.page_label.get(),
            self.zoom_button.get(),
            self.about_button.get(),
        ])
    }

    fn ink_controls(&self) -> [HWND; 5] {
        let widths = self.widths.get();
        [
            self.eraser.get(),
            widths[0],
            widths[1],
            widths[2],
            self.pressure.get(),
        ]
    }
}

impl Drop for App {
    fn drop(&mut self) {
        unsafe {
            if !self.font.get().is_invalid() {
                let _ = DeleteObject(self.font.get().into());
            }
        }
    }
}

// Destroy child windows before dropping App, including on an initialization error.
struct WindowGuard(HWND);
impl Drop for WindowGuard {
    fn drop(&mut self) {
        unsafe {
            if IsWindow(Some(self.0)).as_bool() {
                let _ = DestroyWindow(self.0);
            }
        }
    }
}

pub fn run() -> Result<()> {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)?;
        let instance = HINSTANCE(GetModuleHandleW(None)?.0);
        let cursor = LoadCursorW(None, IDC_ARROW)?;
        let dpi = GetDpiForSystem();
        // Win32 treats a small integer in this parameter as a resource ID.
        let icon_resource = PCWSTR(std::ptr::without_provenance(1));
        let icon = HICON(
            LoadImageW(
                Some(instance),
                icon_resource,
                IMAGE_ICON,
                GetSystemMetricsForDpi(SM_CXICON, dpi),
                GetSystemMetricsForDpi(SM_CYICON, dpi),
                LR_SHARED,
            )?
            .0,
        );
        let small_icon = HICON(
            LoadImageW(
                Some(instance),
                icon_resource,
                IMAGE_ICON,
                GetSystemMetricsForDpi(SM_CXSMICON, dpi),
                GetSystemMetricsForDpi(SM_CYSMICON, dpi),
                LR_SHARED,
            )?
            .0,
        );
        let parent_class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            hInstance: instance,
            lpszClassName: w!("RustMioNoteWindow"),
            lpfnWndProc: Some(window_proc),
            hCursor: cursor,
            hIcon: icon,
            hIconSm: small_icon,
            ..Default::default()
        };
        let page_class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            hInstance: instance,
            lpszClassName: w!("RustMioNotePage"),
            lpfnWndProc: Some(page_proc),
            hCursor: cursor,
            ..Default::default()
        };
        let viewport_class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            hInstance: instance,
            lpszClassName: w!("RustMioNoteViewport"),
            lpfnWndProc: Some(view::window_proc),
            hCursor: cursor,
            ..Default::default()
        };
        if RegisterClassExW(&parent_class) == 0
            || RegisterClassExW(&page_class) == 0
            || RegisterClassExW(&viewport_class) == 0
        {
            return Err(Error::from_thread());
        }
        let app = Box::new(App::new());
        let scale = dpi as f32 / 96.0;
        let mut bounds = RECT {
            right: (840.0 * scale) as i32,
            bottom: (720.0 * scale) as i32,
            ..Default::default()
        };
        AdjustWindowRectExForDpi(
            &mut bounds,
            WS_OVERLAPPEDWINDOW,
            false,
            WINDOW_EX_STYLE::default(),
            dpi,
        )?;
        let window = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("RustMioNoteWindow"),
            &HSTRING::from(format!(
                "罫線ノート — mio note {}",
                env!("CARGO_PKG_VERSION")
            )),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            bounds.right - bounds.left,
            bounds.bottom - bounds.top,
            None,
            None,
            Some(instance),
            Some((&*app as *const App).cast()),
        )?;
        let _window_guard = WindowGuard(window);

        // Start Ink as soon as its HWND exists. The host initializes on its own
        // thread while the UI creates the toolbar, font and tooltips.
        create_page(window, instance, &app)?;
        let mut rect = RECT::default();
        GetClientRect(app.page.get(), &mut rect)?;
        let controller = Controller::new(
            window,
            app.page.get(),
            rect.right as u32,
            rect.bottom as u32,
            app.render_scale.get(),
            app.pen_settings.get(),
            Page {
                paper: app.paper.get(),
                rule_spacing: app.rule_spacing.get(),
            },
        )?;
        let _ = app.ink.set(controller);
        create_children(window, instance, &app)?;
        layout(window, &app)?;
        let _ = ShowWindow(window, SW_SHOW);
        crate::log!("window visible");
        let mut message = MSG::default();
        loop {
            let result = GetMessageW(&mut message, None, 0, 0).0;
            if result == -1 {
                return Err(Error::from_thread());
            }
            if result == 0 {
                break;
            }
            if message.message == WM_KEYDOWN && app.ready.get() {
                if message.wParam.0 == 0x70 && message.lParam.0 & (1 << 30) == 0 {
                    about::show(window)?;
                    continue;
                }
                if GetKeyState(VK_CONTROL.0 as i32) < 0 {
                    let command = match message.wParam.0 {
                        0x4f => Some(OPEN),
                        0x53 => Some(if GetKeyState(VK_SHIFT.0 as i32) < 0 {
                            SAVE_AS
                        } else {
                            SAVE
                        }),
                        0x45 => Some(EXPORT_PDF),
                        0x30 | 0x60 => Some(FIT_PAGE),
                        0x31 | 0x61 => Some(FIT_WIDTH),
                        0x32 | 0x62 => Some(RESET_ZOOM),
                        0xbb | 0x6b => Some(ZOOM_IN),
                        0xbd | 0x6d => Some(ZOOM_OUT),
                        _ => None,
                    };
                    if let Some(command) = command {
                        if matches!(
                            command,
                            ZOOM_OUT | ZOOM_IN | FIT_WIDTH | FIT_PAGE | RESET_ZOOM
                        ) {
                            view::command(window, &app, command, None)?;
                        } else if message.lParam.0 & (1 << 30) == 0 {
                            document::command(window, &app, command);
                        }
                        continue;
                    }
                }
                if view::key(window, &app, message.wParam.0)? {
                    continue;
                }
                let id = match message.wParam.0 {
                    0x50 if GetKeyState(VK_CONTROL.0 as i32) >= 0 => Some(PEN),
                    0x45 if GetKeyState(VK_CONTROL.0 as i32) >= 0 => Some(ERASER),
                    _ => None,
                };
                if let Some(id) = id {
                    if id == ERASER && message.lParam.0 & (1 << 30) != 0 {
                        continue;
                    }
                    set_tool(&app, id)?;
                    continue;
                }
            }
            if !IsDialogMessageW(window, &message).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        Ok(())
    }
}

unsafe fn create_page(window: HWND, instance: HINSTANCE, app: &App) -> Result<()> {
    unsafe { view::create(window, instance, app) }
}

unsafe fn create_children(window: HWND, instance: HINSTANCE, app: &App) -> Result<()> {
    unsafe {
        let button_style = WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32);
        app.label.set(create_control(
            window,
            instance,
            w!("STATIC"),
            if app.settings_issue {
                w!("設定を読み込めませんでした（初期値で起動）。")
            } else {
                w!("Ctrl+S 保存／Ctrl+O 開く")
            },
            0,
            WS_GROUP,
        )?);
        app.width_label.set(create_control(
            window,
            instance,
            w!("STATIC"),
            w!("線幅"),
            0,
            WS_GROUP,
        )?);
        app.widths.set([
            create_control(
                window,
                instance,
                w!("BUTTON"),
                w!("細"),
                THIN,
                button_style | WS_GROUP,
            )?,
            create_control(
                window,
                instance,
                w!("BUTTON"),
                w!("中"),
                MEDIUM,
                button_style,
            )?,
            create_control(
                window,
                instance,
                w!("BUTTON"),
                w!("太"),
                THICK,
                button_style,
            )?,
        ]);
        app.pressure.set(create_control(
            window,
            instance,
            w!("BUTTON"),
            w!("筆圧：オフ"),
            PRESSURE,
            button_style | WS_GROUP,
        )?);
        app.eraser.set(create_control(
            window,
            instance,
            w!("BUTTON"),
            w!("消しゴム：オフ (E)"),
            ERASER,
            button_style | WS_GROUP,
        )?);
        app.spacing_label.set(create_control(
            window,
            instance,
            w!("STATIC"),
            w!("罫線間隔"),
            0,
            WS_GROUP,
        )?);
        app.spacings.set([
            create_control(
                window,
                instance,
                w!("BUTTON"),
                w!("罫線間隔：狭"),
                NARROW,
                button_style | WS_GROUP,
            )?,
            create_control(
                window,
                instance,
                w!("BUTTON"),
                w!("罫線間隔：標準"),
                STANDARD,
                button_style,
            )?,
            create_control(
                window,
                instance,
                w!("BUTTON"),
                w!("罫線間隔：広"),
                WIDE,
                button_style,
            )?,
        ]);
        app.file_buttons.set([
            create_control(
                window,
                instance,
                w!("BUTTON"),
                w!("開く…"),
                OPEN,
                button_style | WS_GROUP,
            )?,
            create_control(
                window,
                instance,
                w!("BUTTON"),
                w!("保存"),
                SAVE,
                button_style,
            )?,
            create_control(
                window,
                instance,
                w!("BUTTON"),
                w!("別名で保存…"),
                SAVE_AS,
                button_style,
            )?,
        ]);
        app.export_button.set(create_control(
            window,
            instance,
            w!("BUTTON"),
            w!("書き出し"),
            EXPORT,
            button_style,
        )?);
        app.about_button.set(create_control(
            window,
            instance,
            w!("BUTTON"),
            w!("バージョン情報・コピーライト"),
            ABOUT,
            button_style | WS_GROUP,
        )?);
        app.page_label.set(create_control(
            window,
            instance,
            w!("STATIC"),
            w!("1 / 1"),
            0,
            WS_GROUP,
        )?);
        app.page_controls.set([
            create_control(
                window,
                instance,
                w!("BUTTON"),
                w!("前のページ"),
                PREVIOUS,
                button_style | WS_GROUP,
            )?,
            create_control(
                window,
                instance,
                w!("BUTTON"),
                w!("次のページ"),
                NEXT,
                button_style,
            )?,
            create_control(
                window,
                instance,
                w!("BUTTON"),
                w!("ページ追加"),
                ADD_PAGE,
                button_style,
            )?,
            create_control(
                window,
                instance,
                w!("BUTTON"),
                w!("A4"),
                A4,
                button_style | WS_GROUP,
            )?,
            create_control(window, instance, w!("BUTTON"), w!("A5"), A5, button_style)?,
            create_control(
                window,
                instance,
                w!("BUTTON"),
                w!("B4 (JIS)"),
                B4,
                button_style,
            )?,
            create_control(
                window,
                instance,
                w!("BUTTON"),
                w!("B5 (JIS)"),
                B5,
                button_style,
            )?,
            create_control(
                window,
                instance,
                w!("BUTTON"),
                w!("用紙の縦横切り替え"),
                ROTATE,
                button_style,
            )?,
            create_control(
                window,
                instance,
                w!("BUTTON"),
                w!("現在のページを削除"),
                DELETE_PAGE,
                button_style,
            )?,
        ]);
        view::create_controls(window, instance, app)?;
        enable_ink_controls(app, false);
        refresh_toolbar(app);
        update_font(window, app);
        if let Err(error) = create_tooltips(window, instance, app) {
            crate::log!("tooltip error: {error}");
        }
        Ok(())
    }
}

unsafe fn create_control(
    window: HWND,
    instance: HINSTANCE,
    class: PCWSTR,
    text: PCWSTR,
    id: usize,
    style: WINDOW_STYLE,
) -> Result<HWND> {
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class,
            text,
            WS_CHILD | WS_VISIBLE | style,
            0,
            0,
            1,
            1,
            Some(window),
            Some(HMENU(id as _)),
            Some(instance),
            None,
        )
    }
}

unsafe fn create_tooltips(window: HWND, instance: HINSTANCE, app: &App) -> Result<()> {
    unsafe {
        InitCommonControlsEx(&INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_WIN95_CLASSES,
        })
        .ok()?;
        let tips = CreateWindowExW(
            WS_EX_TOPMOST,
            w!("tooltips_class32"),
            None,
            WS_POPUP | WINDOW_STYLE(TTS_ALWAYSTIP | TTS_NOPREFIX),
            0,
            0,
            0,
            0,
            Some(window),
            None,
            Some(instance),
            None,
        )?;
        let widths = app.widths.get();
        let spacings = app.spacings.get();
        let files = app.file_buttons.get();
        let pages = app.page_controls.get();
        let views = app.view_controls.get();
        for (control, text) in [
            (app.eraser.get(), w!("消しゴム切り替え (E)／ペンに戻る (P)")),
            (widths[0], w!("線幅：細")),
            (widths[1], w!("線幅：中")),
            (widths[2], w!("線幅：太")),
            (app.pressure.get(), w!("筆圧のオン／オフ（青色がオン）")),
            (spacings[0], w!("罫線間隔：狭")),
            (spacings[1], w!("罫線間隔：標準")),
            (spacings[2], w!("罫線間隔：広")),
            (files[0], w!("ノートを開く (Ctrl+O)")),
            (files[1], w!("ノートを保存 (Ctrl+S)")),
            (files[2], w!("別名で保存 (Ctrl+Shift+S)")),
            (
                app.export_button.get(),
                w!("PDF・PNG・SVGの書き出し（Ctrl+EでPDF）"),
            ),
            (pages[0], w!("前のページ")),
            (pages[1], w!("次のページ")),
            (pages[2], w!("末尾にページを追加")),
            (pages[3], w!("A4: 210×297 mm")),
            (pages[4], w!("A5: 148×210 mm")),
            (pages[5], w!("B4 (JIS): 257×364 mm")),
            (pages[6], w!("B5 (JIS): 182×257 mm")),
            (
                pages[7],
                w!("縦・横の切り替え。小さい用紙へ変更すると筆跡を縮小します。"),
            ),
            (views[0], w!("縮小 (Ctrl+−)／Ctrl+ホイール")),
            (
                pages[8],
                w!("現在のページを削除（最後の1ページは削除できません）"),
            ),
            (views[1], w!("拡大 (Ctrl+＋)／Ctrl+ホイール")),
            (views[2], w!("横幅に合わせる (Ctrl+1)")),
            (views[3], w!("ページ全体を表示 (Ctrl+0)")),
            (app.zoom_button.get(), w!("100%に戻す (Ctrl+2)")),
            (
                app.about_button.get(),
                w!("バージョン情報・コピーライト (F1)"),
            ),
        ] {
            let info = TTTOOLINFOW {
                // V1 fields are enough here and also work without a comctl32 v6 manifest.
                cbSize: std::mem::offset_of!(TTTOOLINFOW, lParam) as u32,
                uFlags: TTF_IDISHWND | TTF_SUBCLASS,
                hwnd: window,
                uId: control.0 as usize,
                lpszText: PWSTR(text.0 as *mut _),
                ..Default::default()
            };
            if SendMessageW(
                tips,
                TTM_ADDTOOLW,
                None,
                Some(LPARAM((&info as *const TTTOOLINFOW) as isize)),
            )
            .0 == 0
            {
                return Err(Error::new(
                    windows::Win32::Foundation::E_FAIL,
                    "Could not add toolbar tooltip",
                ));
            }
        }
        Ok(())
    }
}

fn refresh_toolbar(app: &App) {
    unsafe {
        let _ = SetWindowTextW(
            app.eraser.get(),
            if app.active_tool.get() == Tool::Eraser {
                w!("消しゴム：オン (E)")
            } else {
                w!("消しゴム：オフ (E)")
            },
        );
        let _ = SetWindowTextW(
            app.pressure.get(),
            if app.pen_settings.get().pressure {
                w!("筆圧：オン")
            } else {
                w!("筆圧：オフ")
            },
        );
        let _ = SetWindowTextW(
            app.page_label.get(),
            &HSTRING::from(format!(
                "{} / {}",
                app.page_index.get() + 1,
                app.page_total.get()
            )),
        );
        for control in app
            .ink_controls()
            .into_iter()
            .chain(app.spacings.get())
            .chain(app.page_controls.get())
            .chain(app.view_controls.get())
            .chain([app.zoom_button.get()])
        {
            let _ = InvalidateRect(Some(control), None, false);
        }
    }
}

fn save_preferences(window: HWND, app: &App) {
    let preferences = Preferences {
        pen: app.pen_settings.get(),
        spacing: app.rule_spacing.get(),
    };
    let result = (|| {
        let mut writer = app.settings_writer.borrow_mut();
        if writer.is_none() {
            let store = app.settings_store.clone().ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Settings directory unavailable",
                )
            })?;
            let target = window.0 as usize;
            *writer = Some(SettingsWriter::new(store, move || unsafe {
                let _ = PostMessageW(
                    Some(HWND(target as _)),
                    SETTINGS_SAVED,
                    WPARAM(0),
                    LPARAM(0),
                );
            })?);
        }
        writer
            .as_mut()
            .ok_or_else(|| std::io::Error::other("Settings writer unavailable"))?
            .save(preferences)
    })();
    if let Err(error) = result {
        preferences_status(app, Err(error));
    }
}

fn preferences_status(app: &App, result: std::io::Result<()>) {
    let label = match result {
        Ok(()) => {
            crate::log!("preferences saved");
            if app.busy.get() {
                return;
            }
            w!("Ctrl+S 保存／Ctrl+O 開く")
        }
        Err(error) => {
            crate::log!("settings save error: {error}");
            w!("設定を保存できませんでした。")
        }
    };
    unsafe {
        let _ = SetWindowTextW(app.label.get(), label);
    }
}

fn finish_preferences(window: HWND, app: &App) {
    let writer = app.settings_writer.borrow_mut().take();
    if let Some(mut writer) = writer
        && let Err(error) = writer.finish()
    {
        crate::log!("settings save error on close: {error}");
        unsafe {
            MessageBoxW(
                Some(window),
                w!("設定を保存できませんでした。次回は以前の設定で起動します。"),
                w!("mio note"),
                MB_OK | MB_ICONERROR,
            );
        }
    }
}

fn toolbar_button(app: &App, id: usize) -> Option<(Content, bool)> {
    let pen = app.pen_settings.get();
    let spacing = app.rule_spacing.get();
    Some(match id {
        ERASER => (Content::Eraser, app.active_tool.get() == Tool::Eraser),
        THIN => (Content::Text("細"), pen.width == PenWidth::Thin),
        MEDIUM => (Content::Text("中"), pen.width == PenWidth::Medium),
        THICK => (Content::Text("太"), pen.width == PenWidth::Thick),
        PRESSURE => (Content::Pressure, pen.pressure),
        NARROW => (Content::Spacing(4.0), spacing == RuleSpacing::Narrow),
        STANDARD => (Content::Spacing(7.0), spacing == RuleSpacing::Standard),
        WIDE => (Content::Spacing(10.0), spacing == RuleSpacing::Wide),
        OPEN => (Content::Text("開く…"), false),
        SAVE => (Content::Text("保存"), false),
        SAVE_AS => (Content::Text("別名で保存…"), false),
        EXPORT => (Content::Text("書き出し ▾"), false),
        ABOUT => (Content::Text("©"), false),
        PREVIOUS => (Content::Text("<"), false),
        NEXT => (Content::Text(">"), false),
        ADD_PAGE => (Content::Text("+ ページ"), false),
        DELETE_PAGE => (Content::Text("削除"), false),
        A4 => (Content::Text("A4"), app.paper.get().size == PaperSize::A4),
        A5 => (Content::Text("A5"), app.paper.get().size == PaperSize::A5),
        B4 => (Content::Text("B4"), app.paper.get().size == PaperSize::B4),
        B5 => (Content::Text("B5"), app.paper.get().size == PaperSize::B5),
        ROTATE => (
            Content::Text(if app.paper.get().orientation == Orientation::Portrait {
                "縦"
            } else {
                "横"
            }),
            false,
        ),
        ZOOM_OUT => (Content::Zoom(false), false),
        ZOOM_IN => (Content::Zoom(true), false),
        FIT_WIDTH => (Content::Text("横幅"), app.view.get().mode == Mode::Width),
        FIT_PAGE => (Content::Text("全体表示"), app.view.get().mode == Mode::Page),
        RESET_ZOOM => (
            Content::Percent(
                app.view_layout
                    .get()
                    .map_or(100, |view| (view.zoom() * 100.0).round() as u32),
            ),
            app.view.get().mode == Mode::Zoom(1.0),
        ),
        _ => return None,
    })
}

unsafe fn enable_ink_controls(app: &App, enabled: bool) {
    unsafe {
        let enabled = enabled && app.ready.get() && !app.busy.get() && !app.closing.get();
        for control in app
            .ink_controls()
            .into_iter()
            .chain(app.spacings.get())
            .chain(app.file_buttons.get())
            .chain(app.page_controls.get())
            .chain(app.view_controls.get())
            .chain([app.export_button.get(), app.zoom_button.get()])
            .chain(app.scrollbars.get())
        {
            let _ = EnableWindow(control, enabled);
        }
        let pages = app.page_controls.get();
        let _ = EnableWindow(pages[0], enabled && app.page_index.get() > 0);
        let _ = EnableWindow(
            pages[1],
            enabled && app.page_index.get() + 1 < app.page_total.get(),
        );
        let _ = EnableWindow(
            pages[2],
            enabled && app.page_total.get() < crate::note::MAX_PAGES,
        );
        let _ = EnableWindow(pages[8], enabled && app.page_total.get() > 1);
    }
}

unsafe fn update_font(window: HWND, app: &App) {
    unsafe {
        let height = -(14.0 * GetDpiForWindow(window) as f32 / 96.0).round() as i32;
        let font = CreateFontW(
            height,
            0,
            0,
            0,
            FW_NORMAL.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            DEFAULT_PITCH.0 as u32,
            w!("Yu Gothic UI"),
        );
        if !font.is_invalid() {
            let old = app.font.replace(font);
            for child in app.controls() {
                SendMessageW(
                    child,
                    WM_SETFONT,
                    Some(WPARAM(font.0 as usize)),
                    Some(LPARAM(1)),
                );
            }
            if !old.is_invalid() {
                let _ = DeleteObject(old.into());
            }
        }
    }
}

unsafe fn layout(window: HWND, app: &App) -> Result<()> {
    unsafe {
        if app.page.get().is_invalid() {
            return Ok(());
        }
        let mut bounds = RECT::default();
        GetClientRect(window, &mut bounds)?;
        let scale = GetDpiForWindow(window) as f32 / 96.0;
        let px = |value: f32| (value * scale).round() as i32;
        view::layout(window, app)?;
        MoveWindow(
            app.eraser.get(),
            px(240.0),
            px(10.0),
            px(40.0),
            px(36.0),
            true,
        )?;
        MoveWindow(
            app.label.get(),
            px(380.0),
            px(61.0),
            (bounds.right - px(392.0)).max(0),
            px(26.0),
            true,
        )?;
        MoveWindow(
            app.width_label.get(),
            px(12.0),
            px(20.0),
            px(34.0),
            px(26.0),
            true,
        )?;
        for (index, button) in app.widths.get().into_iter().enumerate() {
            MoveWindow(
                button,
                px(48.0 + index as f32 * 44.0),
                px(10.0),
                px(40.0),
                px(36.0),
                true,
            )?;
        }
        MoveWindow(
            app.pressure.get(),
            px(196.0),
            px(10.0),
            px(40.0),
            px(36.0),
            true,
        )?;
        MoveWindow(
            app.spacing_label.get(),
            px(296.0),
            px(20.0),
            px(64.0),
            px(26.0),
            true,
        )?;
        for (index, button) in app.spacings.get().into_iter().enumerate() {
            MoveWindow(
                button,
                px(368.0 + index as f32 * 44.0),
                px(10.0),
                px(40.0),
                px(36.0),
                true,
            )?;
        }
        for (button, (x, width)) in
            app.file_buttons
                .get()
                .into_iter()
                .zip([(12.0, 64.0), (84.0, 64.0), (156.0, 112.0)])
        {
            MoveWindow(button, px(x), px(54.0), px(width), px(32.0), true)?;
        }
        MoveWindow(
            app.export_button.get(),
            px(280.0),
            px(54.0),
            px(88.0),
            px(32.0),
            true,
        )?;
        let pages = app.page_controls.get();
        for (button, (x, width)) in pages.into_iter().zip([
            (12.0, 32.0),
            (108.0, 32.0),
            (148.0, 84.0),
            (320.0, 36.0),
            (360.0, 36.0),
            (400.0, 36.0),
            (440.0, 36.0),
            (488.0, 64.0),
            (240.0, 64.0),
        ]) {
            MoveWindow(button, px(x), px(98.0), px(width), px(32.0), true)?;
        }
        MoveWindow(
            app.page_label.get(),
            px(48.0),
            px(106.0),
            px(56.0),
            px(24.0),
            true,
        )?;
        view::layout_controls(window, app)?;
        MoveWindow(
            app.about_button.get(),
            px(560.0),
            px(98.0),
            px(28.0),
            px(32.0),
            true,
        )?;
        let _ = InvalidateRect(Some(app.page.get()), None, false);
        Ok(())
    }
}

fn set_tool(app: &App, id: usize) -> Result<()> {
    if !app.ready.get() || app.closing.get() || app.busy.get() {
        return Ok(());
    }
    let tool = if id == ERASER && app.active_tool.get() == Tool::Pen {
        Tool::Eraser
    } else {
        Tool::Pen
    };
    if app.active_tool.get() == tool {
        return Ok(());
    }
    if let Some(ink) = app.ink.get() {
        ink.tool(tool)?;
    }
    app.active_tool.set(tool);
    refresh_toolbar(app);
    Ok(())
}

fn set_pen(window: HWND, app: &App, id: usize) -> Result<()> {
    if !app.ready.get() || app.closing.get() || app.busy.get() {
        return Ok(());
    }
    let mut settings = app.pen_settings.get();
    match id {
        THIN => settings.width = PenWidth::Thin,
        MEDIUM => settings.width = PenWidth::Medium,
        THICK => settings.width = PenWidth::Thick,
        PRESSURE => settings.pressure = !settings.pressure,
        _ => return Ok(()),
    }
    if app.pen_settings.get() == settings {
        return Ok(());
    }
    if let Some(ink) = app.ink.get() {
        ink.pen(settings)?;
    }
    app.pen_settings.set(settings);
    refresh_toolbar(app);
    save_preferences(window, app);
    Ok(())
}

fn set_spacing(window: HWND, app: &App, id: usize) {
    if !app.ready.get() || app.closing.get() || app.busy.get() {
        return;
    }
    let spacing = match id {
        NARROW => RuleSpacing::Narrow,
        STANDARD => RuleSpacing::Standard,
        WIDE => RuleSpacing::Wide,
        _ => return,
    };
    if app.rule_spacing.replace(spacing) == spacing {
        return;
    }
    document::changed(window, app);
    if let Some(ink) = app.ink.get()
        && let Err(error) = ink.spacing(spacing)
    {
        crate::log!("spacing error: {error}");
    }
    unsafe {
        let _ = InvalidateRect(Some(app.page.get()), None, false);
    }
    refresh_toolbar(app);
    crate::log!("rule_spacing={:.0}", spacing.size());
    save_preferences(window, app);
}

unsafe extern "system" fn window_proc(
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
        if ptr.is_null() {
            return DefWindowProcW(window, message, wparam, lparam);
        }
        let app = &*ptr;
        if message == WM_DRAWITEM && lparam.0 != 0 {
            let item = &*(lparam.0 as *const DRAWITEMSTRUCT);
            if let Some((content, active)) = toolbar_button(app, item.CtlID as usize) {
                toolbar::draw(
                    item,
                    GetDpiForWindow(window) as f32 / 96.0,
                    app.font.get(),
                    content,
                    active,
                );
                return LRESULT(1);
            }
        }
        if message == WM_ERASEBKGND {
            let dc = HDC(wparam.0 as _);
            let mut rect = RECT::default();
            let _ = GetClientRect(window, &mut rect);
            let brush = CreateSolidBrush(rgb(245, 246, 247));
            FillRect(dc, &rect, brush);
            let _ = DeleteObject(brush.into());
            return LRESULT(1);
        }
        let result: Result<bool> = (|| {
            match message {
                WM_GETMINMAXINFO => {
                    let dpi = GetDpiForWindow(window).max(96);
                    let scale = dpi as f32 / 96.0;
                    let mut bounds = RECT {
                        right: (600.0 * scale).round() as i32,
                        bottom: (480.0 * scale).round() as i32,
                        ..Default::default()
                    };
                    AdjustWindowRectExForDpi(
                        &mut bounds,
                        WS_OVERLAPPEDWINDOW,
                        false,
                        WINDOW_EX_STYLE::default(),
                        dpi,
                    )?;
                    let limits = &mut *(lparam.0 as *mut MINMAXINFO);
                    limits.ptMinTrackSize.x = bounds.right - bounds.left;
                    limits.ptMinTrackSize.y = bounds.bottom - bounds.top;
                }
                WM_SIZE if wparam.0 != SIZE_MINIMIZED as usize => {
                    layout(window, app)?;
                }
                WM_DPICHANGED => {
                    let suggested = &*(lparam.0 as *const RECT);
                    SetWindowPos(
                        window,
                        None,
                        suggested.left,
                        suggested.top,
                        suggested.right - suggested.left,
                        suggested.bottom - suggested.top,
                        SWP_NOZORDER | SWP_NOACTIVATE,
                    )?;
                    update_font(window, app);
                    layout(window, app)?;
                }
                WM_COMMAND => {
                    let id = wparam.0 & 0xffff;
                    if (wparam.0 >> 16) & 0xffff == BN_CLICKED as usize {
                        match id {
                            PEN | ERASER => set_tool(app, id)?,
                            THIN | MEDIUM | THICK | PRESSURE => set_pen(window, app, id)?,
                            NARROW | STANDARD | WIDE => set_spacing(window, app, id),
                            OPEN | SAVE | SAVE_AS | EXPORT | EXPORT_PDF | EXPORT_PNG
                            | EXPORT_SVG | EXPORT_PNG_ZIP | EXPORT_SVG_ZIP => {
                                document::command(window, app, id)
                            }
                            PREVIOUS | NEXT | ADD_PAGE | DELETE_PAGE | A4 | A5 | B4 | B5
                            | ROTATE => document::page_command(window, app, id),
                            ZOOM_OUT | ZOOM_IN | FIT_WIDTH | FIT_PAGE | RESET_ZOOM => {
                                view::command(window, app, id, None)?;
                            }
                            ABOUT if !app.closing.get() => about::show(window)?,
                            _ => {}
                        }
                    }
                }
                WM_HSCROLL | WM_VSCROLL => view::scrollbar(window, app, message, wparam)?,
                WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
                    view::wheel(window, app, message, wparam, lparam)?;
                }
                ink::READY => {
                    if app.ready.replace(true) || app.closing.get() {
                        return Ok(true);
                    }
                    enable_ink_controls(app, true);
                    crate::log!("input ready");
                    match crate::paths::startup_note() {
                        Ok(Some(path)) => document::load(window, app, path),
                        Ok(None) => {}
                        Err(error) => document::error(window, app, error),
                    }
                }
                ink::STROKES => {
                    app.stroke_count.set(wparam.0 as u32);
                    crate::log!("stroke_count={}", app.stroke_count.get());
                    document::changed(window, app);
                    // The Ink input site can leave this UI thread without a
                    // keyboard focus window after drawing. Restore shortcuts
                    // only in our active window; preserve controls and dialogs.
                    restore_keyboard_focus(window);
                }
                SETTINGS_SAVED => {
                    let result = app
                        .settings_writer
                        .borrow()
                        .as_ref()
                        .and_then(SettingsWriter::take_completion);
                    if let Some(result) = result {
                        preferences_status(app, result);
                    }
                }
                ink::IO_DONE => {
                    document::completed(window, app);
                    restore_keyboard_focus(window);
                }
                ink::FAILED => {
                    app.ready.set(false);
                    app.busy.set(false);
                    enable_ink_controls(app, false);
                    SetWindowTextW(
                        app.label.get(),
                        w!("手書きの処理に失敗しました。アプリを再起動してください。"),
                    )?;
                    if app.closing.get() {
                        DestroyWindow(window)?;
                    }
                }
                ink::STOPPED => {
                    DestroyWindow(window)?;
                }
                WM_CLOSE => {
                    document::request(window, app, document::Action::Close);
                }
                WM_DESTROY => {
                    PostQuitMessage(0);
                }
                _ => return Ok(false),
            }
            Ok(true)
        })();
        match result {
            Ok(true) => LRESULT(0),
            Ok(false) => DefWindowProcW(window, message, wparam, lparam),
            Err(error) => {
                crate::log!("window error: {error}");
                LRESULT(0)
            }
        }
    }
}

unsafe extern "system" fn page_proc(
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
        if matches!(message, WM_MOUSEWHEEL | WM_MOUSEHWHEEL) {
            let ptr = GetWindowLongPtrW(window, GWLP_USERDATA) as *const App;
            if !ptr.is_null() {
                if let Err(error) =
                    view::wheel(GetAncestor(window, GA_ROOT), &*ptr, message, wparam, lparam)
                {
                    crate::log!("page wheel error: {error}");
                }
                return LRESULT(0);
            }
        }
        if message == WM_PAINT {
            let mut paint = PAINTSTRUCT::default();
            let dc = BeginPaint(window, &mut paint);
            let mut rect = RECT::default();
            let _ = GetClientRect(window, &mut rect);
            let paper = CreateSolidBrush(rgb(255, 254, 249));
            FillRect(dc, &rect, paper);
            let _ = DeleteObject(paper.into());
            let ptr = GetWindowLongPtrW(window, GWLP_USERDATA) as *const App;
            let scale = if ptr.is_null() {
                1.0
            } else {
                (*ptr).render_scale.get()
            };
            let px = |value: f32| (value * scale).round() as i32;
            let rule_width = (GetDpiForWindow(window) as f32 / 96.0).round().max(1.0) as i32;
            let rule = CreatePen(PS_SOLID, rule_width, rgb(213, 224, 234));
            let original = SelectObject(dc, rule.into());
            let spacing = if ptr.is_null() {
                RuleSpacing::default()
            } else {
                (*ptr).rule_spacing.get()
            };
            let mut rule_y = 64.0;
            while px(rule_y) < rect.bottom {
                let y = px(rule_y);
                let _ = MoveToEx(dc, 0, y, None);
                let _ = LineTo(dc, rect.right, y);
                rule_y += spacing.size();
            }
            let margin = CreatePen(PS_SOLID, rule_width, rgb(228, 181, 181));
            SelectObject(dc, margin.into());
            let _ = MoveToEx(dc, px(64.0), 0, None);
            let _ = LineTo(dc, px(64.0), rect.bottom);
            SelectObject(dc, original);
            let _ = DeleteObject(rule.into());
            let _ = DeleteObject(margin.into());
            let _ = EndPaint(window, &paint);
            return LRESULT(0);
        }
        if message == WM_ERASEBKGND {
            return LRESULT(1);
        }
        DefWindowProcW(window, message, wparam, lparam)
    }
}

pub(crate) fn restore_keyboard_focus(window: HWND) {
    unsafe {
        if GetForegroundWindow() == window && GetFocus().is_invalid() {
            let _ = SetFocus(Some(window));
        }
    }
}

fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(r as u32 | ((g as u32) << 8) | ((b as u32) << 16))
}

pub fn show_error(error: &Error) {
    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(format!("ノートを起動できませんでした。\n\n{error}")),
            w!("mio note"),
            MB_OK | MB_ICONERROR,
        );
    }
}
mod about;
mod document;
mod view;
