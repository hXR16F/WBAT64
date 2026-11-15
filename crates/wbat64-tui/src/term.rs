//! The console device: a backend that owns the terminal without using stdout.
//!
//! wbat64 draws on `CONOUT$` / `/dev/tty` and reads keys from `CONIN$` /
//! `/dev/tty`, so stdout stays available for results. On Windows the process
//! std handles are temporarily pointed at the console (that is what crossterm
//! and the Win32 console APIs consult) and restored when the session ends.

use std::collections::VecDeque;
use std::time::Duration;

use wbat64_core::{char_width, Color, Error, Result, Style};
use wbat64_render::Screen;

use crate::event::{Event, Key, KeyCode, Mods, MouseButton, MouseKind};

/// A terminal backend.
pub trait Backend {
    fn size(&mut self) -> Result<(u16, u16)>;
    fn write_str(&mut self, text: &str) -> Result<()>;
    fn flush(&mut self) -> Result<()>;
    fn poll_event(&mut self, timeout: Duration) -> Result<Option<Event>>;
    fn cursor_position(&mut self) -> Option<(u16, u16)>;
    fn is_headless(&self) -> bool {
        false
    }
    fn supports_ansi(&self) -> bool {
        true
    }
    fn dump(&self) -> Option<String> {
        None
    }
    /// Read the visible screen if the backend can (Win32 console only).
    ///
    /// This lets a dialog restore the real background exactly when it moves or
    /// closes, instead of blanking cells wbat64 did not paint itself.
    fn capture_screen(&mut self) -> Option<Screen> {
        None
    }
}

// ---------------------------------------------------------------------------
// Windows console plumbing
// ---------------------------------------------------------------------------

#[cfg(windows)]
mod win {
    use std::ffi::c_void;

    pub type Handle = *mut c_void;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct Coord {
        pub x: i16,
        pub y: i16,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct SmallRect {
        pub left: i16,
        pub top: i16,
        pub right: i16,
        pub bottom: i16,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct ConsoleScreenBufferInfo {
        pub size: Coord,
        pub cursor: Coord,
        pub attributes: u16,
        pub window: SmallRect,
        pub maximum_window_size: Coord,
    }

    pub const INVALID_HANDLE_VALUE: Handle = -1isize as Handle;
    pub const STD_INPUT_HANDLE: u32 = -10i32 as u32;
    pub const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
    pub const GENERIC_READ: u32 = 0x8000_0000;
    pub const GENERIC_WRITE: u32 = 0x4000_0000;
    pub const FILE_SHARE_READ: u32 = 0x0000_0001;
    pub const FILE_SHARE_WRITE: u32 = 0x0000_0002;
    pub const OPEN_EXISTING: u32 = 3;
    pub const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x0004;
    pub const ENABLE_MOUSE_INPUT: u32 = 0x0010;
    pub const ENABLE_QUICK_EDIT_MODE: u32 = 0x0040;
    pub const ENABLE_EXTENDED_FLAGS: u32 = 0x0080;

    #[link(name = "kernel32")]
    extern "system" {
        pub fn GetStdHandle(which: u32) -> Handle;
        pub fn SetStdHandle(which: u32, handle: Handle) -> i32;
        pub fn CreateFileW(
            name: *const u16,
            access: u32,
            share: u32,
            security: *mut c_void,
            disposition: u32,
            flags: u32,
            template: Handle,
        ) -> Handle;
        pub fn GetConsoleMode(handle: Handle, mode: *mut u32) -> i32;
        pub fn SetConsoleMode(handle: Handle, mode: u32) -> i32;
        pub fn CloseHandle(handle: Handle) -> i32;
        pub fn WriteConsoleW(
            handle: Handle,
            buffer: *const u16,
            length: u32,
            written: *mut u32,
            reserved: *mut c_void,
        ) -> i32;
        pub fn GetConsoleScreenBufferInfo(
            handle: Handle,
            info: *mut ConsoleScreenBufferInfo,
        ) -> i32;
        pub fn ReadConsoleOutputCharacterW(
            handle: Handle,
            buffer: *mut u16,
            length: u32,
            start: Coord,
            read: *mut u32,
        ) -> i32;
        pub fn ReadConsoleOutputAttribute(
            handle: Handle,
            buffer: *mut u16,
            length: u32,
            start: Coord,
            read: *mut u32,
        ) -> i32;
    }

    pub fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }
}

#[cfg(windows)]
struct WindowsHandles {
    saved_in: win::Handle,
    saved_out: win::Handle,
    in_handle: win::Handle,
    out_handle: win::Handle,
    /// Input mode before raw mode/mouse capture, restored on drop.
    saved_in_mode: Option<u32>,
}

#[cfg(windows)]
unsafe fn open_windows_console() -> std::io::Result<(std::fs::File, WindowsHandles)> {
    use std::os::windows::io::FromRawHandle;

    let out_name = win::wide("CONOUT$");
    let in_name = win::wide("CONIN$");
    let access = win::GENERIC_READ | win::GENERIC_WRITE;
    let share = win::FILE_SHARE_READ | win::FILE_SHARE_WRITE;

    let out = win::CreateFileW(
        out_name.as_ptr(),
        access,
        share,
        std::ptr::null_mut(),
        win::OPEN_EXISTING,
        0,
        std::ptr::null_mut(),
    );
    if out == win::INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error());
    }
    let input = win::CreateFileW(
        in_name.as_ptr(),
        access,
        share,
        std::ptr::null_mut(),
        win::OPEN_EXISTING,
        0,
        std::ptr::null_mut(),
    );
    if input == win::INVALID_HANDLE_VALUE {
        let err = std::io::Error::last_os_error();
        win::CloseHandle(out);
        return Err(err);
    }

    let saved_out = win::GetStdHandle(win::STD_OUTPUT_HANDLE);
    let saved_in = win::GetStdHandle(win::STD_INPUT_HANDLE);
    let mut input_mode: u32 = 0;
    let saved_in_mode = if win::GetConsoleMode(input, &mut input_mode) != 0 {
        Some(input_mode)
    } else {
        None
    };
    win::SetStdHandle(win::STD_OUTPUT_HANDLE, out);
    win::SetStdHandle(win::STD_INPUT_HANDLE, input);

    // Turn on VT processing for the console output.
    let mut mode: u32 = 0;
    if win::GetConsoleMode(out, &mut mode) != 0 {
        win::SetConsoleMode(out, mode | win::ENABLE_VIRTUAL_TERMINAL_PROCESSING);
    }

    let file = std::fs::File::from_raw_handle(out as std::os::windows::io::RawHandle);
    Ok((
        file,
        WindowsHandles {
            saved_in,
            saved_out,
            in_handle: input,
            out_handle: out,
            saved_in_mode,
        },
    ))
}

