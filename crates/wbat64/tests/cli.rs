//! End-to-end tests: drive the `wbat64` binary with scripted keys.

use std::path::{Path, PathBuf};
use std::process::Command;

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn wbat64(args: &[&str]) -> Run {
    wbat64_with_env(args, &[])
}

fn wbat64_with_env(args: &[&str], env: &[(&str, &str)]) -> Run {
    let mut command = Command::new(env!("CARGO_BIN_EXE_wbat64"));
    command.current_dir(fixtures()).args(args);
    for (name, value) in env {
        command.env(name, value);
    }
    let output = command.output().expect("wbat64 runs");
    Run {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    }
}

fn json(run: &Run) -> serde_json::Value {
    serde_json::from_str(run.stdout.trim()).unwrap_or_else(|e| {
        panic!(
            "invalid JSON ({e}): {:?} (stderr: {})",
            run.stdout, run.stderr
        )
    })
}

#[test]
fn version_and_help() {
    let run = wbat64(&["version"]);
    assert_eq!(run.code, 0);
    assert!(run.stdout.starts_with("wbat64 "));

    let run = wbat64(&["help"]);
    assert_eq!(run.code, 0);
    assert!(run.stdout.contains("USAGE"));

    let run = wbat64(&["--version"]);
    assert_eq!(run.code, 0);
    assert!(run.stdout.starts_with("wbat64 "));
}

#[test]
fn default_source_file_from_ini() {
    let run = wbat64(&["box", ":one_button", "--keys", "enter", "--emit", "json"]);
    assert_eq!(run.code, 1, "{}", run.stderr);
    assert_eq!(json(&run)["button"]["name"], "go");
}

#[test]
fn whole_command_as_one_quoted_argument() {
    // This is what `call w.bat "box ..."` passes through.
    let run = wbat64(&["box @dialogs.txt:one_button --keys enter --emit json"]);
    assert_eq!(run.code, 1, "{}", run.stderr);
    assert_eq!(json(&run)["button"]["name"], "go");
}

