# WBAT64

**Dialog boxes, menus and lists for batch files and shell scripts.**
A modern Rust successor to WBAT.COM 2.50 by Horst Schaeffer.

WBAT64 draws a pop-up on the terminal, runs a small interaction, and hands the
result back as an exit code and as shell variables. It ships as a single
executable plus a Rust library built on the same engine.

```bat
wbat64 box 19,55 (blue on +brown) "Continue?^Existing files will be replaced." &yes,&no,cancel
```

## Features

- **One static executable**, no runtime dependencies
- **Cross-platform:** Windows (conhost and Windows Terminal), Linux, macOS
- **WBAT-compatible:** layouts, quick boxes, `wbat.ini` and exit codes all work as before
- **Modern extras:** Unicode, truecolor, JSON output, stdin lists, mouse support, resize handling
- **Clean output:** the UI is drawn on the console device, never on stdout, so results stay capturable

> Looking for the old DOSBox/OTVDM-based version? It lives on the
> [`legacy` branch](https://github.com/hXR16F/WBAT64/tree/legacy).

## Installation

Build from source:

```sh
cargo build --release
./target/release/wbat64 help
```

On Windows you can run `build.bat` instead, which builds the workspace and
copies the result to `bin\wbat64.exe`.

Then copy `wbat64` (`wbat64.exe` on Windows) somewhere on your `PATH`. For use
from batch files, copy `w.bat` next to it.

## Usage in batch files

`w.bat` keeps the classic WBAT calling convention. It runs WBAT64 and imports
the resulting variables into your script:

```bat
@echo off
call w.bat box @wbat.txt:signup
if errorlevel 100 exit /b 1
echo Hello %name% from %city%
```

Unlike the original, it does not rewrite itself while running. It uses a
temporary file instead.

Because the UI goes to `CONOUT$`, you can also redirect or capture the output
yourself:

```bat
wbat64 box ok > results.bat
wbat64 box @wbat.txt:signup --emit bat > vars.bat
```

<details>
<summary><strong>How <code>w.bat</code> finds things</strong></summary>

**Executable**, first match wins:

1. `WBAT64_EXE`
2. `wbat64.exe` next to `w.bat`
3. `bin\wbat64.exe`
4. `target\release\wbat64.exe`
5. `wbat64` on `PATH`

**Temporary file directory**, first writable one wins:

1. `WBAT64_TMP`
2. the current directory
3. the script directory
4. the executable directory
5. `%TEMP%`

Each candidate is probed with `wbat64 --probe-write`, so the check is made by
WBAT64 itself. This matters because a sandboxed shell can let `cmd` create a
file where `wbat64` is denied.

Set `WBAT64_DEBUG=1` to print the resolved executable and every probe result.
`--emit-file` also clears read-only leftovers and retries briefly if antivirus
software or an indexer is holding the file.

</details>

## Commands

| Command | Synopsis | Notes |
| --- | --- | --- |
| `box` | `box [pos] [(colors)] ["text"] items` | Frame, shadow, title, buttons |
| `box` | `box [pos] [(colors)] @file:label [#bar[,timeout]]` | Layout unit |
| `menu` | `menu ...` | Like `box`, but buttons have no button color |
| `list` | `list [pos] file[:label]` or `list -` | Scrolling list; `-` reads stdin |
| `text` | `text [pos] [(colors)] "string"` or `@file:label` | Text rectangle, no frame |
| `fill` | `fill [pos] [(h,w)] [(colors)] ["str"] [+shift]` | Fill or recolor an area; without a string it only recolors |
| `cls` | `cls [(colors)]` | Clear the screen |
| `cursor` | `cursor [line] [+feeds]` | Position the cursor; `cursor 25+5` scrolls |
| `save` / `restore` | `save [id]`, `restore [id]` | In-memory screen stack |
| `fix` | `fix box ...` | Leave the box on screen after closing |
| `stdout` | `stdout @file[:label]` | Write a unit to stdout with variables expanded |
| `el` | `el n` | Set the exit code |
| `ansi?` | `ansi?` | Exit 0 if the console supports VT |
| `pal` | `pal [file]` | Apply a 16-color DOS palette (`pal.ini`) |
| `font` | `font [25\|33]` | Bitmap fonts are unsupported; warns and exits 0 |
| `check` | `check [@file[:label]]` | Validate layouts, with line/column errors |
| `version`, `help` | | |

Chain commands with `!` or `--and`. A `box`, `menu` or `list` must come last:

```bat
wbat64 cls --and text 2,4 @wbat.txt:info --and box ok
```

Positions are 1-based `line,column`. Boxes are centered unless positioned;
`text` and `fill` default to `1,1`.

### Colors

Colors use the WBAT DOS palette names, in this order:
`black red green blue brown cyan magenta white`. Prefix a name with `+` (or write
`bright` / `light`) for the bright variant.

The form is `(foreground on background)`. A leading `on` sets only the
background, and the `on` itself is optional:

```bat
fill 25,1 (1,80) (on white)     :: 80x1 white block, default foreground
box (black white) "Hello"       :: same as (black on white)
```

WBAT64 also accepts `#rrggbb`, `rgb(r,g,b)` and bare `0-255` terminal indexes:

```bat
wbat64 cls (#ffd700 on #202020)
wbat64 box (white on 17) "Indexed background"
```

DOS indexes are mapped to the ANSI palette on output, so `(red on black)` is
really red, even though DOS lists red as index 4.

### Preselect and countdown

```bat
wbat64 box @wbat.txt:autoclose #1,15
wbat64 menu @wbat.txt:main_menu #%mpos%+1
```

- `#n` preselects button `n`. A value of 0 or an invalid one selects the first button.
- `#n,t` closes after `t` seconds (max 3600) and returns button `n`. An invalid `n` closes as ESC.
- Any key or click cancels the timer.
- A `#?` token in the layout shows the remaining seconds (`--` once cancelled).

## Layout files

A layout file is a sequence of units. A line starting with `:` begins a unit and
ends the previous one, and a bare `:` line also ends a unit. The label line can
carry a title and an `[x]` close button:

```
:signup "New user" [x]
 Name: [$ name,40 ]
 City: [$ city    ]
 [.] male  [.] female
 [!] over 21   [!] send newsletter
 [    OK    ] [? cancel ]
:
```

### Controls

| Layout | Meaning |
| --- | --- |
| `[ OK ]` | Button or menu item. `&` marks the hotkey; a trailing `&` disables it |
| `[? cancel ]` | Escape button (the `?` is not shown) |
| `[$ name,40,U! ]` | Input: width 40, uppercase, required |
| `[!] text` | Checkbox (up to 9), stored in `WCB1..WCB9` |
| `[.] text` | Radio button (one set, up to 9), stored in `WRB` |

### Text markup

- `~highlight~` highlights text
- `%VAR%` inserts a variable; `%VAR%________` gives it a fixed width
- `[[`, `%%`, `~~`, `##` are escapes
- End a line with `^` if your editor strips trailing spaces
- A line starting with `─` draws a divider

A leading `#` in a `text` unit makes a title bar:

```
:info
#  About
wbat64 ~0.1~ running for %USERNAME%
:
```

Layouts can also live inside a batch file: `@%0:label` (put them after `exit /b`).

### Frame, padding and colors

- WBAT's default `BoxFrame` is blank. The frame reserves one cell on each side
  but draws no border, which is the original one-character margin. Set
  `frame = "single"`, `"double"`, `"rounded"`, `"ascii"` (or a 9-character
  pattern) for a visible border.
- Each button gets one blank cell of padding on each side (the bracket cells),
  even when the layout says `[OK]`. Separate buttons with spaces.
- The box title uses only the `Title` / `BoxTitle` foreground and sits on the
  box background. A `─` divider uses the `Divider` foreground on the box background.
- `text` without colors writes with the colors already on screen. With colors,
  it fills the whole text rectangle.

## Results

| Channel | Values |
| --- | --- |
| Exit code | `1..n` button number, `100` ESC, `0` list item selected, `254` list ESC, `255` error |
| `WBAT` | Selected button as one word (or the list line) |
| `?` | Exit code (`ELvar`), or the list item number |
| `WCB1..9`, `WRB` | Checkbox and radio state |
| *name* | Each input field, trailing spaces removed |

On ESC, the escape button's word (or else the last button's word) becomes
`WBAT`. Input, checkbox and radio variables are left untouched unless
`ESCinput=1`. Initial values are read from the same variables before the box opens.