// ---------------------------------------------------------------------------
// Unix tty plumbing
// ---------------------------------------------------------------------------

#[cfg(unix)]
struct UnixHandles {
    saved_in: libc::c_int,
    saved_out: libc::c_int,
}

#[cfg(unix)]
unsafe fn open_unix_tty() -> std::io::Result<(std::fs::File, UnixHandles)> {
    use std::os::unix::io::FromRawFd;

    let path = std::ffi::CString::new("/dev/tty").unwrap();
    let fd = libc::open(path.as_ptr(), libc::O_RDWR);
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let saved_in = libc::dup(0);
    let saved_out = libc::dup(1);
    if saved_in < 0 || saved_out < 0 {
        libc::close(fd);
        return Err(std::io::Error::last_os_error());
    }
    libc::dup2(fd, 0);
    libc::dup2(fd, 1);
    let file = std::fs::File::from_raw_fd(fd);
    Ok((
        file,
        UnixHandles {
            saved_in,
            saved_out,
        },
    ))
}

// ---------------------------------------------------------------------------
// Console backend
// ---------------------------------------------------------------------------

/// The real terminal.
pub struct ConsoleBackend {
    out: std::fs::File,
    #[cfg(windows)]
    handles: WindowsHandles,
    #[cfg(unix)]
    handles: UnixHandles,
    mouse: bool,
    size: (u16, u16),
}

impl ConsoleBackend {
    pub fn new(mouse: bool) -> Result<ConsoleBackend> {
        #[cfg(windows)]
        let (out, handles) = unsafe { open_windows_console() }
            .map_err(|e| Error::msg(format!("no console available ({e})")))?;
        #[cfg(unix)]
        let (out, handles) = unsafe { open_unix_tty() }
            .map_err(|e| Error::msg(format!("no terminal available ({e})")))?;

        crossterm::terminal::enable_raw_mode()
            .map_err(|e| Error::msg(format!("cannot enter raw mode ({e})")))?;
        let size = crossterm::terminal::size().unwrap_or((80, 25));

        let mut backend = ConsoleBackend {
            out,
            handles,
            mouse: false,
            size,
        };
        if mouse {
            backend.set_mouse(true);
        }
        Ok(backend)
    }

