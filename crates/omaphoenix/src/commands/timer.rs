use phx_core::schedule;

use crate::cli::TimerCmd;
use crate::out::say;

pub fn run(command: TimerCmd) -> Result<(), String> {
    match command {
        TimerCmd::On { interval } => {
            schedule::on(&interval)?;
            say(&schedule::status());
        }
        TimerCmd::Off => schedule::off()?,
        TimerCmd::Status => say(&schedule::status()),
    }
    Ok(())
}
