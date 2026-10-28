//! Pinned built-ins, manual tags, merged folders, and source management.
use crate::{
    app::{App, Dialog},
    message::Message,
    model::CategoryRef,
};
use iced::{
    Element, Length,
    widget::{Space, button, column, container, row, scrollable, text},
};
pub(crate) fn view(app: &App) -> Element<'_, Message> {
    let nav = |label: String, category: CategoryRef| {
        button(text(label).size(14))
            .width(Length::Fill)
            .padding([10, 12])
            .style(if app.category == category {
                button::primary
            } else {
                button::text
            })
            .on_press(Message::Category(category))
    };
    let mut items = column![
        text("WALLPICKER").size(18),
        text("A change of scenery.")
            .size(12)
            .color(crate::theme::color(&app.config.theme.muted)),
        Space::new().height(20),
        nav(
            format!("▧  All wallpapers     {}", app.scan.pool.len()),
            CategoryRef::All
        ),
        nav(
            format!("☆  Favorites     {}", app.library.favorites.len()),
            CategoryRef::Favorites
        ),
        Space::new().height(18),
        row![
            text("CATEGORIES").size(11),
            Space::new().width(Length::Fill),
            button("+")
                .on_press(Message::OpenDialog(Dialog::NewCategory))
                .style(button::text)
        ]
        .align_y(iced::Alignment::Center)
    ]
    .spacing(6);
    for category in &app.library.categories {
        let mut entry = row![nav(
            format!("{}   {}", category.name, category.images.len()),
            CategoryRef::Manual { id: category.id }
        )]
        .spacing(3);
        if app.category == (CategoryRef::Manual { id: category.id }) {
            entry = entry.push(
                button("⋯")
                    .on_press(Message::OpenDialog(Dialog::Rename(category.id)))
                    .style(button::text),
            );
        }
        items = items.push(entry);
    }
    items = items
        .push(Space::new().height(16))
        .push(text("FOLDERS").size(11));
    for (name, paths) in &app.scan.auto_categories {
        if app.library.hidden_auto_categories.contains(name) {
            continue;
        }
        items = items.push(
            row![
                nav(
                    format!("{}   {}", name, paths.len()),
                    CategoryRef::Auto { name: name.clone() }
                ),
                button("−")
                    .style(button::text)
                    .on_press(Message::HideFolder(name.clone()))
            ]
            .spacing(2),
        );
    }
    if !app.library.hidden_auto_categories.is_empty() {
        items = items.push(
            button("Manage hidden folders")
                .on_press(Message::OpenDialog(Dialog::Sources))
                .style(button::text),
        );
    }
    items = items.push(Space::new().height(16)).push(row![
        text("SOURCES").size(11),
        Space::new().width(Length::Fill),
        button("+")
            .on_press(Message::OpenDialog(Dialog::AddSource))
            .style(button::text)
    ]);
    for source in &app.library.sources {
        items = items.push(
            text(
                source
                    .path
                    .file_name()
                    .unwrap_or(source.path.as_os_str())
                    .to_string_lossy()
                    .into_owned(),
            )
            .size(12),
        );
    }
    items = items
        .push(
            button("Manage sources")
                .on_press(Message::OpenDialog(Dialog::Sources))
                .style(button::text),
        )
        .push(Space::new().height(16))
        .push(
            button("Clean missing entries")
                .on_press(Message::OpenDialog(Dialog::Cleanup))
                .style(button::text),
        )
        .push(
            button("Playback & rotation settings")
                .on_press(Message::OpenDialog(Dialog::Settings))
                .style(button::text),
        );
    container(scrollable(items).height(Length::Fill))
        .padding(20)
        .width(250)
        .height(Length::Fill)
        .style(crate::theme::panel)
        .into()
}
