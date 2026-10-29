#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![deny(unsafe_op_in_unsafe_fn)]

mod drawing;
mod export;
mod file_dialog;
mod ink;
mod ink_io;
mod isf;
mod note;
mod paper;
mod paths;
mod pdf;
mod png;
mod settings;
mod storage;
mod svg;
mod toolbar;
mod ui;
mod validation;
mod view;

use std::{
    fmt,
    fs::File,
    io::Write,
    sync::{Mutex, OnceLock},
    time::Instant,
};
use windows::{
    Win32::System::{
        LibraryLoader::{LOAD_LIBRARY_SEARCH_SYSTEM32, SetDefaultDllDirectories},
        WinRT::{RO_INIT_SINGLETHREADED, RoInitialize, RoUninitialize},
    },
    core::Result,
};

static START: OnceLock<Instant> = OnceLock::new();
static LOG: OnceLock<Mutex<DiagnosticLog>> = OnceLock::new();
const MAX_LOG_BYTES: usize = 1024 * 1024;
struct DiagnosticLog {
    file: File,
    written: usize,
}

pub(crate) fn logging_enabled() -> bool {
    LOG.get().is_some()
}

// Test the flag before evaluating arguments: normal handwriting allocates no
// diagnostic strings and never waits for a file or the logger's mutex.
macro_rules! log {
    ($($args:tt)*) => {
        if $crate::logging_enabled() {
            $crate::write_log(format_args!($($args)*));
        }
    };
}
pub(crate) use log;

pub(crate) fn write_log(message: fmt::Arguments<'_>) {
    if let Some(file) = LOG.get()
        && let Ok(mut file) = file.lock()
    {
        let elapsed = START
            .get()
            .map_or(0.0, |start| start.elapsed().as_secs_f64() * 1000.0);
        let line = format!("{elapsed:>10.3} ms  {message}\n");
        if line.len() <= MAX_LOG_BYTES.saturating_sub(file.written) {
            match file.file.write_all(line.as_bytes()) {
                Ok(()) => file.written += line.len(),
                Err(_) => file.written = MAX_LOG_BYTES,
            }
        }
    }
}

fn run() -> Result<()> {
    // The UI owns the host. All InkPresenter work is queued to its ink thread.
    unsafe {
        SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32)?;
        RoInitialize(RO_INIT_SINGLETHREADED)?;
    }
    initialize_log();
    let result = ui::run();
    unsafe {
        RoUninitialize();
    }
    result
}

fn initialize_log() {
    if std::env::var_os("MIONOTE_DIAGNOSTICS").as_deref() == Some(std::ffi::OsStr::new("1"))
        && let Ok(dir) = paths::data_directory()
        && std::fs::create_dir_all(&dir).is_ok()
        && let Ok(file) = storage::replace_file(&dir.join("mio-note.log"), |_| Ok(()))
    {
        let _ = LOG.set(Mutex::new(DiagnosticLog { file, written: 0 }));
    }
    log!("start architecture={}", std::env::consts::ARCH);
}

fn main() {
    START.get_or_init(Instant::now);
    if let Err(error) = run() {
        log!("fatal: {error}");
        ui::show_error(&error);
        std::process::exit(1);
    }
    log!("exit");
}
