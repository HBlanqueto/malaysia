use std::rc::Rc;
use std::time::Duration;

use bk::adornments::Badge;
use bk::prelude::*;
use bk::theme;
use ctdesign::{
    DOCK_PROGRESS_HEIGHT, DOCK_RADIUS, DOCK_WINDOW_BADGE, DOCK_WINDOW_FRAME, DOCK_WINDOW_ICON,
};
use freya_components::image_viewer::{DecodeMode, ImageViewer};
use torin::prelude::Position;

use super::{Thumbnail, app_icon_named, downloads_icon, thumbnail, trash_icon};
use crate::desktop::DesktopEntry;

#[derive(PartialEq)]
pub(crate) struct DockIcon {
    pub(crate) id: String,
    pub(crate) entries: Rc<Vec<DesktopEntry>>,
    pub(crate) theme_revision: u64,
    pub(crate) size: f32,
    pub(crate) raster_size: f32,
    pub(crate) glass_plate: bool,
    pub(crate) badge: Option<i64>,
    pub(crate) progress: Option<f64>,
}

impl Component for DockIcon {
    fn render(&self) -> impl IntoElement {
        let resolved = theme::resolved();
        rect()
            .width(Size::px(self.size))
            .height(Size::px(self.size))
            .child(app_icon_named(
                &self.id,
                &self.entries,
                self.size,
                self.raster_size,
                self.glass_plate,
            ))
            .maybe_child(self.badge.map(|count| {
                rect()
                    .position(Position::new_absolute().top(0.0).right(0.0))
                    .child(Badge::new(count.to_string()))
            }))
            .maybe_child(self.progress.map(|progress| {
                progress_bar(
                    self.size,
                    progress,
                    theme::color(resolved.shell.tile),
                    theme::color(resolved.shell.tile_active),
                )
            }))
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Place {
    Downloads,
    Trash { full: bool },
}

#[derive(PartialEq)]
pub(crate) struct DockPlace {
    pub(crate) place: Place,
    pub(crate) theme_revision: u64,
    pub(crate) size: f32,
    pub(crate) raster_size: f32,
}

impl Component for DockPlace {
    fn render(&self) -> impl IntoElement {
        match self.place {
            Place::Downloads => downloads_icon(self.size),
            Place::Trash { full } => trash_icon(full, self.size, self.raster_size),
        }
    }
}

#[derive(PartialEq)]
pub(crate) struct DockWindow {
    pub(crate) id: u64,
    pub(crate) key: String,
    pub(crate) entries: Rc<Vec<DesktopEntry>>,
    pub(crate) theme_revision: u64,
    pub(crate) picture: Option<Thumbnail>,
    pub(crate) size: f32,
    pub(crate) length: f32,
    pub(crate) raster_size: f32,
}

impl Component for DockWindow {
    fn render(&self) -> impl IntoElement {
        let resolved = theme::resolved();
        let scaled_icon = |share: f32| {
            app_icon_named(
                &self.key,
                &self.entries,
                self.size * share,
                self.raster_size * share,
                false,
            )
        };
        rect()
            .width(Size::px(self.length))
            .height(Size::px(self.size))
            .center()
            .child(match &self.picture {
                Some(picture) => {
                    let (width, height) = thumbnail::body(picture.size, self.size);
                    ImageViewer::new((self.id, picture.png.clone()))
                        .decode_mode(DecodeMode::Source)
                        .asset_age(Duration::ZERO)
                        .width(Size::px(width))
                        .height(Size::px(height))
                        .into_element()
                }
                None => rect()
                    .width(Size::px(self.size * DOCK_WINDOW_FRAME))
                    .height(Size::px(self.size * DOCK_WINDOW_FRAME))
                    .corner_radius(self.size * DOCK_RADIUS / 2.0)
                    .background(theme::color(resolved.shell.tile))
                    .center()
                    .child(scaled_icon(DOCK_WINDOW_ICON))
                    .into_element(),
            })
            .maybe_child(self.picture.is_some().then(|| {
                rect()
                    .position(Position::new_absolute().right(0.0).bottom(0.0))
                    .child(scaled_icon(DOCK_WINDOW_BADGE))
            }))
    }
}

fn progress_bar(size: f32, progress: f64, track: Color, fill: Color) -> Rect {
    let inset = size / 8.0;
    let width = size - 2.0 * inset;
    rect()
        .position(Position::new_absolute().bottom(inset).left(inset))
        .width(Size::px(width))
        .height(Size::px(DOCK_PROGRESS_HEIGHT))
        .corner_radius(DOCK_PROGRESS_HEIGHT / 2.0)
        .background(track)
        .child(
            rect()
                .width(Size::px(width * progress.clamp(0.0, 1.0) as f32))
                .height(Size::fill())
                .corner_radius(DOCK_PROGRESS_HEIGHT / 2.0)
                .background(fill),
        )
}
