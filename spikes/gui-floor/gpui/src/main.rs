// Floor test: idle memory + AT-SPI exposure of the smallest Pulse-like window.
use gpui::{
    App, Application, Context, IntoElement, ParentElement, Render, Styled, Window, WindowOptions,
    div, prelude::*, px, rgb,
};

struct Pulse {
    snaps: u32,
}

impl Render for Pulse {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .p(px(24.))
            .size_full()
            .bg(rgb(0x2c2525))
            .text_color(rgb(0xe6d9db))
            .child(div().text_xl().child("You're protected."))
            .child(format!("Snapshots taken: {}", self.snaps))
            .child(
                div()
                    .id("snap")
                    .px_3()
                    .py_2()
                    .bg(rgb(0xf38d70))
                    .text_color(rgb(0x181414))
                    .rounded_md()
                    .child("Take snapshot")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.snaps += 1;
                        cx.notify();
                    })),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let _ = cx.open_window(WindowOptions::default(), |_, cx| cx.new(|_| Pulse { snaps: 0 }));
        cx.activate(true);
    });
}
