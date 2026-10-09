// Floor test: idle memory + AT-SPI exposure of the smallest Pulse-like window.
use iced::widget::{button, column, text};
use iced::{Color, Element};

#[derive(Default)]
struct Pulse {
    snaps: u32,
}
#[derive(Debug, Clone)]
enum Msg {
    Snap,
}
fn update(p: &mut Pulse, m: Msg) {
    match m {
        Msg::Snap => p.snaps += 1,
    }
}
fn view(p: &Pulse) -> Element<'_, Msg> {
    column![
        text("You're protected.").size(20).color(Color::from_rgb8(0xe6, 0xd9, 0xdb)),
        text(format!("Snapshots taken: {}", p.snaps)),
        button("Take snapshot").on_press(Msg::Snap),
    ]
    .spacing(12)
    .padding(24)
    .into()
}
fn main() -> iced::Result {
    iced::application(Pulse::default, update, view).title("OmaPhoenix").run()
}
