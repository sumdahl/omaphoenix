//! Compiles the Slint UI into Rust at build time.
fn main() {
    if let Err(e) = slint_build::compile("ui/app.slint") {
        // A build script reports through stderr and its exit code.
        std::eprintln!("slint compile: {e}");
        std::process::exit(1);
    }
}
