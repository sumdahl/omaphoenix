use std::io::{self, IsTerminal, Write};

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::crossterm::terminal;
use zeroize::Zeroizing;

fn read_password(prompt: &str) -> Result<Zeroizing<String>, String> {
    let _ = write!(io::stderr(), "{prompt}");
    let _ = io::stderr().flush();
    terminal::enable_raw_mode().map_err(|e| e.to_string())?;
    let mut s = Zeroizing::new(String::new());
    let result = loop {
        match event::read() {
            Ok(Event::Key(k)) if k.kind == KeyEventKind::Press => match k.code {
                KeyCode::Enter => break Ok(()),
                KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                    break Err("cancelled".to_owned());
                }
                KeyCode::Backspace => {
                    s.pop();
                }
                KeyCode::Char(c) => s.push(c),
                _ => {}
            },
            Ok(_) => {}
            Err(e) => break Err(e.to_string()),
        }
    };
    let _ = terminal::disable_raw_mode();
    let _ = writeln!(io::stderr());
    result.map(|()| s)
}

/// `OMAPHOENIX_PASSPHRASE` (scripts, tests), else asked without echo.
pub fn passphrase(confirm: bool) -> Result<Zeroizing<String>, String> {
    if let Ok(p) = std::env::var("OMAPHOENIX_PASSPHRASE")
        && !p.is_empty()
    {
        return Ok(Zeroizing::new(p));
    }
    if !io::stdin().is_terminal() {
        return Err(
            "no terminal to ask for the secrets passphrase; set OMAPHOENIX_PASSPHRASE".into(),
        );
    }
    let p = read_password("secrets passphrase: ")?;
    if !confirm {
        return Ok(p);
    }
    if p.chars().count() < 12 {
        return Err("use at least 12 characters; this passphrase guards your SSH keys".into());
    }
    if *read_password("repeat passphrase: ")? != *p {
        return Err("passphrases don't match".into());
    }
    Ok(p)
}
