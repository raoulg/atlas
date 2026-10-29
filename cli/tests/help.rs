//! What `atlas-code --help` prints: the banner, a picture of seven lines, whole at any width the help is
//! wrapped to.

use std::process::Command;

fn help(columns: &str) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_atlas-code")).arg("--help").env("COLUMNS", columns).output().expect("atlas runs");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).expect("utf-8")
}

/// The text of a line as it shows: its escape codes taken out.
fn shown(line: &str) -> String {
    let mut s = String::new();
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            chars.by_ref().find(|c| c.is_ascii_alphabetic());
        } else {
            s.push(c);
        }
    }
    s
}

#[test]
fn the_banner_is_whole_at_every_width() {
    for columns in ["60", "80", "120", "200"] {
        let text = help(columns);
        let lines: Vec<String> = text.lines().take(7).map(shown).collect();
        assert!(lines.iter().all(|l| l.chars().count() >= 49), "COLUMNS={columns}: a banner line is broken\n{}", lines.join("\n"));
        assert!(lines[2].trim_end().ends_with("atlas"), "COLUMNS={columns}: the name stands beside the third line\n{}", lines.join("\n"));
        assert_eq!(text.lines().nth(7).map(str::trim), Some(""), "COLUMNS={columns}: seven lines, then the about\n{text}");
    }
}