    fn set_mouse(&mut self, enabled: bool) {
        self.mouse = enabled;
        #[cfg(windows)]
        unsafe {
            let mut mode: u32 = 0;
            if win::GetConsoleMode(self.handles.in_handle, &mut mode) != 0 {
                if enabled {
                    mode |= win::ENABLE_MOUSE_INPUT | win::ENABLE_EXTENDED_FLAGS;
                    mode &= !win::ENABLE_QUICK_EDIT_MODE;
                } else {
                    mode &= !win::ENABLE_MOUSE_INPUT;
                }
                win::SetConsoleMode(self.handles.in_handle, mode);
            }
        }
        #[cfg(unix)]
        {
            use std::io::Write;
            let seq = if enabled {
                "\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1006h"
            } else {
                "\x1b[?1006l\x1b[?1003l\x1b[?1002l\x1b[?1000l"
            };
            let _ = self.out.write_all(seq.as_bytes());
            let _ = self.out.flush();
        }
        let _ = enabled;
    }

    fn convert(event: crossterm::event::Event) -> Option<Event> {
        use crossterm::event as ct;
        match event {
            ct::Event::Key(key) => {
                if matches!(key.kind, ct::KeyEventKind::Release) {
                    return None;
                }
                let code = match key.code {
                    ct::KeyCode::Char(c) => KeyCode::Char(c),
                    ct::KeyCode::Enter => KeyCode::Enter,
                    ct::KeyCode::Esc => KeyCode::Esc,
                    ct::KeyCode::Tab => KeyCode::Tab,
                    ct::KeyCode::BackTab => KeyCode::BackTab,
                    ct::KeyCode::Backspace => KeyCode::Backspace,
                    ct::KeyCode::Delete => KeyCode::Delete,
                    ct::KeyCode::Insert => KeyCode::Insert,
                    ct::KeyCode::Left => KeyCode::Left,
                    ct::KeyCode::Right => KeyCode::Right,
                    ct::KeyCode::Up => KeyCode::Up,
                    ct::KeyCode::Down => KeyCode::Down,
                    ct::KeyCode::Home => KeyCode::Home,
                    ct::KeyCode::End => KeyCode::End,
                    ct::KeyCode::PageUp => KeyCode::PageUp,
                    ct::KeyCode::PageDown => KeyCode::PageDown,
                    ct::KeyCode::F(n) => KeyCode::F(n),
                    _ => KeyCode::Unknown,
                };
                let mut mods = Mods::NONE;
                mods.shift = key.modifiers.contains(ct::KeyModifiers::SHIFT);
                mods.ctrl = key.modifiers.contains(ct::KeyModifiers::CONTROL);
                mods.alt = key.modifiers.contains(ct::KeyModifiers::ALT);
                Some(Event::Key(Key { code, mods }))
            }
            ct::Event::Mouse(mouse) => {
                let button = match mouse.kind {
                    ct::MouseEventKind::Down(b) => Some((b, true)),
                    ct::MouseEventKind::Up(b) => Some((b, false)),
                    _ => None,
                };
                if let Some((b, down)) = button {
                    let button = match b {
                        ct::MouseButton::Left => MouseButton::Left,
                        ct::MouseButton::Right => MouseButton::Right,
                        ct::MouseButton::Middle => MouseButton::Middle,
                    };
                    return Some(Event::Mouse(if down {
                        MouseKind::Down {
                            x: mouse.column,
                            y: mouse.row,
                            button,
                        }
                    } else {
                        MouseKind::Up {
                            x: mouse.column,
                            y: mouse.row,
                            button,
                        }
                    }));
                }
                match mouse.kind {
                    ct::MouseEventKind::Drag(ct::MouseButton::Left) => {
                        Some(Event::Mouse(MouseKind::Drag {
                            x: mouse.column,
                            y: mouse.row,
                        }))
                    }
                    ct::MouseEventKind::Moved => Some(Event::Mouse(MouseKind::Move {
                        x: mouse.column,
                        y: mouse.row,
                    })),
                    ct::MouseEventKind::ScrollUp => Some(Event::Mouse(MouseKind::Wheel {
                        x: mouse.column,
                        y: mouse.row,
                        up: true,
                    })),
                    ct::MouseEventKind::ScrollDown => Some(Event::Mouse(MouseKind::Wheel {
                        x: mouse.column,
                        y: mouse.row,
                        up: false,
                    })),
                    _ => None,
                }
            }
            ct::Event::Resize(cols, rows) => Some(Event::Resize(cols, rows)),
            _ => None,
        }
    }
}

