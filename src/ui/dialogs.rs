//! Explicit metadata-only confirmations and category/source editors.
use crate::{
    app::{App, Dialog},
    message::Message,
    model::CategoryRef,
};
use iced::{
    Element, Length,
    widget::{Space, button, checkbox, column, container, row, scrollable, text, text_input},
};
pub(crate) fn view(app: &App) -> Element<'_, Message> {
    let Some(dialog) = &app.dialog else {
        return Space::new().into();
    };
    let mut body = column![].spacing(14);
    let mut confirm = Some("Save");
    match dialog {
        Dialog::Commands => {
            confirm = None;
            body = body
                .push(text("Commands").size(23))
                .push(
                    text_input("Type an action, category, or source…", &app.dialog_text)
                        .id("dialog-input")
                        .on_input(Message::DialogText)
                        .on_submit(Message::ActivateCommand)
                        .padding(12),
                )
                .push(text("↑ / ↓ choose · Enter run · Esc cancel").size(12));
            let mut list = column![].spacing(5);
            let commands = app.commands();
            for (i, (name, action)) in commands
                .into_iter()
                .enumerate()
                .skip(app.command_index.saturating_sub(4))
                .take(9)
            {
                list = list.push(
                    button(text(name).size(13))
                        .width(Length::Fill)
                        .style(if i == app.command_index {
                            button::primary
                        } else {
                            button::text
                        })
                        .on_press(action),
                );
            }
            body = body.push(list);
        }

        Dialog::NewCategory | Dialog::Rename(_) => {
            body = body
                .push(
                    text(if matches!(dialog, Dialog::NewCategory) {
                        "New category"
                    } else {
                        "Rename category"
                    })
                    .size(23),
                )
                .push(
                    text_input("Category name", &app.dialog_text)
                        .id("dialog-input")
                        .on_input(Message::DialogText)
                        .on_submit(Message::Confirm)
                        .padding(12),
                );
            if let Dialog::Rename(id) = dialog {
                body = body.push(
                    button("Delete category…")
                        .style(button::danger)
                        .on_press(Message::OpenDialog(Dialog::DeleteCategory(*id))),
                );
            }
        }
        Dialog::DeleteCategory(id) => {
            let c = app.library.categories.iter().find(|c| c.id == *id);
            body=body.push(text("Delete this category?").size(23)).push(text(format!("Remove “{}” and its {} memberships? Image files and other categories are untouched.",c.map(|c|c.name.as_str()).unwrap_or("Category"),c.map(|c|c.images.len()).unwrap_or(0))));
            confirm = Some("Delete category");
        }
        Dialog::AddSource => {
            body = body
                .push(text("Add wallpaper source").size(23))
                .push(text(
                    "Choose a directory. Images are read in place; nothing is moved.",
                ))
                .push(
                    row![
                        text_input("/home/you/Pictures/wallpaper", &app.dialog_text)
                            .id("dialog-input")
                            .on_input(Message::DialogText)
                            .on_submit(Message::Confirm)
                            .padding(12),
                        button("Browse…  Ctrl+B").on_press(Message::Browse)
                    ]
                    .spacing(10),
                )
                .push(
                    checkbox(app.dialog_recursive)
                        .label("Scan subfolders recursively (Ctrl+R)")
                        .on_toggle(Message::DialogRecursive),
                )
                .push(
                    checkbox(app.dialog_auto)
                        .label("Create read-only folder categories (Ctrl+F)")
                        .on_toggle(Message::DialogAuto),
                );
            confirm = Some("Add source");
        }
        Dialog::RemoveSource(path) => {
            body=body.push(text("Remove this source?").size(23)).push(text(path.to_string_lossy())).push(text("Its wallpapers disappear from All and folder categories, and their Favorites and manual memberships are pruned. Image files on disk are untouched."));
            confirm = Some("Remove source");
        }
        Dialog::Sources => {
            confirm = None;
            body = body.push(text("Sources & folders").size(23));
            let mut list = column![].spacing(16);
            for (i, source) in app.library.sources.iter().enumerate() {
                list = list.push(
                    container(
                        column![
                            text(source.path.to_string_lossy()).size(14),
                            row![
                                checkbox(source.recursive)
                                    .label("Recursive")
                                    .on_toggle(move |v| Message::ToggleSourceRecursive(i, v)),
                                checkbox(source.auto_categories)
                                    .label("Folder categories")
                                    .on_toggle(move |v| Message::ToggleSourceAuto(i, v))
                            ]
                            .spacing(15),
                            button("Remove source…").style(button::danger).on_press(
                                Message::OpenDialog(Dialog::RemoveSource(source.path.clone()))
                            )
                        ]
                        .spacing(10),
                    )
                    .padding(12)
                    .style(crate::theme::panel),
                );
            }
            if app.library.sources.is_empty() {
                list = list.push(text(
                    "No sources yet. Add a wallpaper folder to get started.",
                ));
            }
            list=list.push(button("+ Add source").on_press(Message::OpenDialog(Dialog::AddSource))).push(text("Hidden folder categories").size(17)).push(text("Folder categories regenerate from immediate subfolders; hidden names stay hidden across rescans.").size(12));
            for name in &app.library.hidden_auto_categories {
                list = list.push(row![
                    text(name).width(Length::Fill),
                    button("Unhide").on_press(Message::UnhideFolder(name.clone()))
                ]);
            }
            list = list.push(button("Clean orphaned thumbnails").on_press(Message::CleanCache));
            body = body.push(scrollable(list).height(340));
        }
        Dialog::AddImages => {
            confirm = None;
            body = body
                .push(text(format!("Add {} selected images", app.selected.len())).size(23))
                .push(text("Choose a category. Existing memberships are kept."));
            for c in &app.library.categories {
                body = body.push(
                    button(text(&c.name))
                        .width(Length::Fill)
                        .on_press(Message::AddTo(c.id)),
                );
            }
            if app.library.categories.is_empty() {
                body = body.push(text("Create a manual category first.")).push(
                    button("New category").on_press(Message::OpenDialog(Dialog::NewCategory)),
                );
            }
        }
        Dialog::RemoveImages => {
            body = body
                .push(text("Remove selected memberships?").size(23))
                .push(text(format!(
                    "Remove {} images from {}? Image files and other categories are untouched.",
                    app.selected.len(),
                    app.title()
                )));
            confirm = Some("Remove memberships");
        }
        Dialog::CleanupCache => {
            body=body.push(text("Clean orphaned thumbnails?").size(23)).push(text("Delete cached previews that no longer belong to the scanned library. Wallpaper files remain untouched."));
            confirm = Some("Clean cache");
        }
        Dialog::Cleanup => {
            body=body.push(text("Clean missing entries?").size(23)).push(text("Remove Favorites and manual category memberships whose image files no longer exist. Existing files stay untouched."));
            confirm = Some("Clean missing entries");
        }
        Dialog::Hide(name) => {
            body=body.push(text("Hide folder category?").size(23)).push(text(format!("Hide “{name}” from the sidebar? Its images remain in All. Unhide it in Manage sources whenever you like.")));
            confirm = Some("Hide folder");
        }
        Dialog::Context(path) => {
            confirm = None;
            body = body
                .push(
                    text(
                        path.file_name()
                            .unwrap_or(path.as_os_str())
                            .to_string_lossy(),
                    )
                    .size(18),
                )
                .push(
                    button("Apply wallpaper")
                        .width(Length::Fill)
                        .on_press(Message::Apply(path.clone())),
                )
                .push(
                    button("Toggle favorite")
                        .width(Length::Fill)
                        .on_press(Message::Favorite(path.clone())),
                )
                .push(
                    button("Add selection to category…")
                        .width(Length::Fill)
                        .on_press(Message::OpenDialog(Dialog::AddImages)),
                );
            if matches!(
                app.category,
                CategoryRef::Manual { .. } | CategoryRef::Favorites
            ) {
                body = body.push(
                    button("Remove selection from this category…")
                        .on_press(Message::RemoveSelected),
                );
            }
            body = body
                .push(
                    button("Reveal in file manager")
                        .width(Length::Fill)
                        .on_press(Message::Reveal(path.clone())),
                )
                .push(
                    button("Copy path")
                        .width(Length::Fill)
                        .on_press(Message::Copy(path.clone())),
                );
        }
    }
    if let Some(error) = &app.error {
        body = body.push(
            text(error)
                .color(iced::Color::from_rgb8(235, 125, 139))
                .size(13),
        );
    }
    let mut footer = row![
        Space::new().width(Length::Fill),
        button(if confirm.is_some() { "Cancel" } else { "Done" }).on_press(Message::Cancel)
    ]
    .spacing(10);
    if let Some(label) = confirm {
        footer = footer.push(button(label).on_press(Message::Confirm));
    }
    body = body.push(footer);
    let context = matches!(dialog, Dialog::Context(_));
    let content: Element<'_, Message> = if context {
        scrollable(body).height(Length::Shrink).into()
    } else {
        body.into()
    };
    let panel = container(content)
        .max_height((app.height - 16.0).max(100.0))
        .padding(24)
        .width(if matches!(dialog, Dialog::Context(_)) {
            320
        } else {
            570
        })
        .style(crate::theme::panel);
    panel.into()
}
