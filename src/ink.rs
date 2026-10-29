//! Windows Ink owns both wet and dry ink; there is no per-point Rust renderer.
//! The thread-local engine is only touched by IInkHostWorkItem::Invoke.

use crate::export::Kind;
use crate::settings::PenSettings;
use crate::{
    ink_io::{self, PageSnapshot, Task, Transfer},
    note::Page,
    paper::Paper,
};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU32, Ordering},
    },
};
use windows::{
    Foundation::{Size, TypedEventHandler},
    UI::{
        Color,
        Core::CoreInputDeviceTypes,
        Input::Inking::{
            InkDrawingAttributes, InkInputProcessingMode, InkPresenter, InkStrokeContainer,
            InkStrokesCollectedEventArgs, InkStrokesErasedEventArgs, PenTipShape,
        },
    },
    Win32::{
        Foundation::{E_FAIL, HWND, LPARAM, WPARAM},
        Graphics::DirectComposition::{
            DCompositionCreateDevice3, IDCompositionDevice, IDCompositionTarget,
            IDCompositionVisual,
        },
        System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
        UI::{
            Input::Ink::{
                IInkDesktopHost, IInkHostWorkItem, IInkHostWorkItem_Impl, IInkPresenterDesktop,
                InkDesktopHost,
            },
            WindowsAndMessaging::PostMessageW,
        },
    },
    core::{AgileReference, Error, Interface, Result, implement},
};

pub const READY: u32 = 0x8001;
pub const FAILED: u32 = 0x8002;
pub const STROKES: u32 = 0x8003;
pub const STOPPED: u32 = 0x8004;
pub const IO_DONE: u32 = 0x8005;

#[derive(Clone, Copy)]
pub enum PageAction {
    Add,
    Delete,
    Go(usize),
    Paper(Paper),
}

// Attach a surviving page before releasing the current page. A failed switch
// must leave both the list and the active index intact.
pub(crate) fn delete_current_page<T>(
    pages: &mut Vec<T>,
    active: &mut usize,
    activate: impl FnOnce(&mut T) -> Result<()>,
) -> Result<()> {
    if pages.len() <= 1 {
        return Err(Error::new(E_FAIL, "最後の1ページは削除できません。"));
    }
    if *active >= pages.len() {
        return Err(Error::new(E_FAIL, "ページがありません。"));
    }
    let removed = *active;
    let next = if removed + 1 < pages.len() {
        removed + 1
    } else {
        removed - 1
    };
    activate(&mut pages[next])?;
    pages.remove(removed);
    *active = removed.min(pages.len() - 1);
    Ok(())
}
#[derive(Clone, Copy)]
pub struct PageInfo {
    pub index: usize,
    pub total: usize,
    pub page: Page,
    pub count: u32,
}

pub enum Completion {
    Saved { path: PathBuf },
    Loaded { path: PathBuf, state: PageInfo },
    Page { state: PageInfo, changed: bool },
    Exported { path: PathBuf, kind: Kind },
}

type IoResult = std::result::Result<Completion, String>;

struct Shared {
    host: AgileReference<IInkDesktopHost>,
    window: usize,
    completed: Mutex<Option<IoResult>>,
}

impl Shared {
    fn queue(self: &Arc<Self>, command: Command) -> Result<()> {
        let work: IInkHostWorkItem = WorkItem {
            command: Mutex::new(Some(command)),
            notify_window: self.window,
            shared: self.clone(),
        }
        .into();
        unsafe { self.host.resolve()?.QueueWorkItem(&work) }
    }

    fn finish(&self, result: IoResult) {
        if let Ok(mut completed) = self.completed.lock() {
            *completed = Some(result);
            notify(self.window, IO_DONE, 0);
        }
    }
}