### Output modes

| Option | Result |
| --- | --- |
| *(none)* | Exit code only |
| `--emit bat` | `set "NAME=value"` lines on stdout |
| `--emit-file PATH` | Same, written to a file (used by `w.bat`) |
| `--emit ps1` | `$env:NAME = 'value'` |
| `--emit sh` | `export NAME='value'` |
| `--emit json` | One JSON object |

Under `cmd.exe`, if stdout is redirected to a file ending in `.bat`, batch
output is written automatically. PowerShell pipes stdout, so use `--emit bat` there.

**Escaping:** in batch output `%` becomes `%%` and CR/LF are dropped. With
`--delayed` (`EnableDelayedExpansion`), `!` becomes `^^!`. PowerShell and POSIX
shell values are single-quoted, with the usual doubling / `'\''` rules.

### JSON

```
> wbat64 box @wbat.txt:signup --emit json
{"button":{"index":1,"name":"OK"},"checkboxes":[1,0],"escaped":false,
 "exit_code":1,"inputs":{"city":"London","name":"Ada"},"radio":2}

> dir /b | wbat64 list - --emit json
{"escaped":false,"exit_code":0,"index":7,"item":"report.txt"}
```

## Native extras

These options are new in WBAT64:

- `--keys "down,down,enter"` runs dialogs headlessly, with no terminal needed.
  Mouse events can be scripted too: `click:4x6`, `rclick:60x20`, `drag:10x5`, `move:10x5`.
