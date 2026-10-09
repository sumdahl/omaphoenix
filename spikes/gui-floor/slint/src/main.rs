// Floor test: idle memory + AT-SPI exposure of the smallest Pulse-like window.
slint::slint! {
    import { Button, VerticalBox } from "std-widgets.slint";
    export component App inherits Window {
        title: "OmaPhoenix";
        preferred-width: 600px;
        preferred-height: 400px;
        background: #2c2525;
        in-out property <int> snaps: 0;
        VerticalBox {
            Text { text: "You're protected."; font-size: 20px; color: #e6d9db; accessible-role: text; }
            Text { text: "Snapshots taken: " + snaps; color: #c3b7b8; }
            Button { text: "Take snapshot"; clicked => { snaps += 1; } }
        }
    }
}
fn main() -> Result<(), slint::PlatformError> {
    App::new()?.run()
}
