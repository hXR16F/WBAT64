//! Console smoke test: open the real console device, write Unicode + VT, read
//! the cells back, and restore.
//!
//! Run with `cargo run -p wbat64-tui --example console_probe` from a terminal.
//! It is intentionally not part of the automated test suite: CI has no console.

use std::ffi::c_void;
use std::time::Duration;

use wbat64_tui::term::{Backend, ConsoleBackend};

type Handle = *mut c_void;

#[repr(C)]
struct Coord {
    x: i16,
    y: i16,
}

#[link(name = "kernel32")]
extern "system" {
    fn GetStdHandle(which: u32) -> Handle;
    fn ReadConsoleOutputCharacterW(
        handle: Handle,
        buffer: *mut u16,
        length: u32,
        start: Coord,
        read: *mut u32,
    ) -> i32;
}

const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;

fn read_cells(count: usize) -> Option<String> {
    let mut buffer = vec![0u16; count];
    let mut read: u32 = 0;
    unsafe {
        let handle = GetStdHandle(STD_OUTPUT_HANDLE);
        if ReadConsoleOutputCharacterW(
            handle,
            buffer.as_mut_ptr(),
            count as u32,
            Coord { x: 0, y: 0 },
            &mut read,
        ) == 0
        {
            return None;
        }
    }
    Some(String::from_utf16_lossy(&buffer[..read as usize]))
}

fn main() {
    let mut backend = match ConsoleBackend::new(false) {
        Ok(backend) => backend,
        Err(error) => {
            eprintln!("console_probe: {error}");
            std::process::exit(1);
        }
    };
    let size = backend.size().expect("size");

    let expected = "┌─┐✓日";
    backend.write_str("\x1b[1;1H").expect("move");
    backend.write_str(expected).expect("write unicode");
    backend.flush().expect("flush");

    let read_back = read_cells(expected.chars().count() + 2);
    let event = backend.poll_event(Duration::from_millis(50));
    let cursor = backend.cursor_position();
    drop(backend);

    let ok = read_back
        .as_deref()
        .map(|text| text.starts_with(expected))
        .unwrap_or(false);
    println!("size={size:?} cursor={cursor:?} event={event:?} read_back={read_back:?} ok={ok}");
    if !ok {
        eprintln!("console_probe: Unicode round-trip failed");
        std::process::exit(1);
    }
}