- `--dump-screen` prints the final cell buffer. Combine it with `--size 80x25`.
- `--multi` lets a list select several items with Space.
- `--filter` turns typing into a live substring filter (Esc clears it first).
- `--set NAME=VALUE` injects variables into layouts.
- `--prefix` prefixes emitted variable names.
- `--encoding utf8|cp437|cp850|oem|auto` selects the layout encoding.
- `--compat` / `--native` (or `WBAT64_COMPAT=1`) select the WBAT defaults. The
  shipped `wbat64.exe` runs in native mode. Copy or rename it to `wbat.exe`, or
  set `WBAT64_COMPAT=1`, for drop-in WBAT behavior.

With a real mouse, hovering a button inverts its colors (keyboard focus keeps
the `Bar` color), and clicking activates it.

## Configuration

### `wbat.ini` (compat mode)

Searched in the current directory, then the executable directory.

```ini
[general]
ELvar=1            ; set ? = exit code
rightESC=1         ; right-click outside = ESC
ESCinput=0         ; 1 = keep edits on ESC
ErrorPause=1
BoxFrame="         " ; nine spaces = blank frame (1-char margin)
CheckSymbol="x"
RadioSymbol="*"
HiSymbol="~"
SourceFile=wbat.txt
PalFile=+pal.ini
[color]
Screen=black on +green
Box=black on +white
BoxHi=red on +white
Bar=+white on +red
Button=black on white
HotKey=red
Shadow=+black      ; NONE turns the shadow off
Divider=cyan       ; background comes from the box
Title=black on +blue
TextHi=red on +green
TextTitle=+white on magenta
```

Any key can be overridden for one run with a `W`-prefixed environment variable:

```bat
set WButton=black on +cyan
set WrightESC=0
```

### `wbat64.toml` (native mode)

Searched as `./wbat64.toml`, then `%APPDATA%\wbat64\wbat64.toml` or
`~/.config/wbat64/wbat64.toml`. Use `--config PATH` to point at another file.

```toml
[general]
frame = "none"           # none/blank | ascii | single | double | rounded | 9-char pattern
mouse = true
escape_discards = true
max_width = 0            # 0 = terminal width
encoding = "utf8"

[colors]
screen = "#c8e8c0 on #1e1e1e"
box    = "black on +white"
bar    = "+white on #b00020"
shadow = "none"
```

Set `NO_COLOR` (or pass `--no-color`) for a monochrome theme.

## Library

```rust
use wbat64::{Dialog, Layout, Outcome};

fn main() -> wbat64::Result<()> {
    let layout = Layout::from_file("examples/wbat.txt", "signup")?;
    let res = Dialog::new(layout)
        .var("city", "Berlin")
        .timeout(std::time::Duration::from_secs(60))
        .run()?;

    match res.outcome {
        Outcome::Button { index, name } => println!("button {index} ({name})"),
        Outcome::Escape => println!("cancelled"),
        Outcome::Timeout { .. } => println!("timeout"),
    }
    println!("{:?}", res.inputs.get("name"));
    Ok(())
}
```

The workspace is split into these crates:

| Crate | Contents |
| --- | --- |
| `wbat64-core` | Layout tokenizer/AST, colors, INI + TOML subset, interpolation, results, escapers |
| `wbat64-render` | Cell buffer, frame/shadow/title painters, diffing, ANSI encoding |
| `wbat64-tui` | Console device, event loop, dialog and list widgets, input editor |
| `wbat64-cli` | Hand-written argument grammar, dispatch, emitters |
| `wbat64` | Thin binary plus the `Layout` / `Dialog` embedding API |

