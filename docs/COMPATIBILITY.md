# WBAT64 compatibility notes

WBAT64 is a clean-room implementation. The behavior below comes from the WBAT
2.50 manual (`WBAT.TXT`), `WBAT.INI`, `PAL.INI` and the `DEMO.BAT` tree — not
from disassembling the original binary. WBAT.COM 2.50 was written by **Horst
Schaeffer** (1999–2005); WBAT64 is an independent reimplementation by
**zaktabyte** and contains none of WBAT's code or assets. Where the manual is
ambiguous, WBAT64 chooses a behavior and documents it here.

The shipped executable is named `wbat64`, so it uses native defaults. Copy or
rename it to `wbat.exe`, pass `--compat`, or set `WBAT64_COMPAT=1` for the
WBAT-compatible defaults (78-column boxes, OEM layout encoding, `WBAT.INI`
only).

## What maps directly

| WBAT | wbat64 |
| --- | --- |
| `WBAT BOX ...` | `wbat64 box ...` |
| `@file:label`, `:label`, `@+file`, `+:label` | same |
| `#3`, `#3,15`, `#0+1` | same |
| `(blue on +brown)` | same, plus `#rrggbb`, `rgb()`, `0-255` |
| `box 21,40 "text" ok,back` | same (quick box) |
| `[brackets]` controls | same |
| `~highlight~`, `%VAR%`, `%VAR%____` | same |
| `[[`, `%%`, `~~`, `##` escapes | same |
| `WBAT.INI` keys and `W`-prefixed overrides | same |
| Exit codes 1..n / 100 / 254 / 255 | same |
| `WBAT`, `?`, `WCB1..9`, `WRB`, input variables | same |
| `cls`, `text`, `fill`, `cursor`, `save`, `restore`, `fix`, `stdout`, `el`, `pal` | same names and chaining with `!` |
| `CALL W.BAT ...` | `call w.bat ...` (temporary file instead of self-modifying batch) |

## Deliberate differences

* **Result transport.** `W.BAT` rewrote itself; wbat64 writes a temporary
  `set` file via `--emit-file`. The calling convention is unchanged. `w.bat`
  picks the first directory among `WBAT64_TMP`, the current directory, the script
  directory and `%TEMP%` that `wbat64 --probe-write` confirms it can write to.
* **No console ⇒ error.** If the UI cannot reach a console device, wbat64 exits
  255 instead of drawing to redirected stdout.
* **Exit code 0 for `box`.** WBAT's box buttons start at 1. wbat64 uses 1 too,
  except that a box with no buttons and a single input field returns 1 when
  Enter closes it.
* **Width limits.** Compat mode rejects boxes wider than 78 columns; native
  mode uses the terminal width and truncates instead.
* **Fill.** Omitting the string recolours the area and keeps the existing text;
  `fill ... " "` clears it. Color expressions are `foreground on background`
  (the word `on` is optional); a leading `on`, as in `(on white)`, sets only
  the background and leaves the foreground at the terminal default.
* **Buttons.** A button occupies the whole bracket span: `[OK]` is four cells
  (` OK `), because WBAT turns the two bracket cells into one blank of padding
  on each side. Spaces between buttons come from the layout text.
* **Box titles.** The quoted title in a label line is drawn in the top border.
  Only the foreground of `Title`/`BoxTitle` is used; the title sits on the box
  background, matching WBAT's shipped behaviour. `[x]` adds a clickable close
  button.
* **Dividers.** A line beginning with `─` (U+2500, CP437 0xC4) draws a divider
  in the `Divider` foreground; the background is always the box background.
* **Frame.** The built-in default is WBAT's blank 3x3 `BoxFrame`. It still
  reserves one cell on each side, which is the original one-character margin
  around the content; use `frame = "single"` (or another name) for a visible
  border.
* **Text units.** Without explicit colours, `text` writes with the colours
  already on screen and does not repaint the rectangle; with colours it fills
  the rectangle. `TextHi` and `TextTitle` come from the configuration.
* **`font`.** DOS bitmap fonts cannot be loaded; the command warns and exits 0.
* **`pal`.** Applies `OSC 4` on VT terminals and `SetConsoleScreenBufferInfoEx`
  is *not* used; on Windows 10+ `OSC 4` is understood by Windows Terminal and
  modern conhost. Palette values are `R G B` in 0..63 in DOS order.

## Chosen semantics (manual was silent)

* **Encoding.** Layouts are decoded with the console OEM code page in compat
  mode and UTF-8 in native mode. Console drawing uses `WriteConsoleW`, so the
  active output code page does not mangle Unicode glyphs. Batch result files are
  written in the console OEM code page (CP437/CP850 by default, overridable with
  `--encoding utf8|cp850`) because that is how `cmd.exe` reads `.bat` files;
  JSON and PowerShell/shell emitters stay UTF-8.
* **Screen restoration.** Before drawing, a Windows session reads the visible
  console cells (`ReadConsoleOutputCharacterW` / `ReadConsoleOutputAttribute`)
  and uses them as the baseline, so dragging or closing a box puts the original
  text and colours back exactly. On other terminals there is no portable way to
  read cells: wbat64 restores the cells it painted and clears the rest to the
  terminal default. `save`/`restore` round-trips exact cells; a `fix` box stays
  on screen instead.
* **Enter on a single input field** closes the box and returns button 1 with the
  input variable name as the result word.
* **Enter with several controls** moves focus forward; required (`!`) fields
  block button activation, but Esc still cancels.
* **ESC keeps or discards edits** per `ESCinput` (default: discard). Checkboxes,
  radio buttons and inputs are all part of this rule.
* **Timeout with an invalid bar** closes as ESC; a valid one returns that
  button without marking the run as "escaped".
* **Type-ahead in lists** resets after `ListHotTicks` ticks (default 10 ≈ 1 s).
* **Multi-select** (native) returns the first marked item in `WBAT`/`?` and the
  whole set in JSON.
* **Input editing**: Home/End/Left/Right/Backspace/Delete, horizontal scrolling,
  password masking with `*`, `U` uppercases on entry and for initial values.
* **Mouse**: left click focuses/activates a control, dragging the title bar moves
  the box, hovering a button inverts its colours (focus is unchanged), a right
  click outside is ESC when `rightESC=1`, and the wheel scrolls lists. A `fix`
  box cannot be dragged away and stays after closing.

## Keys not inherited

Section 9 of the design notes lists later additions. Implemented today: JSON /
ps1 / sh emitters, stdin lists, `--multi`, `--filter`, `wbat64 check`, truecolor,
Unicode widths, resize handling, `NO_COLOR`, headless `--keys`/`--dump-screen`.

Not implemented: `--plain` screen-reader mode, `wbat64 pick`, `wbat64 progress`, the
optional C ABI. `--plain` returns a clear error instead of silently drawing.

## Items the original manual leaves as [verify]

These were decided as follows; they are the first things to check against a real
WBAT under DOSBox if strict fidelity is required:

* exact drag behavior while the mouse leaves the box (wbat64 clamps to the screen);
* the result word for labels with several `&` markers (wbat64 uses the first);
* whether WBAT keeps checkbox/radio state on ESC when `ESCinput=0` (wbat64 keeps
  them only with `ESCinput=1`, matching the manual's wording);
* whether `#0` means "first item" (wbat64 does);
* list box sizing when only a position is given (wbat64 grows the box to fit the
  items, up to 12 rows, and never beyond the screen).
