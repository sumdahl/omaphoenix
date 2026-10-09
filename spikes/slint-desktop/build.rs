//! Compiles the Slint UI into Rust at build time.
fn main() {
    slint_build::compile("ui/app.slint").expect("slint compile");
}
