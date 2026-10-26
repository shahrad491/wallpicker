//! Configurable dark palette and consistent rounded surfaces.
use crate::config::Palette;
use iced::{Color, Theme, widget::container};
pub fn color(value: &str) -> Color {
    let rgb = value
        .strip_prefix('#')
        .and_then(|v| u32::from_str_radix(v, 16).ok())
        .unwrap_or(0x152026);
    Color::from_rgb8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}
pub fn theme(p: &Palette) -> Theme {
    Theme::custom_with_fn(
        "Wallpicker",
        iced::theme::Palette {
            background: color(&p.background),
            text: color(&p.foreground),
            primary: color(&p.accent),
            success: color(&p.accent),
            danger: Color::from_rgb8(235, 125, 139),
            warning: Color::from_rgb8(240, 200, 120),
        },
        |palette| {
            let mut extended = iced::theme::palette::Extended::generate(palette);
            extended.background.weak =
                iced::theme::palette::Pair::new(color(&p.surface), color(&p.foreground));
            extended
        },
    )
}
pub fn panel(theme: &Theme) -> container::Style {
    let p = theme.extended_palette();
    container::Style {
        background: Some(p.background.weak.color.into()),
        border: iced::Border {
            radius: 12.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}
pub fn tile(theme: &Theme, selected: bool) -> container::Style {
    let p = theme.extended_palette();
    container::Style {
        background: Some(p.background.weak.color.into()),
        border: iced::Border {
            radius: 10.into(),
            width: if selected { 2.0 } else { 1.0 },
            color: if selected {
                p.primary.base.color
            } else {
                p.background.strong.color
            },
        },
        ..Default::default()
    }
}
