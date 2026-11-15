//! The real stdout, captured once so UI work can swap the process handles.
//!
//! `wbat64` draws on the console device, but results still go to the stdout the
//! caller redirected. The handle is duplicated at startup, so it keeps working
//! even while the TUI has pointed `STD_OUTPUT_HANDLE` at `CONOUT$`.

use std::fs::File;
use std::io::Write;

use wbat64_core::{Error, Result};

/// The captured standard output.
pub struct Output {
    file: Option<File>,
    path: Option<String>,
    /// stdout is a console: write UTF-16 instead of code-page bytes.
    console: bool,
}

impl Output {
    pub fn capture() -> Output {
        #[cfg(windows)]
        {
            if let Some((file, path, console)) = capture_windows_stdout() {
                return Output {
                    file: Some(file),
                    path,
                    console,
                };
            }
        }
        #[cfg(unix)]
        {
            if let Some((file, path)) = capture_unix_stdout() {
                let console = unsafe { libc::isatty(1) == 1 };
                return Output {
                    file: Some(file),
                    path,
                    console,
                };
            }
        }
        Output {
            file: None,
            path: None,
            console: false,
        }
    }

    pub fn write_str(&mut self, text: &str) -> Result<()> {
        #[cfg(windows)]
        if self.console {
            if let Some(file) = &self.file {
                use std::os::windows::io::AsRawHandle;
                if write_console_utf16(file.as_raw_handle(), text) {
                    return Ok(());
                }
            }
        }
        match &mut self.file {
            Some(file) => file
                .write_all(text.as_bytes())
                .map_err(|e| Error::io("writing results", e)),
            None => {
                let mut stdout = std::io::stdout();
                stdout
                    .write_all(text.as_bytes())
                    .map_err(|e| Error::io("writing results", e))
            }
        }
    }

    /// Write raw bytes (used for OEM-encoded batch output).
    pub fn write_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        match &mut self.file {
            Some(file) => file
                .write_all(bytes)
                .map_err(|e| Error::io("writing results", e)),
            None => std::io::stdout()
                .write_all(bytes)
                .map_err(|e| Error::io("writing results", e)),
        }
    }

    /// True when stdout is a console (as opposed to a file or a pipe).
    pub fn is_console(&self) -> bool {
        self.console
    }

    pub fn flush(&mut self) -> Result<()> {
        match &mut self.file {
            Some(file) => file.flush().map_err(|e| Error::io("flushing results", e)),
            None => std::io::stdout()
                .flush()
                .map_err(|e| Error::io("flushing results", e)),
        }
    }

    /// True when stdout is redirected to a file whose name ends in `.bat`.
    pub fn stdout_is_bat(&self) -> bool {
        self.path
            .as_deref()
            .map(|p| p.to_ascii_lowercase().ends_with(".bat"))
            .unwrap_or(false)
    }

    /// The path stdout points at, when it is a regular file.
    pub fn stdout_path(&self) -> Option<&str> {
        self.path.as_deref()
    }
}

#[cfg(windows)]
fn capture_windows_stdout() -> Option<(File, Option<String>, bool)> {
    use std::ffi::c_void;
    use std::os::windows::io::FromRawHandle;

    type Handle = *mut c_void;
    const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
    const DUPLICATE_SAME_ACCESS: u32 = 2;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetStdHandle(which: u32) -> Handle;
        fn DuplicateHandle(
            process: Handle,
            source: Handle,
            target: Handle,
            target_handle: *mut Handle,
            access: u32,
            inherit: i32,
            options: u32,
        ) -> i32;
        fn GetCurrentProcess() -> Handle;
        fn GetFinalPathNameByHandleW(
            handle: Handle,
            buffer: *mut u16,
            length: u32,
            flags: u32,
        ) -> u32;
        fn GetFileInformationByHandleEx(
            handle: Handle,
            class: i32,
            buffer: *mut c_void,
            size: u32,
        ) -> i32;
        fn GetFileType(handle: Handle) -> u32;
        fn GetLastError() -> u32;
        fn GetConsoleMode(handle: Handle, mode: *mut u32) -> i32;
    }

    unsafe {
        let handle = GetStdHandle(STD_OUTPUT_HANDLE);
        if handle.is_null() || handle == (-1isize as Handle) {
            return None;
        }
        let mut mode: u32 = 0;
        let is_console = GetConsoleMode(handle, &mut mode) != 0;
        let mut duplicate: Handle = std::ptr::null_mut();
        let ok = DuplicateHandle(
            GetCurrentProcess(),
            handle,
            GetCurrentProcess(),
            &mut duplicate,
            0,
            0,
            DUPLICATE_SAME_ACCESS,
        );
        if ok == 0 || duplicate.is_null() {
            return None;
        }
        // Determine the file name, if stdout is a regular file.
        let mut buffer = [0u16; 1024];
        let mut path = None;
        let len = GetFinalPathNameByHandleW(duplicate, buffer.as_mut_ptr(), 1024, 0);
        if len > 0 && (len as usize) < buffer.len() {
            let name = String::from_utf16_lossy(&buffer[..len as usize]);
            path = Some(name.strip_prefix(r"\\?\").unwrap_or(&name).to_string());
        } else {
            // A handle opened write-only (common for `> file`) may not allow
            // GetFinalPathNameByHandleW; FileNameInfo usually still works.
            let mut info = vec![0u8; 4096];
            let ok = GetFileInformationByHandleEx(
                duplicate,
                2, // FileNameInfo
                info.as_mut_ptr() as *mut c_void,
                info.len() as u32,
            );
            if ok != 0 {
                let name_len = u32::from_ne_bytes([info[0], info[1], info[2], info[3]]) as usize;
                let words: Vec<u16> = info[4..4 + name_len.min(info.len() - 4)]
                    .chunks_exact(2)
                    .map(|pair| u16::from_ne_bytes([pair[0], pair[1]]))
                    .collect();
                let name = String::from_utf16_lossy(&words);
                if !name.is_empty() {
                    path = Some(name.trim_start_matches(r"\\?\").to_string());
                }
            }
        }
        if std::env::var_os("WBAT64_DEBUG").is_some() {
            eprintln!(
                "wbat64: debug: stdout path = {path:?} handle={handle:?} dup={duplicate:?} type={} last={}",
                GetFileType(duplicate),
                GetLastError()
            );
        }
        let file = File::from_raw_handle(duplicate as std::os::windows::io::RawHandle);
        Some((file, path, is_console))
    }
}

/// Write results to a console handle as UTF-16 (code-page independent).
#[cfg(windows)]
fn write_console_utf16(handle: *mut std::ffi::c_void, text: &str) -> bool {
    use std::ffi::c_void;

    #[link(name = "kernel32")]
    extern "system" {
        fn WriteConsoleW(
            handle: *mut c_void,
            buffer: *const u16,
            length: u32,
            written: *mut u32,
            reserved: *mut c_void,
        ) -> i32;
    }

    let wide: Vec<u16> = text.encode_utf16().collect();
    let mut written: u32 = 0;
    unsafe {
        WriteConsoleW(
            handle,
            wide.as_ptr(),
            wide.len() as u32,
            &mut written,
            std::ptr::null_mut(),
        ) != 0
    }
}

#[cfg(unix)]
fn capture_unix_stdout() -> Option<(File, Option<String>)> {
    use std::os::unix::io::FromRawFd;
    unsafe {
        let fd = libc::dup(1);
        if fd < 0 {
            return None;
        }
        let path = std::fs::read_link("/proc/self/fd/1")
            .ok()
            .map(|p| p.display().to_string());
        Some((File::from_raw_fd(fd), path))
    }
}
