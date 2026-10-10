use std::fs::File;
use std::io::{self, Write};

use secaudit_core::{Finding, Severity};

/// Colours, or empty strings when output isn't a terminal.
pub(super) struct Style {
    pub(super) red: &'static str,
    pub(super) green: &'static str,
    pub(super) yellow: &'static str,
    pub(super) blue: &'static str,
    pub(super) dim: &'static str,
    pub(super) bold: &'static str,
    pub(super) reset: &'static str,
}

impl Style {
    pub(super) fn new(color: bool) -> Self {
        let c = |code| if color { code } else { "" };
        Self {
            red: c("\x1b[31m"),
            green: c("\x1b[32m"),
            yellow: c("\x1b[33m"),
            blue: c("\x1b[34m"),
            dim: c("\x1b[2m"),
            bold: c("\x1b[1m"),
            reset: c("\x1b[0m"),
        }
    }
}

/// Writes to the terminal and, with `--report`, to a colourless copy.
pub(super) struct Out {
    pub(super) report: Option<File>,
}

impl Out {
    pub(super) fn line(&mut self, s: &str) -> io::Result<()> {
        let mut stdout = io::stdout().lock();
        writeln!(stdout, "{s}")?;
        if let Some(f) = &mut self.report {
            writeln!(f, "{}", strip_ansi(s))?;
        }
        Ok(())
    }
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub(super) fn print_finding(out: &mut Out, st: &Style, f: &Finding) -> io::Result<()> {
    let (tag, color) = match f.sev {
        Severity::Pass => ("PASS", st.green),
        Severity::Warn => ("WARN", st.yellow),
        Severity::Fail => ("FAIL", st.red),
        Severity::Skip => ("SKIP", st.dim),
        _ => ("INFO", st.blue),
    };
    out.line(&format!("  {color}[{tag}]{} {}", st.reset, f.msg))?;
    for line in f.detail.lines() {
        out.line(&format!("         {line}"))?;
    }
    if !f.fix.is_empty() {
        out.line(&format!("         {}> fix: {}{}", st.dim, f.fix, st.reset))?;
    }
    if !f.hint.is_empty() {
        out.line(&format!("         {}> {}{}", st.dim, f.hint, st.reset))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn strips_colours_for_the_report_copy() {
        assert_eq!(
            super::strip_ansi("\x1b[1m\x1b[34m== A ==\x1b[0m"),
            "== A =="
        );
    }
}
