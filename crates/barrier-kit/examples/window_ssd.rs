use std::time::Instant;

use barrier_kit::controls::Button;
use barrier_kit::platform::{Decorations, WindowOptions, run_window};
use barrier_kit::titlebar::{Titlebar, resize_edges};
use freya_core::integration::AppComponent;
use freya_core::prelude::*;
use torin::prelude::{Alignment, Direction, Gaps, Size};

struct Demo {
    started: Instant,
}

impl App for Demo {
    fn render(&self) -> impl IntoElement {
        let started = self.started;
        use_hook(move || {
            if std::env::var_os("YUKI_WINDOW_TIMING").is_some() {
                eprintln!("window_ssd: first render after {:?}", started.elapsed());
            }
        });
        let palette = barrier_kit::theme::resolved();
        let colors = vow_appearance::palette(palette.scheme);
        let [r, g, b, a] = colors.titlebar_bg.focused.to_rgba8();
        let background = Color::from_argb(a, r, g, b);
        let [tr, tg, tb, ta] = colors.text.focused.to_rgba8();
        let text = Color::from_argb(ta, tr, tg, tb);
        rect()
            .width(Size::fill())
            .height(Size::fill())
            .background(background)
            .direction(Direction::Vertical)
            .child(Titlebar::new().on_close(|()| {
                std::process::exit(0);
            }))
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .center()
                    .direction(Direction::Vertical)
                    .spacing(12.0)
                    .padding(Gaps::new(24.0, 24.0, 24.0, 24.0))
                    .main_align(Alignment::Center)
                    .child(
                        label()
                            .text("Ventana SSD de Yuki")
                            .font_size(16.0)
                            .color(text),
                    )
                    .child(
                        Button::new()
                            .on_press(|_| {})
                            .child(label().text("Un botón").color(text)),
                    ),
            )
            .child(resize_edges())
    }
}

fn main() {
    let app = AppComponent::new(Demo {
        started: Instant::now(),
    });
    run_window(
        app,
        WindowOptions::new("Yuki SSD", "demo.ssd")
            .decorations(Decorations::Server)
            .size((640, 420)),
    )
    .expect("window_ssd");
}
