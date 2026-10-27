//! Colours, byte-exact padding and score helpers shared by the screens.

use std::collections::BTreeSet;
use std::io::{self, Write};

use phx_audit::store::{Run, sev_name};
use ratatui::crossterm::terminal;

pub(super) const RED: &str = "\x1b[31m";
pub(super) const YEL: &str = "\x1b[33m";
pub(super) const GRN: &str = "\x1b[32m";
pub(super) const BLU: &str = "\x1b[34m";
pub(super) const B: &str = "\x1b[1m";
pub(super) const I: &str = "\x1b[3m";
pub(super) const N: &str = "\x1b[0m";
pub(super) const EL: &str = "\x1b[K";
pub(super) const CLEAR: &str = "\x1b[H\x1b[2J\x1b[3J";
pub(super) const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub(super) fn icon(sev: &str) -> &'static str {
    match sev {
        "fail" => "✗",
        "warn" => "!",
        "pass" => "✓",
        "skip" => "–",
        _ => "i",
    }
}

pub(super) fn label(sev: &str) -> &'static str {
    match sev {
        "fail" => "FAIL",
        "warn" => "WARN",
        "pass" => "PASS",
        "skip" => "SKIP",
        _ => "NOTE",
    }
}

/// bash `printf '%-Ns'`: pads to N bytes.
pub(super) fn lpad(s: &str, w: usize) -> String {
    format!("{s}{}", " ".repeat(w.saturating_sub(s.len())))
}

/// bash `printf '%Ns'`.
pub(super) fn rpad(s: &str, w: usize) -> String {
    format!("{}{s}", " ".repeat(w.saturating_sub(s.len())))
}

pub(super) fn trunc(t: &str, m: usize) -> String {
    if t.chars().count() > m {
        let mut s: String = t.chars().take(m.saturating_sub(1)).collect();
        s.push('…');
        s
    } else {
        t.to_owned()
    }
}

pub(super) fn mmss(s: u64) -> String {
    format!("{}:{:02}", s / 60, s % 60)
}

pub(super) fn cols() -> usize {
    terminal::size().map_or(80, |(c, _)| usize::from(c))
}

pub(super) fn lines() -> usize {
    terminal::size().map_or(24, |(_, r)| usize::from(r))
}

pub(super) fn width() -> usize {
    let c = cols();
    if c > 96 { 92 } else { c.saturating_sub(4) }
}

pub(super) fn print(s: &str) {
    let mut out = io::stdout();
    let _ = out.write_all(s.as_bytes());
    let _ = out.flush();
}

pub(super) fn score_of(f: usize, w: usize) -> usize {
    100usize.saturating_sub(10 * f + 2 * w)
}

pub(super) fn grade_of(s: usize) -> char {
    match s {
        90.. => 'A',
        80..=89 => 'B',
        70..=79 => 'C',
        60..=69 => 'D',
        _ => 'F',
    }
}

pub(super) fn grade_color(g: char) -> &'static str {
    match g {
        'A' | 'B' => GRN,
        'C' | 'D' => YEL,
        _ => RED,
    }
}

/// Raw counts (fail, warn, pass, info, skip); this front end ignores accepted risks.
pub(super) fn counts(run: &Run) -> [usize; 5] {
    let mut c = [0; 5];
    for i in &run.items {
        let k = match sev_name(i.finding.sev) {
            "fail" => 0,
            "warn" => 1,
            "pass" => 2,
            "info" => 3,
            _ => 4,
        };
        if let Some(v) = c.get_mut(k) {
            *v += 1;
        }
    }
    c
}

pub(super) fn issue_keys(run: &Run) -> BTreeSet<String> {
    run.items
        .iter()
        .filter(|i| matches!(sev_name(i.finding.sev), "fail" | "warn"))
        .map(|i| {
            format!(
                "{}\t{}\t{}",
                sev_name(i.finding.sev),
                i.finding.section,
                i.finding.msg
            )
        })
        .collect()
}

pub(super) fn sev_color<'a>(sev: &str, muted: &'a str) -> &'a str {
    match sev {
        "fail" => RED,
        "warn" => YEL,
        "pass" => GRN,
        "skip" => muted,
        _ => BLU,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn pads_by_bytes_like_bash_printf() {
        assert_eq!(super::lpad("é", 5), "é   ");
        assert_eq!(
            super::trunc("Secrets & credentials in home", 22),
            "Secrets & credentials…"
        );
        assert_eq!(
            super::lpad("Secrets & credentials…", 22),
            "Secrets & credentials…"
        );
    }
}