fn start_worker(shared: Arc<Shared>, task: Task) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("mio-note-file-io".into())
        .spawn(move || {
            // Keep this apartment alive through the completion QueueWorkItem call.
            if let Err(error) = unsafe {
                windows::Win32::System::WinRT::RoInitialize(
                    windows::Win32::System::WinRT::RO_INIT_MULTITHREADED,
                )
            } {
                shared.finish(Err(error.to_string()));
                notify(shared.window, FAILED, 0);
                return;
            }
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ink_io::perform(task)))
                    .unwrap_or_else(|_| Err("ノートの処理中に内部エラーが発生しました。".into()));
            if let Err(error) = shared.queue(Command::Complete(result)) {
                crate::log!("file completion queue error: {error}");
                shared.finish(Err(error.to_string()));
                notify(shared.window, FAILED, 0);
            }
            unsafe {
                windows::Win32::System::WinRT::RoUninitialize();
            }
        })?;
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Pen,
    Eraser,
}

enum Command {
    Initialize {
        host: AgileReference<IInkDesktopHost>,
        window: usize,
        width: u32,
        height: u32,
        scale: f32,
        pen: PenSettings,
        page: Page,
    },
    Resize {
        width: u32,
        height: u32,
        scale: f32,
    },
    Tool(Tool),
    Pen(PenSettings),
    Page(PageAction),
    Spacing(crate::settings::RuleSpacing),
    Save {
        path: PathBuf,
        export: Option<Kind>,
    },
    Load {
        path: PathBuf,
    },
    Complete(std::result::Result<Transfer, String>),
    Stop,
}

thread_local! {
    static ENGINE: RefCell<Option<Engine>> = const { RefCell::new(None) };
}

pub struct Controller {
    _host: IInkDesktopHost,
    shared: Arc<Shared>,
    viewport: Cell<(u32, u32, f32)>,
}

impl Controller {
    pub fn new(
        notify_window: HWND,
        page: HWND,
        width: u32,
        height: u32,
        scale: f32,
        pen: PenSettings,
        settings: Page,
    ) -> Result<Self> {
        let host: IInkDesktopHost =
            unsafe { CoCreateInstance(&InkDesktopHost, None, CLSCTX_INPROC_SERVER)? };
        let shared = Arc::new(Shared {
            host: AgileReference::new(&host)?,
            window: notify_window.0 as usize,
            completed: Mutex::new(None),
        });
        let controller = Self {
            _host: host,
            shared,
            viewport: Cell::new((width, height, scale)),
        };
        controller.queue(Command::Initialize {
            host: controller.shared.host.clone(),
            window: page.0 as usize,
            width,
            height,
            scale,
            pen,
            page: settings,
        })?;
        Ok(controller)
    }

    fn queue(&self, command: Command) -> Result<()> {
        self.shared.queue(command)
    }

    pub fn resize(&self, width: u32, height: u32, scale: f32) -> Result<()> {
        if width == 0 || height == 0 || self.viewport.get() == (width, height, scale) {
            return Ok(());
        }
        self.queue(Command::Resize {
            width,
            height,
            scale,
        })?;
        self.viewport.set((width, height, scale));
        Ok(())
    }

    pub fn tool(&self, tool: Tool) -> Result<()> {
        self.queue(Command::Tool(tool))
    }

    pub fn pen(&self, settings: PenSettings) -> Result<()> {
        self.queue(Command::Pen(settings))
    }

    pub fn stop(&self) -> Result<()> {
        self.queue(Command::Stop)
    }

    pub fn save(&self, path: PathBuf) -> Result<()> {
        self.queue(Command::Save { path, export: None })
    }

    pub fn export(&self, path: PathBuf, kind: Kind) -> Result<()> {
        self.queue(Command::Save {
            path,
            export: Some(kind),
        })
    }
    pub fn page(&self, action: PageAction) -> Result<()> {
        self.queue(Command::Page(action))
    }
    pub fn spacing(&self, spacing: crate::settings::RuleSpacing) -> Result<()> {
        self.queue(Command::Spacing(spacing))
    }

    pub fn load(&self, path: PathBuf) -> Result<()> {
        self.queue(Command::Load { path })
    }

    pub fn take_completion(&self) -> Option<IoResult> {
        self.shared.completed.lock().ok()?.take()
    }
}

