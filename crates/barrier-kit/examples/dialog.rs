use std::time::Instant;

use barrier_kit::controls::Button;
use barrier_kit::platform::{Decorations, WindowControls, WindowOptions, run_window};
use barrier_kit::titlebar::{Titlebar, resize_edges};
use freya_core::integration::AppComponent;
use freya_core::prelude::*;
use torin::prelude::{Alignment, Direction, Gaps, Size};

fn theme_text() -> Color {
    let palette = barrier_kit::theme::resolved();
    let colors = vow_appearance::palette(palette.scheme);
    let [r, g, b, a] = colors.text.focused.to_rgba8();
    Color::from_argb(a, r, g, b)
}

fn theme_background() -> Color {
    let palette = barrier_kit::theme::resolved();
    let colors = vow_appearance::palette(palette.scheme);
    let [r, g, b, a] = colors.titlebar_bg.focused.to_rgba8();
    Color::from_argb(a, r, g, b)
}

struct Parent {
    started: Instant,
}

impl App for Parent {
    fn render(&self) -> impl IntoElement {
        let started = self.started;
        use_hook(move || {
            if std::env::var_os("YUKI_WINDOW_TIMING").is_some() {
                eprintln!("dialog parent: first render after {:?}", started.elapsed());
            }
        });
        let controls = WindowControls::get();
        let text = theme_text();
        let background = theme_background();
        rect()
            .width(Size::fill())
            .height(Size::fill())
            .background(background)
            .direction(Direction::Vertical)
            .child(Titlebar::new().on_close(|()| std::process::exit(0)))
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .center()
                    .direction(Direction::Vertical)
                    .spacing(16.0)
                    .padding(Gaps::new(24.0, 24.0, 24.0, 24.0))
                    .main_align(Alignment::Center)
                    .child(label().text("Ventana padre").font_size(18.0).color(text))
                    .child(
                        Button::new()
                            .on_press(move |_| {
                                let parent = controls.surface_id();
                                controls.open_window(
                                    AppComponent::new(Dialog {
                                        started: Instant::now(),
                                    }),
                                    WindowOptions::new("Diálogo", "demo.dialog")
                                        .decorations(Decorations::Server)
                                        .size((360, 180))
                                        .parent(Some(parent)),
                                );
                            })
                            .child(label().text("Abrir diálogo").color(text)),
                    ),
            )
            .child(resize_edges())
    }
}

struct Dialog {
    started: Instant,
}

impl App for Dialog {
    fn render(&self) -> impl IntoElement {
        let started = self.started;
        use_hook(move || {
            if std::env::var_os("YUKI_WINDOW_TIMING").is_some() {
                eprintln!("dialog child: first render after {:?}", started.elapsed());
            }
        });
        let text = theme_text();
        let background = theme_background();
        rect()
            .width(Size::fill())
            .height(Size::fill())
            .background(background)
            .direction(Direction::Vertical)
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .center()
                    .direction(Direction::Vertical)
                    .spacing(16.0)
                    .padding(Gaps::new(24.0, 24.0, 24.0, 24.0))
                    .main_align(Alignment::Center)
                    .child(
                        label()
                            .text("¿Confirmas la acción?")
                            .font_size(16.0)
                            .color(text),
                    )
                    .child(
                        rect()
                            .direction(Direction::Horizontal)
                            .spacing(12.0)
                            .child(
                                Button::new()
                                    .on_press(|_| std::process::exit(0))
                                    .child(label().text("Aceptar").color(text)),
                            )
                            .child(
                                Button::new()
                                    .on_press(|_| std::process::exit(0))
                                    .child(label().text("Cancelar").color(text)),
                            ),
                    ),
            )
    }
}

fn main() {
    run_window(
        AppComponent::new(Parent {
            started: Instant::now(),
        }),
        WindowOptions::new("Padre", "demo.parent")
            .decorations(Decorations::Client)
            .size((640, 420)),
    )
    .expect("dialog");
}
