use std::os::unix::process::CommandExt;

/// Replaces this process with `omaphoenix-gui` from the same directory.
pub fn open() -> Result<(), String> {
    let gui = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join("omaphoenix-gui")))
        .filter(|g| g.is_file())
        .ok_or("the desktop app isn't installed; rerun install.sh and choose it (or use: omaphoenix tui)")?;
    Err(format!(
        "{}: {}",
        gui.display(),
        std::process::Command::new(&gui).exec()
    ))
}