#[implement(IInkHostWorkItem)]
struct WorkItem {
    command: Mutex<Option<Command>>,
    notify_window: usize,
    shared: Arc<Shared>,
}

impl IInkHostWorkItem_Impl for WorkItem_Impl {
    fn Invoke(&self) -> Result<()> {
        // Never unwind through a COM callback.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let command = self
                .command
                .lock()
                .map_err(|_| Error::new(E_FAIL, "Ink command lock poisoned"))?
                .take()
                .ok_or_else(|| Error::new(E_FAIL, "Ink command already executed"))?;
            ENGINE.with(|slot| -> Result<()> {
                match command {
                    Command::Initialize {
                        host,
                        window,
                        width,
                        height,
                        scale,
                        pen,
                        page,
                    } => {
                        let engine = Engine::new(
                            &host.resolve()?,
                            HWND(window as _),
                            self.notify_window,
                            (width, height, scale),
                            pen,
                            page,
                        )?;
                        *slot.borrow_mut() = Some(engine);
                        crate::log!("Windows Ink ready; input=pen,mouse; touch=off");
                        notify(self.notify_window, READY, 0);
                    }
                    Command::Resize {
                        width,
                        height,
                        scale,
                    } => {
                        if let Some(engine) = slot.borrow_mut().as_mut() {
                            engine.resize(width, height, scale)?;
                        }
                    }
                    Command::Tool(tool) => {
                        if let Some(engine) = slot.borrow().as_ref() {
                            engine.presenter.InputProcessingConfiguration()?.SetMode(
                                match tool {
                                    Tool::Pen => InkInputProcessingMode::Inking,
                                    Tool::Eraser => InkInputProcessingMode::Erasing,
                                },
                            )?;
                            crate::log!(
                                "{}",
                                match tool {
                                    Tool::Pen => "tool=pen",
                                    Tool::Eraser => "tool=eraser",
                                }
                            );
                        }
                    }
                    Command::Pen(settings) => {
                        if let Some(engine) = slot.borrow_mut().as_mut() {
                            Engine::set_pen(&engine.presenter, settings, engine.scale)?;
                            engine.pen = settings;
                        }
                    }
                    Command::Page(action) => {
                        if let Some(engine) = slot.borrow_mut().as_mut() {
                            engine.presenter.SetIsInputEnabled(false)?;
                            let result = engine
                                .change_page(action)
                                .map(|(state, changed)| Completion::Page { state, changed })
                                .map_err(|e| e.to_string());
                            engine.presenter.SetIsInputEnabled(true)?;
                            self.shared.finish(result);
                        }
                    }
                    Command::Spacing(spacing) => {
                        if let Some(engine) = slot.borrow_mut().as_mut() {
                            engine.pages[engine.active_page].page.rule_spacing = spacing;
                        }
                    }
                    Command::Save { path, export } => {
                        if let Some(engine) = slot.borrow().as_ref() {
                            let result =
                                (|| -> std::result::Result<(), Box<dyn std::error::Error>> {
                                    engine.presenter.SetIsInputEnabled(false)?;
                                    let pages =
                                        engine.snapshot(export.is_some_and(Kind::current_only))?;
                                    start_worker(
                                        self.shared.clone(),
                                        if let Some(kind) = export {
                                            Task::Export { path, pages, kind }
                                        } else {
                                            Task::Save {
                                                path,
                                                pages,
                                                active_page: engine.active_page,
                                            }
                                        },
                                    )?;
                                    Ok(())
                                })();
                            if let Err(error) = result {
                                engine.presenter.SetIsInputEnabled(true)?;
                                self.shared.finish(Err(error.to_string()));
                            }
                        }
                    }
                    Command::Load { path } => {
                        if let Some(engine) = slot.borrow().as_ref() {
                            engine.presenter.SetIsInputEnabled(false)?;
                            if let Err(error) =
                                start_worker(self.shared.clone(), Task::Load { path })
                            {
                                engine.presenter.SetIsInputEnabled(true)?;
                                self.shared.finish(Err(error.to_string()));
                            }
                        }
                    }
                    Command::Complete(result) => {
                        if let Some(engine) = slot.borrow_mut().as_mut() {
                            let result = result.and_then(|transfer| {
                                engine.apply_transfer(transfer).map_err(|e| e.to_string())
                            });
                            engine.presenter.SetIsInputEnabled(true)?;
                            self.shared.finish(result);
                        }
                    }
                    Command::Stop => {
                        if let Some(engine) = slot.borrow_mut().take() {
                            engine.presenter.SetIsInputEnabled(false)?;
                            unsafe {
                                engine.target.SetRoot(None::<&IDCompositionVisual>)?;
                                engine.device.Commit()?;
                            }
                        }
                        crate::log!("Windows Ink stopped");
                        notify(self.notify_window, STOPPED, 0);
                    }
                }
                Ok(())
            })
        }))
        .unwrap_or_else(|_| Err(Error::new(E_FAIL, "Panic in Windows Ink work item")));
        if let Err(error) = &result {
            crate::log!("ink error: {error}");
            notify(self.notify_window, FAILED, 0);
        }
        result
    }
}

