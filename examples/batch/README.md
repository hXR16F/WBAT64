# Batch-file examples

Run these from `cmd.exe` (double-clicking works too). Every script calls
`..\_env.bat`, which prefers `..\..\bin\wbat64.exe` and otherwise uses `wbat64` from
`PATH`, and points at `..\dialogs.txt` and `..\..\w.bat`.

`w.bat` writes its temporary result file to `WBAT64_TMP`, the current directory,
the script directory, or `%TEMP%` — whichever wbat64 can actually write. Set
`WBAT64_TMP` to force a directory (useful in sandboxed shells where `%TEMP%` is
read-only).

| File | Shows |
| --- | --- |
| `01_pause.bat` | A `PAUSE` replacement: one box with an OK button |
| `02_choice.bat` | A `CHOICE` replacement: three buttons and descending `if errorlevel` tests |
| `03_form.bat` | Inputs, radio buttons and checkboxes imported with `w.bat` |
| `04_menu.bat` | A menu loop with `#%mpos%+1`, `goto %wbat%` and ESC handling |
| `05_list.bat` | Picking a file from a `dir /b` listing |
| `06_timeout.bat` | A countdown (`#1,3`) that returns button 1 |
| `07_text.bat` | `cls` + `text` + `fill` + `box` chained with `--and` |
| `08_check.bat` | Validating a layout file with `wbat64 check` |
| `09_emit.bat` | Capturing `--emit bat` output and calling it (no `w.bat`) |

Notes:

* Every script forwards extra arguments to `wbat64`, so it can be driven headless
  in a test: `04_menu.bat --keys esc --emit json`.
* `ESC` returns errorlevel 100 for boxes/menus; 254 for lists.
* Test errorlevels in descending order (`if errorlevel 100` before `if errorlevel 3`).
* `&` inside an unquoted batch argument must be escaped as `^&` (or put the
  hotkey in an `@file:label` layout instead, as these examples do).
* Under `EnableDelayedExpansion` use `--and` instead of `!` as the chain
  separator.
