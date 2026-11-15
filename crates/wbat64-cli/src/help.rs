//! `wbat64 help` and `--help`.

pub const USAGE: &str = r#"wbat64 0.1.0 — dialog boxes for batch files and shell scripts

USAGE
  wbat64 <command> [position] [(colors)] ["text"] [items | @file:label] [#bar[,timeout]] [options]
  wbat64 <cmd> ... ! <cmd> ...          chain commands ('!' needs delayed expansion off)
  wbat64 <cmd> ... --and <cmd> ...      same chain without the '!' character

COMMANDS
  box   [pos] [(col)] "text" items    dialog with frame, shadow and buttons
  box   [pos] [(col)] @file:label [#bar[,timeout]]
  menu  ...                           like box, but buttons are not coloured
  list  [pos] [(col)] file[:label]    scrolling list, one item per line
  list  -                             read the list from stdin
  text  [pos] [(col)] "string"        draw a text rectangle (no frame)
  text  [pos] [(col)] @file:label
  fill  [pos] [(h,w)] [(col)] ["str"] [+shift]
                                      fill an area, optionally recolour only
  cls   [(col)]                       clear the screen
  cursor [line] [+feeds]              position the cursor; feeds scrolls lines
  save  [id] / restore [id]           in-memory screen stack
  fix box ...                         leave the box on screen after closing
  stdout @file[:label]                write the unit to stdout, variables expanded
  el n                                set the exit code (0..255)
  ansi?                               exit 0 when the console supports VT output
  pal [file]                          apply a 16-colour DOS palette (PAL.INI)
  font [25|33]                        not supported; warns and exits 0
  check [@file[:label]]               validate layouts, print line/column errors
  version, help                       this text

POSITION AND SIZE
  Positions are 1-based "line,column". Boxes are centred by default; with only
  a line given they are centred horizontally. text/fill default to 1,1.
  fill takes (height,width) in its own parentheses.

COLORS
  (foreground on background) with DOS names: black red green blue brown cyan
  magenta white. Prefix '+' or write bright/light for the bright half; yellow is
  bright brown; grey/gray is normal white. Also accepted: #rrggbb, rgb(r,g,b)
  and 0-255 terminal indexes, e.g. (#ffd700 on #202020).

BAR AND TIME-OUT
  #3        preselect button 3 (0 or invalid selects the first)
  #3,15     close after 15 seconds and return button 3
  Arithmetic is allowed: #%mpos%+1. Any key or click cancels the timer; a #?
  token in the layout shows the countdown.

LAYOUT FILES
  Units start at a line beginning with ':' and end at the next one. The label
  line may carry a title and a [x] close button:
      :signup "New user" [x]
  Controls: [ OK ]  [? cancel ]  [$ name,40,U!]  [!]  [.].  Use [[ for a
  literal '['. ~text~ highlights, %VAR% expands, %VAR%____ fixes the width,
  and [[ %% ~~ ## are escapes.

RESULT CHANNELS
  Exit codes: 1..n button number, 100 ESC, 0 list item selected, 254 list
  ESC, 255 error. Variables: WBAT (button word or list line), ? (exit code),
  WCB1..9 (checkboxes), WRB (radio), and one variable per input field.

OPTIONS
  --emit bat|ps1|sh|json   result format on stdout
  --emit-file PATH         write "set NAME=value" lines to PATH
  --prefix TEXT            prefix emitted variable names
  --delayed                caller uses EnableDelayedExpansion (! escaping)
  --pos L,C               --colors EXPR      --title TEXT
  --bar N                  --timeout N       --width N   --height N
  --encoding utf8|cp437|cp850|oem|auto
  --set NAME=VALUE         variable visible to layouts
  --keys "down,enter"      scripted events (headless; no terminal needed)
  --dump-screen            print the final cell buffer to stdout
  --size COLSxROWS         headless screen size (default 80x25)
  --multi                  list: select several items with Space
  --filter                list: type to filter
  --fix                    leave the box on screen after closing
  --compat / --native      WBAT compatibility versus native defaults
  --config PATH            read this wbat64.toml
  --no-mouse               disable mouse handling
  --truncate               compat: truncate long list lines at 76 columns
  --version, --help

MOUSE
  Click activates a control; dragging the title bar moves the box; hovering a
  button inverts its colours; the wheel scrolls lists; --no-mouse disables all
  mouse handling. Motion/hover needs a terminal that reports mouse movement.

EXAMPLES
  wbat64 box ok
  wbat64 box 19,55 (blue on +brown) "Continue?^Existing files are replaced." &yes,&no,cancel
  wbat64 box @WBAT.TXT:signup --emit bat > vars.bat
  wbat64 list - --emit json < files.txt
  wbat64 cls --and text 2,4 @WBAT.TXT:info --and box ok
"#;