fn notify(window: usize, message: u32, value: usize) {
    unsafe {
        let _ = PostMessageW(Some(HWND(window as _)), message, WPARAM(value), LPARAM(0));
    }
}

struct Engine {
    // All these interfaces are created, accessed, and released on the ink thread.
    device: IDCompositionDevice,
    target: IDCompositionTarget,
    _visual: IDCompositionVisual,
    desktop: IInkPresenterDesktop,
    presenter: InkPresenter,
    pen: PenSettings,
    scale: f32,
    stroke_count: Arc<AtomicU32>,
    pages: Vec<InkPage>,
    active_page: usize,
    collected_token: i64,
    erased_token: i64,
}

struct InkPage {
    page: Page,
    container: InkStrokeContainer,
    scale: f32,
}

impl Engine {
    fn new(
        host: &IInkDesktopHost,
        window: HWND,
        notify_window: usize,
        viewport: (u32, u32, f32),
        pen: PenSettings,
        page: Page,
    ) -> Result<Self> {
        let (width, height, scale) = viewport;
        unsafe {
            let device: IDCompositionDevice = DCompositionCreateDevice3(None)?;
            let target = device.CreateTargetForHwnd(window, true)?;
            let visual = device.CreateVisual()?;
            target.SetRoot(&visual)?;

            let desktop: IInkPresenterDesktop = host.CreateInkPresenter()?;
            let presenter: InkPresenter = desktop.cast()?;
            presenter
                .SetInputDeviceTypes(CoreInputDeviceTypes::Pen | CoreInputDeviceTypes::Mouse)?;
            presenter
                .InputConfiguration()?
                .SetIsEraserInputEnabled(true)?;
            Self::set_pen(&presenter, pen, scale)?;

            let stroke_count = Arc::new(AtomicU32::new(0));
            let collected_count = stroke_count.clone();
            let collected_token =
                presenter.StrokesCollected(&TypedEventHandler::<
                    InkPresenter,
                    InkStrokesCollectedEventArgs,
                >::new(move |_, args| {
                    if let Some(args) = args.as_ref() {
                        let added = args.Strokes()?.Size()?;
                        let previous = collected_count
                            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
                                Some(count.saturating_add(added))
                            })
                            .unwrap_or_else(|count| count);
                        let count = previous.saturating_add(added);
                        notify(notify_window, STROKES, count as usize);
                    }
                    Ok(())
                }))?;
            let erased_count = stroke_count.clone();
            let erased_token = presenter.StrokesErased(&TypedEventHandler::<
                InkPresenter,
                InkStrokesErasedEventArgs,
            >::new(move |_, args| {
                if let Some(args) = args.as_ref() {
                    let erased = args.Strokes()?.Size()?;
                    let previous = erased_count
                        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
                            Some(count.saturating_sub(erased))
                        })
                        .unwrap_or_else(|count| count);
                    let count = previous.saturating_sub(erased);
                    notify(notify_window, STROKES, count as usize);
                }
                Ok(())
            }))?;

            desktop.SetRootVisual(&visual, &device)?;
            desktop.SetSize(width as f32, height as f32)?;
            presenter.SetIsInputEnabled(true)?;
            device.Commit()?;
            let container = presenter.StrokeContainer()?;
            Ok(Self {
                device,
                target,
                _visual: visual,
                desktop,
                presenter,
                pen,
                scale,
                stroke_count,
                pages: vec![InkPage {
                    page,
                    container,
                    scale,
                }],
                active_page: 0,
                collected_token,
                erased_token,
            })
        }
    }

    fn set_pen(presenter: &InkPresenter, settings: PenSettings, scale: f32) -> Result<()> {
        let attributes = InkDrawingAttributes::new()?;
        attributes.SetColor(Color {
            A: 255,
            R: 31,
            G: 49,
            B: 69,
        })?;
        attributes.SetSize(Size {
            Width: settings.width.size() * scale,
            Height: settings.width.size() * scale,
        })?;
        attributes.SetPenTip(PenTipShape::Circle)?;
        attributes.SetIgnorePressure(!settings.pressure)?;
        presenter.UpdateDefaultDrawingAttributes(&attributes)?;
        crate::log!(
            "pen_width={:.1} scale={scale:.2} applied_size={:.2} pressure={}",
            settings.width.size(),
            attributes.Size()?.Width,
            settings.pressure
        );
        Ok(())
    }

    fn resize(&mut self, width: u32, height: u32, scale: f32) -> Result<()> {
        if width == 0 || height == 0 {
            return Ok(());
        }
        if self.scale != scale {
            ink_io::scale_strokes(&self.presenter.StrokeContainer()?, scale / self.scale)?;
            Self::set_pen(&self.presenter, self.pen, scale)?;
            self.scale = scale;
            self.pages[self.active_page].scale = scale;
        }
        unsafe {
            self.desktop.SetSize(width as f32, height as f32)?;
            self.device.Commit()?;
        }
        crate::log!("resize {width}x{height} scale={scale:.2}");
        Ok(())
    }

    fn snapshot(&self, current_only: bool) -> Result<Vec<PageSnapshot>> {
        let pages = if current_only {
            std::slice::from_ref(&self.pages[self.active_page])
        } else {
            &self.pages
        };
        pages
            .iter()
            .map(|page| {
                Ok(PageSnapshot {
                    page: page.page,
                    strokes: ink_io::clone_strokes(&page.container)?,
                    scale: page.scale,
                })
            })
            .collect()
    }

    fn page_info(&self) -> Result<PageInfo> {
        Ok(PageInfo {
            index: self.active_page,
            total: self.pages.len(),
            page: self.pages[self.active_page].page,
            count: self.presenter.StrokeContainer()?.GetStrokes()?.Size()?,
        })
    }

    fn change_page(&mut self, action: PageAction) -> Result<(PageInfo, bool)> {
        let changed = match action {
            PageAction::Add => {
                if self.pages.len() >= crate::note::MAX_PAGES {
                    return Err(Error::new(E_FAIL, "ページ数の上限に達しました。"));
                }
                let container = InkStrokeContainer::new()?;
                let page = self.pages[self.active_page].page;
                self.presenter.SetStrokeContainer(&container)?;
                self.pages.push(InkPage {
                    page,
                    container,
                    scale: self.scale,
                });
                self.active_page = self.pages.len() - 1;
                true
            }
            PageAction::Go(index) => {
                if index >= self.pages.len() {
                    return Err(Error::new(E_FAIL, "ページがありません。"));
                }
                let target = &mut self.pages[index];
                ink_io::scale_strokes(&target.container, self.scale / target.scale)?;
                target.scale = self.scale;
                self.presenter.SetStrokeContainer(&target.container)?;
                self.active_page = index;
                false
            }
            PageAction::Delete => {
                let presenter = &self.presenter;
                let scale = self.scale;
                delete_current_page(&mut self.pages, &mut self.active_page, |target| {
                    ink_io::scale_strokes(&target.container, scale / target.scale)?;
                    target.scale = scale;
                    presenter.SetStrokeContainer(&target.container)
                })?;
                true
            }
            PageAction::Paper(paper) => {
                let target = &mut self.pages[self.active_page];
                let bounds = target.container.BoundingRect()?;
                let right = (bounds.X + bounds.Width).max(0.0) / target.scale;
                let bottom = (bounds.Y + bounds.Height).max(0.0) / target.scale;
                let ratio = paper.fit_content(right, bottom);
                ink_io::scale_strokes(&target.container, ratio)?;
                let changed = target.page.paper != paper;
                target.page.paper = paper;
                changed
            }
        };
        self.presenter
            .InputProcessingConfiguration()?
            .SetMode(InkInputProcessingMode::Inking)?;
        let info = self.page_info()?;
        self.stroke_count.store(info.count, Ordering::Relaxed);
        Ok((info, changed))
    }

    fn apply_transfer(&mut self, transfer: Transfer) -> Result<Completion> {
        match transfer {
            Transfer::Saved { path } => {
                crate::log!("note saved");
                Ok(Completion::Saved { path })
            }
            Transfer::Loaded {
                path,
                pages,
                active_page,
            } => {
                // Validate every page before replacing any live state.
                let mut prepared = Vec::with_capacity(pages.len());
                for (index, snapshot) in pages.into_iter().enumerate() {
                    let scale = if index == active_page {
                        self.scale
                    } else {
                        1.0
                    };
                    let container = ink_io::container(snapshot.strokes)?;
                    ink_io::scale_strokes(&container, scale / snapshot.scale)?;
                    prepared.push(InkPage {
                        page: snapshot.page,
                        container,
                        scale,
                    });
                }
                self.presenter
                    .InputProcessingConfiguration()?
                    .SetMode(InkInputProcessingMode::Inking)?;
                self.presenter
                    .SetStrokeContainer(&prepared[active_page].container)?;
                self.pages = prepared;
                self.active_page = active_page;
                let state = self.page_info()?;
                self.stroke_count.store(state.count, Ordering::Relaxed);
                crate::log!("note loaded; pages={}", state.total);
                Ok(Completion::Loaded { path, state })
            }
            Transfer::Exported { path, kind } => Ok(Completion::Exported { path, kind }),
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.presenter.RemoveStrokesCollected(self.collected_token);
        let _ = self.presenter.RemoveStrokesErased(self.erased_token);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_deletion_selects_next_or_previous_and_preserves_order() -> Result<()> {
        for (index, next) in [(0, 20), (1, 30), (2, 20)] {
            let mut pages = vec![10, 20, 30];
            let mut active = index;
            let mut attached = None;
            delete_current_page(&mut pages, &mut active, |page| {
                attached = Some(*page);
                Ok(())
            })?;
            assert_eq!(attached, Some(next));
            assert_eq!(pages[active], next);
            let expected: Vec<_> = [10, 20, 30]
                .into_iter()
                .enumerate()
                .filter_map(|(i, page)| (i != index).then_some(page))
                .collect();
            assert_eq!(pages, expected);
        }
        Ok(())
    }

    #[test]
    fn page_deletion_preserves_pages_on_switch_failure_and_rejects_the_last_page() {
        let mut pages = vec![10, 20, 30];
        let mut active = 1;
        assert!(
            delete_current_page(&mut pages, &mut active, |_| Err(Error::new(
                E_FAIL,
                "test switch failure"
            )))
            .is_err()
        );
        assert_eq!(pages, [10, 20, 30]);
        assert_eq!(active, 1);
        let mut pages = vec![42];
        let mut active = 0;
        assert!(
            delete_current_page(&mut pages, &mut active, |_| panic!(
                "must not switch the last page"
            ))
            .is_err()
        );
        assert_eq!(pages, [42]);
        assert_eq!(active, 0);
    }
}
