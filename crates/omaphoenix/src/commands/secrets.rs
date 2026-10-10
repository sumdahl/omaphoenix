use std::io::{self, Write};

use phx_core::App;

use crate::out::say;
use crate::prompt::passphrase;

pub fn init(app: &App<'_>) -> Result<(), String> {
    let _ = writeln!(
        io::stderr(),
        "This passphrase unlocks your secrets on restore. It is never stored;\nlose it and the encrypted secrets are gone (nothing else is)."
    );
    let p = passphrase(true)?;
    app.init_secrets(&p).map_err(|e| e.to_string())?;
    say("✓ secrets set up; ~/.ssh is now included (private keys encrypted)");
    Ok(())
}