impl ConsoleBackend {
    /// Write text to the console as UTF-16.
    ///
    /// The console interprets raw bytes in its active OEM code page, so UTF-8
    /// box-drawing characters would be split into several glyphs and the cursor
    /// would advance further than wbat64's cell model. `WriteConsoleW` avoids the
    /// code page entirely; the VT escape sequences are still parsed because VT
    /// processing is enabled on the handle.
    fn write_console(&mut self, text: &str) -> std::io::Result<()> {
        #[cfg(windows)]
        {
            let wide: Vec<u16> = text.encode_utf16().collect();
            let mut written: u32 = 0;
            let ok = unsafe {
                win::WriteConsoleW(
                    self.handles.out_handle,
                    wide.as_ptr(),
                    wide.len() as u32,
                    &mut written,
                    std::ptr::null_mut(),
                )
            };
            if ok != 0 {
                return Ok(());
            }
        }
        use std::io::Write;
        self.out.write_all(text.as_bytes())
    }

    /// Capture the visible window: characters and 16-colour attributes.
    ///
    /// wbat64's cell coordinates are viewport-relative (VT CUP row 1 = top of the
    /// window), so the console window rectangle is read rather than the whole
    /// screen buffer.
    #[cfg(windows)]
    fn capture_windows_screen(&mut self) -> Option<Screen> {
        unsafe {
            let handle = self.handles.out_handle;
            let mut info: win::ConsoleScreenBufferInfo = std::mem::zeroed();
            if win::GetConsoleScreenBufferInfo(handle, &mut info) == 0 {
                return None;
            }
            let left = info.window.left;
            let top = info.window.top;
            let width = (info.window.right - info.window.left + 1).max(1) as u16;
            let height = (info.window.bottom - info.window.top + 1).max(1) as u16;
            let mut screen = Screen::blank(width, height, Style::default());

            for row in 0..height {
                let start = win::Coord {
                    x: left,
                    y: top + row as i16,
                };
                let mut chars = vec![0u16; width as usize];
                let mut attrs = vec![0u16; width as usize];
                let mut read_chars: u32 = 0;
                let mut read_attrs: u32 = 0;
                if win::ReadConsoleOutputCharacterW(
                    handle,
                    chars.as_mut_ptr(),
                    width as u32,
                    start,
                    &mut read_chars,
                ) == 0
                {
                    continue;
                }
                if win::ReadConsoleOutputAttribute(
                    handle,
                    attrs.as_mut_ptr(),
                    width as u32,
                    start,
                    &mut read_attrs,
                ) == 0
                {
                    continue;
                }

                let text = String::from_utf16_lossy(&chars[..read_chars as usize]);
                let mut x: u16 = 0;
                for ch in text.chars() {
                    if x >= width {
                        break;
                    }
                    if ch == '\0' {
                        x += 1;
                        continue;
                    }
                    let cell_width = char_width(ch).max(1) as u16;
                    let attr = attrs.get(x as usize).copied().unwrap_or(0);
                    let fg = Color::Indexed((attr & 0x0F) as u8);
                    let bg = Color::Indexed(((attr >> 4) & 0x0F) as u8);
                    screen.put(x, row, ch, Style::new(fg, bg));
                    x += cell_width;
                }
            }
            Some(screen)
        }
    }
}

impl Backend for ConsoleBackend {
    fn size(&mut self) -> Result<(u16, u16)> {
        if let Ok(size) = crossterm::terminal::size() {
            self.size = size;
        }
        Ok(self.size)
    }

    fn write_str(&mut self, text: &str) -> Result<()> {
        self.write_console(text)
            .map_err(|e| Error::io("writing to the console", e))
    }

    fn flush(&mut self) -> Result<()> {
        use std::io::Write;
        self.out
            .flush()
            .map_err(|e| Error::io("flushing the console", e))
    }

    fn poll_event(&mut self, timeout: Duration) -> Result<Option<Event>> {
        use crossterm::event as ct;
        let ready = ct::poll(timeout).map_err(|e| Error::msg(format!("poll failed: {e}")))?;
        if !ready {
            return Ok(None);
        }
        let event = ct::read().map_err(|e| Error::msg(format!("read failed: {e}")))?;
        Ok(Self::convert(event))
    }

    fn cursor_position(&mut self) -> Option<(u16, u16)> {
        crossterm::cursor::position().ok()
    }

