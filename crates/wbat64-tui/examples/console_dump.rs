//! Read the visible console window and print it as plain text.
//!
//! Useful to verify what the terminal actually displays after a `fix` box:
//!   wbat64 fix box 2,2 "Unicode ✓ 日本語" " OK " --timeout 1
//!   cargo run -p wbat64-tui --example console_dump
//!
//! It opens `CONOUT$` directly, so it works even when stdout is redirected.

use std::ffi::c_void;

type Handle = *mut c_void;

#[repr(C)]
struct Coord {
    x: i16,
    y: i16,
}

#[repr(C)]
struct SmallRect {
    left: i16,
    top: i16,
    right: i16,
    bottom: i16,
}

#[repr(C)]
struct ScreenBufferInfo {
    size: Coord,
    cursor: Coord,
    attributes: u16,
    window: SmallRect,
    maximum_window_size: Coord,
}

#[link(name = "kernel32")]
extern "system" {
    fn CreateFileW(
        name: *const u16,
        access: u32,
        share: u32,
        security: *mut c_void,
        disposition: u32,
        flags: u32,
        template: Handle,
    ) -> Handle;
    fn GetConsoleScreenBufferInfo(handle: Handle, info: *mut ScreenBufferInfo) -> i32;
    fn ReadConsoleOutputCharacterW(
        handle: Handle,
        buffer: *mut u16,
        length: u32,
        start: Coord,
        read: *mut u32,
    ) -> i32;
    fn CloseHandle(handle: Handle) -> i32;
}

const GENERIC_READ: u32 = 0x8000_0000;
const GENERIC_WRITE: u32 = 0x4000_0000;
const FILE_SHARE_READ: u32 = 1;
const FILE_SHARE_WRITE: u32 = 2;
const OPEN_EXISTING: u32 = 3;
const INVALID_HANDLE_VALUE: Handle = -1isize as Handle;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn main() {
    unsafe {
        let name = wide("CONOUT$");
        let handle = CreateFileW(
            name.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        );
        if handle == INVALID_HANDLE_VALUE {
            eprintln!("console_dump: cannot open CONOUT$");
            std::process::exit(1);
        }
        let mut info: ScreenBufferInfo = std::mem::zeroed();
        if GetConsoleScreenBufferInfo(handle, &mut info) == 0 {
            eprintln!("console_dump: GetConsoleScreenBufferInfo failed");
            CloseHandle(handle);
            std::process::exit(1);
        }
        let width = info.size.x.max(1) as usize;
        let rows = (info.window.bottom - info.window.top + 1).max(1) as usize;
        let top = info.window.top;

        for row in 0..rows {
            let mut buffer = vec![0u16; width];
            let mut read: u32 = 0;
            if ReadConsoleOutputCharacterW(
                handle,
                buffer.as_mut_ptr(),
                width as u32,
                Coord {
                    x: 0,
                    y: top + row as i16,
                },
                &mut read,
            ) == 0
            {
                break;
            }
            let line = String::from_utf16_lossy(&buffer[..read as usize]);
            println!("{:>2}|{}", top + row as i16 + 1, line.trim_end());
        }
        CloseHandle(handle);
    }
}
