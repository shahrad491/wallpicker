//! A viewport-sized grid; offscreen rows are represented by spacers.
use crate::{app::App, message::Message};
use iced::{
    ContentFit, Element, Length,
    widget::{Space, button, column, container, image, mouse_area, row, scrollable, text},
};

pub(crate) fn view(app: &App) -> Element<'_, Message> {
    if app.scanning && app.scan.pool.is_empty() {
        return container(
            column![
                text("Gathering your wallpapers…").size(22),
                text("Sources are scanned in the background.").size(14)
            ]
            .spacing(12),
        )
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into();
    }
    if app.visible.is_empty() {
        return container(
            column![
                text(if app.library.sources.is_empty() {
                    "Your collection starts here"
                } else {
                    "No wallpapers here yet"
                })
                .size(24),
                text(if app.library.sources.is_empty() {
                    "Add a folder to browse its images. Your files stay exactly where they are."
                } else {
                    "Try another category, clear your search, or rescan your sources."
                })
                .size(14),
                button("Add source").on_press(Message::OpenDialog(crate::app::Dialog::AddSource))
            ]
            .spacing(16),
        )
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into();
    }
    let columns = app.columns();
    let row_height = app.row_height();
    let rows = app.visible.len().div_ceil(columns);
    let window = app.row_window();
    let start = window.start;
    let end = window.end;
    let tile_width = app.tile_width();
    let preview_height = (tile_width - 16.0) * 9.0 / 16.0;
    let mut grid = column![Space::new().height(start as f32 * row_height)].spacing(0);
    for r in start..end {
        let mut tiles = row![].spacing(12);
        for path in app.visible.iter().skip(r * columns).take(columns) {
            let selected = app.selected.contains(path);
            let hovered = app.hovered.as_ref() == Some(path);
            let favorite = app.library.favorites.contains(path);
            let preview: Element<'_, Message> = if let Some(cached) = app.thumbnails.get(path) {
                image(cached.clone())
                    .width(Length::Fill)
                    .height(preview_height)
                    .content_fit(ContentFit::Cover)
                    .into()
            } else {
                container(
                    text(if app.thumbnail_errors.contains(path) {
                        "Preview unavailable"
                    } else {
                        "Loading preview…"
                    })
                    .size(12),
                )
                .center_x(Length::Fill)
                .center_y(preview_height)
                .into()
            };
            let mut actions = row![
                text(if Some(path) == app.current.as_ref() {
                    "● Current"
                } else if selected {
                    "Selected"
                } else {
                    ""
                })
                .size(11),
                Space::new().width(Length::Fill)
            ]
            .align_y(iced::Alignment::Center);
            if hovered || selected || favorite {
                actions = actions
                    .push(
                        button(if favorite { "★" } else { "☆" })
                            .padding([2, 6])
                            .style(button::text)
                            .on_press(Message::Favorite(path.clone())),
                    )
                    .push(
                        button("+")
                            .padding([2, 6])
                            .style(button::text)
                            .on_press(Message::AddImage(path.clone())),
                    )
                    .push(
                        button("⋯")
                            .padding([2, 6])
                            .style(button::text)
                            .on_press(Message::Context(path.clone())),
                    );
            }
            let caption = path
                .file_name()
                .unwrap_or(path.as_os_str())
                .to_string_lossy();
            let caption = if caption.chars().count() > 27 {
                format!("{}…", caption.chars().take(26).collect::<String>())
            } else {
                caption.into_owned()
            };
            let tile = container(column![preview, text(caption).size(12), actions].spacing(4))
                .padding(8)
                .width(tile_width)
                .height(row_height - 12.0)
                .style(move |t| crate::theme::tile(t, selected));
            tiles = tiles.push(
                mouse_area(tile)
                    .on_press(Message::Click(path.clone()))
                    .on_double_click(Message::Apply(path.clone()))
                    .on_right_press(Message::Context(path.clone()))
                    .on_enter(Message::Hover(Some(path.clone())))
                    .on_exit(Message::Hover(None)),
            );
        }
        grid = grid.push(container(tiles).height(row_height));
    }
    grid = grid.push(Space::new().height((rows - end) as f32 * row_height));
    scrollable(grid)
        .id("gallery")
        .height(Length::Fill)
        .on_scroll(|viewport| Message::Scroll(viewport.absolute_offset().y))
        .into()
}