    fn capture_screen(&mut self) -> Option<Screen> {
        #[cfg(windows)]
        {
            self.capture_windows_screen()
        }
        #[cfg(unix)]
        {
            None
        }
    }
}

impl Drop for ConsoleBackend {
    fn drop(&mut self) {
        // Leave the terminal usable: default colors and a visible cursor.
        let _ = self.write_console("\x1b[0m\x1b[?25h");
        let _ = self.flush();
        let _ = crossterm::terminal::disable_raw_mode();
        if self.mouse {
            self.set_mouse(false);
        }
        #[cfg(windows)]
        unsafe {
            // Restore the exact input mode we found (quick-edit, mouse flags,
            // processed input, ...) instead of leaving our modifications behind.
            if let Some(mode) = self.handles.saved_in_mode {
                win::SetConsoleMode(self.handles.in_handle, mode);
            }
            win::SetStdHandle(win::STD_OUTPUT_HANDLE, self.handles.saved_out);
            win::SetStdHandle(win::STD_INPUT_HANDLE, self.handles.saved_in);
            win::CloseHandle(self.handles.in_handle);
        }
        #[cfg(unix)]
        unsafe {
            libc::dup2(self.handles.saved_in, 0);
            libc::dup2(self.handles.saved_out, 1);
            libc::close(self.handles.saved_in);
            libc::close(self.handles.saved_out);
        }
    }
}

// ---------------------------------------------------------------------------
// Capability helpers
// ---------------------------------------------------------------------------

/// Whether the console device can process VT escape sequences.
pub fn console_supports_ansi() -> bool {
    #[cfg(windows)]
    unsafe {
        let name = win::wide("CONOUT$");
        let handle = win::CreateFileW(
            name.as_ptr(),
            win::GENERIC_READ | win::GENERIC_WRITE,
            win::FILE_SHARE_READ | win::FILE_SHARE_WRITE,
            std::ptr::null_mut(),
            win::OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        );
        if handle == win::INVALID_HANDLE_VALUE {
            return false;
        }
        let mut mode: u32 = 0;
        let ok = win::GetConsoleMode(handle, &mut mode) != 0;
        // Try to enable VT processing; that is what the renderer needs.
        let enabled = ok && {
            win::SetConsoleMode(handle, mode | win::ENABLE_VIRTUAL_TERMINAL_PROCESSING) != 0
        };
        win::CloseHandle(handle);
        enabled
    }
    #[cfg(unix)]
    unsafe {
        libc::isatty(1) == 1 || libc::isatty(0) == 1
    }
}

/// Wait for one key on the console (used by `ErrorPause`).
pub fn wait_key(timeout: Duration) {
    if let Ok(mut backend) = ConsoleBackend::new(false) {
        let _ = backend.poll_event(timeout);
    }
}

// ---------------------------------------------------------------------------
// Headless backend
// ---------------------------------------------------------------------------

/// A scripted backend used by `--keys`/`--dump-screen` and the test suite.
pub struct HeadlessBackend {
    events: VecDeque<Event>,
    size: (u16, u16),
    idle: u32,
}

impl HeadlessBackend {
    pub fn new(size: (u16, u16), events: Vec<Event>, _dump: bool) -> HeadlessBackend {
        HeadlessBackend {
            events: events.into(),
            size,
            idle: 0,
        }
    }
}

impl Backend for HeadlessBackend {
    fn size(&mut self) -> Result<(u16, u16)> {
        Ok(self.size)
    }

    fn write_str(&mut self, _text: &str) -> Result<()> {
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }

    fn poll_event(&mut self, _timeout: Duration) -> Result<Option<Event>> {
        if let Some(event) = self.events.pop_front() {
            return Ok(Some(event));
        }
        // Let real time pass so countdown timers work in scripted runs, then
        // emit a tick. A dialog with no more input and no timer ends after a
        // bounded number of idle ticks instead of hanging.
        std::thread::sleep(Duration::from_millis(1));
        self.idle = self.idle.saturating_add(1);
        if self.idle > 10_000 {
            return Ok(Some(Event::Key(Key::new(KeyCode::Esc))));
        }
        Ok(Some(Event::Tick))
    }

    fn cursor_position(&mut self) -> Option<(u16, u16)> {
        Some((0, 0))
    }

    fn is_headless(&self) -> bool {
        true
    }

    fn supports_ansi(&self) -> bool {
        false
    }

    fn dump(&self) -> Option<String> {
        None
    }
}