## Examples

`examples/batch/` has small, self-contained scripts. Run them from `cmd.exe`;
`examples/_env.bat` finds the executable on `PATH` or falls back to `bin\wbat64.exe`.

| File | Shows |
| --- | --- |
| `01_pause.bat` | A `PAUSE` replacement |
| `02_choice.bat` | A `CHOICE` replacement with descending `if errorlevel` tests |
| `03_form.bat` | Inputs, radio buttons and checkboxes imported by `w.bat` |
| `04_menu.bat` | A menu loop with `#%mpos%+1` and `goto %wbat%` |
| `05_list.bat` | Picking a file from `dir /b` output |
| `06_timeout.bat` | A `#1,3` countdown that returns button 1 |
| `07_text.bat` | `cls` + `text` + `fill` + `box` chained with `--and` |
| `08_check.bat` | Layout validation with `wbat64 check` |
| `09_emit.bat` | Capturing `--emit bat` output without `w.bat` |

`examples/demo.bat` is a longer tour that combines all of the above.

## Development

```sh
cargo test --workspace
```

The suite has unit tests for the parser, colors, escaping and widgets, plus
end-to-end tests that drive the real executable with `--keys` and
`--dump-screen`. That headless path means CI needs no terminal, and it also
powers snapshot-style checks of rendered frames.

To smoke-test the real console backend (window size, VT output, key handling),
run these from a terminal:

```sh
# Writes ┌─┐✓日, reads the console cells back and fails on a mismatch
cargo run -p wbat64-tui --example console_probe

# Draw a box with `fix`, then dump the real console window
wbat64 fix box 2,4 @examples/wbat.txt:signup --timeout 1
cargo run -p wbat64-tui --example console_dump

# Quick end-to-end check of the release binary
wbat64 box 2,2 "Interactive smoke test" " OK " --timeout 1
```

## Troubleshooting

**`w.bat: wbat64 executable not found`**
The binary is not in `bin\`, next to `w.bat`, or on `PATH`. Run `build.bat` (or
`cargo build --release`) and keep `bin\wbat64.exe`, or point `WBAT64_EXE` at it.
Antivirus tools sometimes quarantine freshly built unsigned executables, so
allow the file if it keeps disappearing.

**`w.bat: cannot create a temporary batch file`**
None of the candidate directories is writable. Set `WBAT64_TMP` to a writable
directory. `WBAT64_DEBUG=1` prints every probe attempt.

## Compatibility notes

- WBAT's self-modifying `W.BAT` is replaced by `--emit-file`. Layouts and invocation stay compatible.
- **Windows output:** the UI is written through `WriteConsoleW` (UTF-16), so box
  drawing, accents and wide characters render correctly regardless of the
  console OEM code page. `.bat` results are emitted in the console OEM code page
  (CP437/CP850), because that is what `cmd.exe` reads. Override this with
  `--encoding utf8` or `--encoding cp850`.
- **Screen restore:** on Windows, WBAT64 reads the visible console cells and
  colors at startup, so moving or closing a box restores the real background.
  Other terminals have no portable way to read cells. There, WBAT64 repaints
  what it knows it drew and clears the rest to the terminal's default colors.
- Boxes wider than 78 columns are rejected in compat mode. Native mode uses the terminal width.
- `font` cannot load DOS bitmap fonts and only warns.
- `stdout` writes the unit as plain text with variables expanded.
- Mouse support uses Win32 console input records on Windows and VT mouse
  reporting on Unix. The same code path can be tested headlessly with
  `click:`, `drag:` and `rclick:` key scripts.
- **Not implemented yet:** `--plain` (screen-reader mode), `wbat64 pick`,
  `wbat64 progress` and the optional C ABI. `--plain` fails with a clear message
  instead of silently drawing.

See [`docs/COMPATIBILITY.md`](docs/COMPATIBILITY.md) for the remaining WBAT
details and the places where behavior was chosen rather than inherited.

## Credits

WBAT64 is an independent, clean-room implementation inspired by **WBAT.COM 2.50**,
the batch-file dialog tool by **Horst Schaeffer** (1999–2005). WBAT is his
freeware with copyright reserved. This project contains no WBAT code, binaries,
fonts, `.FON`/`.FNT` files, original demo layouts or ASCII art. It deliberately
shares only the observable command syntax and result convention, so existing
layouts and batch files keep working.

The Rust implementation is by **zaktabyte**.

## License

MIT. Copyright © 2026 zaktabyte.