#[test]
fn check_valid_layout() {
    let run = wbat64(&["check", "@dialogs.txt"]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert_eq!(run.stdout.trim(), "OK: 6 units");
}

#[test]
fn check_single_unit_grammar() {
    let run = wbat64(&["check", "@listunits.txt"]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert_eq!(run.stdout.trim(), "OK: 1 unit");
}

#[test]
fn check_reports_position() {
    let run = wbat64(&["check", "@broken.txt"]);
    assert_eq!(run.code, 1);
    assert!(run
        .stdout
        .contains("broken.txt:2:7: unterminated control '['"));
}

#[test]
fn check_rejects_unknown_label() {
    let run = wbat64(&["check", "@dialogs.txt:nope"]);
    assert_eq!(run.code, 1);
    assert!(run.stdout.contains("label ':nope' not found"));
}

#[test]
fn missing_layout_file_is_255() {
    let run = wbat64(&["box", "@nope.txt:x", "--keys", "enter"]);
    assert_eq!(run.code, 255);
    assert!(run.stderr.contains("cannot open layout file"));
}

#[test]
fn missing_label_is_255() {
    let run = wbat64(&["box", "@dialogs.txt:nope", "--keys", "enter"]);
    assert_eq!(run.code, 255);
    assert!(run.stderr.contains("label ':nope' not found"));
}

#[test]
fn menu_enter_returns_button_number() {
    let run = wbat64(&[
        "menu",
        "@dialogs.txt:main_menu",
        "#2",
        "--keys",
        "enter",
        "--emit",
        "json",
    ]);
    assert_eq!(run.code, 2, "{}", run.stderr);
    let value = json(&run);
    assert_eq!(value["exit_code"], 2);
    assert_eq!(value["button"]["index"], 2);
    assert_eq!(value["button"]["name"], "Setup");
    assert_eq!(value["escaped"], false);
}

#[test]
fn escape_uses_escape_button_word() {
    let run = wbat64(&[
        "box",
        "@dialogs.txt:main_menu",
        "--keys",
        "esc",
        "--emit",
        "bat",
    ]);
    assert_eq!(run.code, 100);
    assert!(run.stdout.contains("set \"WBAT=Exit\""));
    assert!(run.stdout.contains("set \"?=100\""));
}

#[test]
fn escape_without_escape_button_uses_last_button() {
    let run = wbat64(&[
        "box",
        "@forms.txt:required",
        "--keys",
        "esc",
        "--emit",
        "bat",
    ]);
    assert_eq!(run.code, 100);
    assert!(run.stdout.contains("set \"WBAT=cancel\""));
}

#[test]
fn quick_box_defaults_to_ok() {
    let run = wbat64(&["box", "ok", "--keys", "enter", "--emit", "json"]);
    assert_eq!(run.code, 1);
    let value = json(&run);
    assert_eq!(value["button"]["name"], "ok");
}

#[test]
fn quick_box_prompt_and_buttons_render() {
    let run = wbat64(&[
        "box",
        "5,10",
        "(blue on +brown)",
        "Continue?^Existing files will be replaced.",
        "&yes,&no,cancel",
        "--keys",
        "esc",
        "--dump-screen",
        "--size",
        "70x16",
    ]);
    assert_eq!(run.code, 100);
    assert!(run.stdout.contains("Continue?"));
    assert!(run.stdout.contains("Existing files will be replaced."));
    let button_line = run
        .stdout
        .lines()
        .find(|line| line.contains("yes") && line.contains("cancel"))
        .expect("button row");
    assert!(button_line.contains("yes    no    cancel"), "{button_line}");
}

#[test]
fn form_inputs_checkboxes_and_radio() {
    let keys = "A,d,a,tab,L,o,n,d,o,n,tab,space,tab,tab,space,tab,space,tab,enter";
    let run = wbat64(&[
        "box",
        "@dialogs.txt:signup",
        "--keys",
        keys,
        "--emit",
        "json",
    ]);
    assert_eq!(run.code, 1, "{}", run.stderr);
    let value = json(&run);
    assert_eq!(value["inputs"]["name"], "Ada");
    assert_eq!(value["inputs"]["city"], "London");
    assert_eq!(value["radio"], 1);
    assert_eq!(value["checkboxes"], serde_json::json!([1, 1]));
}

#[test]
fn uppercase_and_password_flags() {
    let run = wbat64(&[
        "box",
        "@forms.txt:flags",
        "--keys",
        "a,b,c,tab,s,e,c,tab,enter",
        "--dump-screen",
        "--size",
        "60x12",
    ]);
    assert_eq!(run.code, 1, "{}", run.stderr);
    assert!(run.stdout.contains("Upper: ABC"), "{}", run.stdout);
    assert!(run.stdout.contains("Pass:  ***"), "{}", run.stdout);
}

#[test]
fn required_field_blocks_until_filled() {
    // First Enter moves to the button; the second is refused while empty.
    let run = wbat64(&[
        "box",
        "@forms.txt:required",
        "--keys",
        "enter,enter,esc",
        "--emit",
        "json",
    ]);
    assert_eq!(run.code, 100);

    let run = wbat64(&[
        "box",
        "@forms.txt:required",
        "--keys",
        "a,enter,enter",
        "--emit",
        "json",
    ]);
    assert_eq!(run.code, 1);
    assert_eq!(json(&run)["inputs"]["name"], "a");
}

#[test]
fn single_input_enter_closes() {
    let run = wbat64(&[
        "box",
        "@forms.txt:single_input",
        "--keys",
        "h,i,enter",
        "--emit",
        "json",
    ]);
    assert_eq!(run.code, 1);
    let value = json(&run);
    assert_eq!(value["inputs"]["only"], "hi");
}

#[test]
fn countdown_timeout_returns_preselected_button() {
    let started = std::time::Instant::now();
    let run = wbat64(&["box", "@dialogs.txt:autoclose", "#1,1", "--emit", "json"]);
    assert_eq!(run.code, 1, "{}", run.stderr);
    assert_eq!(json(&run)["button"]["name"], "ok");
    assert!(started.elapsed().as_secs() >= 1);
}

#[test]
fn countdown_is_displayed() {
    let run = wbat64(&[
        "box",
        "@dialogs.txt:autoclose",
        "#1,5",
        "--keys",
        "x,esc",
        "--dump-screen",
        "--size",
        "60x14",
    ]);
    assert_eq!(run.code, 100);
    assert!(run.stdout.contains("close in -- seconds"), "{}", run.stdout);
}

#[test]
fn bar_arithmetic() {
    let run = wbat64(&[
        "menu",
        "@dialogs.txt:main_menu",
        "#0+1",
        "--keys",
        "enter",
        "--emit",
        "json",
    ]);
    assert_eq!(run.code, 1);
    assert_eq!(json(&run)["button"]["name"], "Tour");
}

#[test]
fn list_selection() {
    let run = wbat64(&["list", "list.txt", "--keys", "down,enter", "--emit", "json"]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    let value = json(&run);
    assert_eq!(value["index"], 2);
    assert_eq!(value["item"], "notes.md");
    assert_eq!(value["escaped"], false);
}

#[test]
fn list_escape_is_254() {
    let run = wbat64(&["list", "list.txt", "--keys", "esc", "--emit", "bat"]);
    assert_eq!(run.code, 254);
    assert!(run.stdout.contains("set \"?=0\""));
}

#[test]
fn list_from_stdin_and_multi() {
    let mut command = Command::new(env!("CARGO_BIN_EXE_wbat64"));
    command
        .current_dir(fixtures())
        .args([
            "list",
            "-",
            "--multi",
            "--keys",
            "space,space,enter",
            "--emit",
            "json",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = command.spawn().unwrap();
    {
        use std::io::Write;
        let stdin = child.stdin.as_mut().unwrap();
        stdin.write_all(b"one\ntwo\nthree\n").unwrap();
    }
    let output = child.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert_eq!(output.status.code(), Some(0), "stderr: {stderr}");
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(!text.trim().is_empty(), "empty stdout, stderr: {stderr}");
    let value: serde_json::Value = serde_json::from_str(text.trim())
        .unwrap_or_else(|e| panic!("bad JSON ({e}): {text:?} stderr: {stderr}"));
    assert_eq!(value["item"], "one");
    assert_eq!(value["selected"], serde_json::json!(["one", "two"]));
}

#[test]
fn list_filter_narrows_items() {
    let run = wbat64(&[
        "list",
        "list.txt",
        "--filter",
        "--keys",
        "p,h,enter",
        "--emit",
        "json",
    ]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    let value = json(&run);
    assert_eq!(value["item"], "photo.jpg");
    assert_eq!(value["index"], 3);
}

#[test]
fn list_filter_escape_clears_before_cancelling() {
    let run = wbat64(&[
        "list",
        "list.txt",
        "--filter",
        "--keys",
        "p,esc,down,enter",
        "--emit",
        "json",
    ]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert_eq!(json(&run)["item"], "notes.md");
}

#[test]
fn bat_output_uses_the_oem_code_page() {
    let dir = fixtures();
    let path = dir.join("oem_test.bat");
    let _ = std::fs::remove_file(&path);
    let run = wbat64_with_env(
        &[
            "box",
            "@forms.txt:required",
            "--keys",
            "esc",
            "--emit-file",
            "oem_test.bat",
            "--encoding",
            "cp437",
        ],
        &[("WESCinput", "1"), ("name", "é")],
    );
    assert_eq!(run.code, 100, "{}", run.stderr);
    let bytes = std::fs::read(&path).expect("batch file");
    let _ = std::fs::remove_file(&path);

    let needle = b"set \"name=";
    let start = bytes
        .windows(needle.len())
        .position(|window| window == needle)
        .expect("name line");
    // 'é' is 0x82 in CP437, not the UTF-8 pair C3 A9.
    assert_eq!(bytes[start + needle.len()], 0x82, "bytes: {bytes:?}");
    assert!(
        !bytes.windows(2).any(|window| window == [0xC3, 0xA9]),
        "batch output must not be UTF-8: {bytes:?}"
    );
}

#[test]
fn json_output_stays_utf8() {
    let run = wbat64_with_env(
        &[
            "box",
            "@forms.txt:required",
            "--keys",
            "esc",
            "--emit",
            "json",
        ],
        &[("WESCinput", "1"), ("name", "é")],
    );
    assert_eq!(run.code, 100);
    assert!(run.stdout.contains("é"), "{}", run.stdout);
}

#[test]
fn list_from_stdin_decodes_the_requested_encoding() {
    let mut command = Command::new(env!("CARGO_BIN_EXE_wbat64"));
    command
        .current_dir(fixtures())
        .args([
            "list",
            "-",
            "--encoding",
            "cp437",
            "--keys",
            "enter",
            "--emit",
            "json",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = command.spawn().unwrap();
    {
        use std::io::Write;
        // 0x82 is 'é' in CP437; the list must not fail as invalid UTF-8.
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(&[0x82, b'\n'])
            .unwrap();
    }
    let output = child.wait_with_output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim()).unwrap();
    assert_eq!(value["item"], "é");
}

#[test]
fn list_from_a_layout_label() {
    let run = wbat64(&[
        "list",
        "@listunits.txt:files",
        "--keys",
        "down,enter",
        "--emit",
        "json",
    ]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    let value = json(&run);
    assert_eq!(value["item"], "beta");
    assert_eq!(value["index"], 2);
}

#[test]
fn long_lines_do_not_overwrite_the_box_frame() {
    let long = "A".repeat(80);
    let run = wbat64(&[
        "box",
        "1,1",
        &long,
        "OK",
        "--config",
        "single.toml",
        "--keys",
        "esc",
        "--dump-screen",
        "--size",
        "40x10",
    ]);
    assert_eq!(run.code, 100);
    let line = run
        .stdout
        .lines()
        .find(|line| line.contains("AAAA"))
        .expect("prompt line");
    assert!(
        line.trim_end().ends_with('│'),
        "prompt must be clipped before the right border: {line:?}"
    );
}

#[test]
fn explicit_config_is_honoured_in_compat_mode() {
    let run = wbat64(&[
        "box",
        "hi",
        "OK",
        "--compat",
        "--config",
        "ascii.toml",
        "--keys",
        "esc",
        "--dump-screen",
        "--size",
        "40x10",
    ]);
    assert_eq!(run.code, 100, "{}", run.stderr);
    assert!(run.stdout.contains('+'), "{}", run.stdout);
    assert!(!run.stdout.contains('┌'), "{}", run.stdout);
}

#[test]
fn probe_write_reports_writability() {
    let dir = fixtures();
    let good = dir.join("probe_write_ok.bat");
    let _ = std::fs::remove_file(&good);
    let good_str = good.to_str().unwrap();
    let run = wbat64(&["--probe-write", good_str]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(!good.exists(), "probe must remove its test file");

    let bad = dir.join("no_such_directory").join("probe.bat");
    let bad_str = bad.to_str().unwrap();
    let run = wbat64(&["--probe-write", bad_str]);
    assert_eq!(run.code, 1);
}

#[test]
fn escape_button_click_returns_esc() {
    let run = wbat64(&[
        "menu",
        "@dialogs.txt:main_menu",
        "--pos",
        "1,1",
        "--keys",
        "click:3x4",
        "--emit",
        "bat",
    ]);
    assert_eq!(run.code, 100, "{}", run.stderr);
    assert!(run.stdout.contains("set \"WBAT=Exit\""), "{}", run.stdout);
}

#[test]
fn escape_button_hotkey_returns_esc() {
    let run = wbat64(&[
        "menu",
        "@dialogs.txt:main_menu",
        "--keys",
        "x",
        "--emit",
        "json",
    ]);
    assert_eq!(run.code, 100, "{}", run.stderr);
    assert_eq!(json(&run)["escaped"], true);
}

#[test]
fn escape_button_via_bar_returns_esc() {
    let run = wbat64(&[
        "menu",
        "@dialogs.txt:main_menu",
        "#4",
        "--keys",
        "enter",
        "--emit",
        "json",
    ]);
    assert_eq!(run.code, 100, "{}", run.stderr);
    assert_eq!(json(&run)["escaped"], true);
}

#[test]
fn escape_button_label_keeps_its_padding() {
    let run = wbat64(&[
        "box",
        "@dialogs.txt:signup",
        "--keys",
        "esc",
        "--dump-screen",
        "--size",
        "80x24",
    ]);
    let button_line = run
        .stdout
        .lines()
        .find(|line| line.contains("cancel"))
        .expect("button line");
    assert!(
        button_line.contains(" cancel"),
        "cancel should keep its padding: {button_line}"
    );
}

#[test]
fn emit_file_with_prefix() {
    let dir = fixtures();
    let path = dir.join("emit_test.bat");
    let _ = std::fs::remove_file(&path);
    let run = wbat64(&[
        "box",
        "@dialogs.txt:one_button",
        "--keys",
        "enter",
        "--emit-file",
        "emit_test.bat",
        "--prefix",
        "W_",
    ]);
    assert_eq!(run.code, 1);
    let text = std::fs::read_to_string(&path).unwrap();
    let _ = std::fs::remove_file(&path);
    assert!(text.contains("set \"W_WBAT=go\""));
    assert!(text.contains("set \"W_?=1\""));
}

#[test]
fn hostile_values_are_escaped_for_bat() {
    // Type: a % & ! ^ " then accept.
    let run = wbat64(&[
        "box",
        "@forms.txt:hostile",
        "--keys",
        "a,%,&,!,^,quote,tab,enter",
        "--emit",
        "bat",
    ]);
    assert_eq!(run.code, 1, "{}", run.stderr);
    assert!(
        run.stdout.contains("set \"value=a%%&!^q\""),
        "{}",
        run.stdout
    );
}

#[test]
fn delayed_expansion_escapes_bang() {
    let run = wbat64(&[
        "box",
        "@forms.txt:hostile",
        "--keys",
        "!,!,tab,enter",
        "--emit",
        "bat",
        "--delayed",
    ]);
    assert_eq!(run.code, 1);
    assert!(
        run.stdout.contains("set \"value=^^!^^!\""),
        "{}",
        run.stdout
    );
}

#[test]
fn ps1_and_sh_quoting() {
    let run = wbat64(&[
        "box",
        "@forms.txt:hostile",
        "--keys",
        "',tab,enter",
        "--emit",
        "ps1",
    ]);
    assert_eq!(run.code, 1, "{}", run.stderr);
    assert!(run.stdout.contains("$env:value = ''''"), "{}", run.stdout);
}

#[test]
fn esc_keeps_values_when_configured() {
    let run = wbat64_with_env(
        &[
            "box",
            "@forms.txt:required",
            "--keys",
            "esc",
            "--emit",
            "bat",
        ],
        &[("WESCinput", "1"), ("name", "Ada")],
    );
    assert_eq!(run.code, 100);
    assert!(run.stdout.contains("set \"name=Ada\""), "{}", run.stdout);
}

#[test]
fn el_sets_exit_code() {
    let run = wbat64(&["el", "42"]);
    assert_eq!(run.code, 42);
}

#[test]
fn stdout_expands_variables() {
    let run = wbat64(&["stdout", "@dialogs.txt:info", "--set", "USERNAME=Ada"]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(
        run.stdout.contains("wbat64 0.1 running for Ada"),
        "{}",
        run.stdout
    );
}

#[test]
fn text_and_fill_render_on_screen() {
    let run = wbat64(&[
        "cls",
        "(white on blue)",
        "--and",
        "text",
        "2,4",
        "(+white on red)",
        "Hello ^World",
        "--and",
        "fill",
        "8,2",
        "(2,6)",
        "#",
        "--dump-screen",
        "--size",
        "40x12",
    ]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    let lines: Vec<&str> = run.stdout.lines().collect();
    assert!(lines[1].contains("Hello"), "{}", run.stdout);
    assert!(lines[2].contains("World"), "{}", run.stdout);
    assert_eq!(lines[7].trim(), "######");
}

#[test]
fn fill_without_a_pattern_only_recolors() {
    let run = wbat64(&[
        "text",
        "1,1",
        "hello",
        "--and",
        "fill",
        "1,1",
        "(1,5)",
        "(red on blue)",
        "--dump-screen",
        "--size",
        "20x5",
    ]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(
        run.stdout.contains("hello"),
        "recolor must keep the text: {}",
        run.stdout
    );
}

#[test]
fn fix_keeps_frame_visible() {
    let run = wbat64(&[
        "box",
        "@forms.txt:fixed",
        "--keys",
        "enter",
        "--fix",
        "--dump-screen",
        "--size",
        "40x10",
    ]);
    assert_eq!(run.code, 1);
    assert!(run.stdout.contains("Fixed box"), "{}", run.stdout);
}

#[test]
fn compat_mode_rejects_wide_box() {
    let run = wbat64(&[
        "box",
        "@forms.txt:wide",
        "--compat",
        "--keys",
        "enter",
        "--size",
        "100x20",
    ]);
    assert_eq!(run.code, 255);
    assert!(run.stderr.contains("maximum 78"), "{}", run.stderr);
}

#[test]
fn mouse_click_activates_button() {
    let run = wbat64(&[
        "box",
        "@forms.txt:fixed",
        "--pos",
        "1,1",
        "--keys",
        "click:2x1",
        "--emit",
        "json",
    ]);
    assert_eq!(run.code, 1, "{}", run.stderr);
    assert_eq!(json(&run)["button"]["name"], "close");
}

#[test]
fn right_click_outside_escapes() {
    let run = wbat64(&[
        "box",
        "@forms.txt:fixed",
        "--pos",
        "1,1",
        "--keys",
        "rclick:60x20",
        "--emit",
        "json",
    ]);
    assert_eq!(run.code, 100);
    assert_eq!(json(&run)["escaped"], true);
}

#[test]
fn drag_moves_the_box() {
    let run = wbat64(&[
        "box",
        "@forms.txt:fixed",
        "--pos",
        "1,1",
        "--keys",
        "click:3x0,drag:13x4,esc",
        "--dump-screen",
        "--size",
        "40x12",
    ]);
    assert_eq!(run.code, 100);
    let lines: Vec<&str> = run.stdout.lines().collect();
    let frame_line = lines.iter().position(|l| l.contains("Fixed box")).unwrap();
    assert!(frame_line >= 4, "frame did not move: {}", run.stdout);
}

#[test]
fn chained_commands_and_ordering() {
    let run = wbat64(&[
        "cls", "--and", "text", "1,1", "hello", "--and", "box", "ok", "--keys", "enter", "--emit",
        "json",
    ]);
    assert_eq!(run.code, 1);
}

#[test]
fn interactive_command_must_be_last() {
    let run = wbat64(&["box", "ok", "!", "cls"]);
    assert_eq!(run.code, 255);
    assert!(run.stderr.contains("last"));
}

#[test]
fn unknown_option_is_rejected() {
    let run = wbat64(&["box", "ok", "--frobnicate"]);
    assert_eq!(run.code, 255);
    assert!(run.stderr.contains("unknown option"));
}

#[test]
fn unknown_command_is_rejected() {
    let run = wbat64(&["frobnicate"]);
    assert_eq!(run.code, 255);
    assert!(run.stderr.contains("unknown command"));
}

#[test]
fn ansi_probe_runs() {
    let run = wbat64(&["ansi?"]);
    assert!(run.code == 0 || run.code == 1);
}

#[test]
fn title_and_close_button_render() {
    let run = wbat64(&[
        "box",
        "@dialogs.txt:one_button",
        "--title",
        "Custom",
        "--keys",
        "esc",
        "--dump-screen",
        "--size",
        "40x12",
    ]);
    assert!(run.stdout.contains("Custom"));
}

#[test]
fn divider_and_highlights_render() {
    let run = wbat64(&[
        "text",
        "1,1",
        "@dialogs.txt:styled",
        "--dump-screen",
        "--size",
        "40x10",
    ]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(
        run.stdout.contains("Normal highlighted text"),
        "{}",
        run.stdout
    );
    assert!(run.stdout.contains("─────"), "{}", run.stdout);
    assert!(run.stdout.contains("literal [ % ~ #"), "{}", run.stdout);
}

#[cfg(windows)]
#[test]
fn auto_emit_bat_when_stdout_ends_in_bat() {
    let dir = fixtures();
    let file = dir.join("auto_emit.bat");
    let script = dir.join("run_auto.bat");
    let _ = std::fs::remove_file(&file);
    let exe = env!("CARGO_BIN_EXE_wbat64");
    std::fs::write(
        &script,
        format!(
            "@echo off\r\n\"{exe}\" box @dialogs.txt:one_button --keys enter > auto_emit.bat\r\n"
        ),
    )
    .unwrap();
    let status = Command::new("cmd")
        .current_dir(&dir)
        .args(["/c", "run_auto.bat"])
        .status()
        .unwrap();
    let text = std::fs::read_to_string(&file).unwrap_or_default();
    let _ = std::fs::remove_file(&file);
    let _ = std::fs::remove_file(&script);
    assert_eq!(status.code(), Some(1));
    assert!(text.contains("set \"WBAT=go\""), "{text}");
}
