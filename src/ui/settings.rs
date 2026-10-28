//! Validated playback/rotation drafts and explicit runtime service controls.
use crate::{
    app::App,
    config::Config,
    message::{Message, RotationServiceAction, SettingsField, SettingsToggle},
};
use anyhow::{Context, Result};
use iced::{
    Element, Length,
    widget::{Column, button, checkbox, column, row, scrollable, text, text_input},
};
use std::{
    path::PathBuf,
    process::{Command, Output},
};
const ROTATION_UNIT: &str = "wallpicker-rotation.service";
#[derive(Debug, Clone)]
pub(crate) struct SettingsDraft {
    pub interval: String,
    pub category: String,
    pub include_live: bool,
    pub notify: bool,
    pub output: String,
    pub fps: String,
    pub muted: bool,
    pub assets: String,
}
impl SettingsDraft {
    pub fn from_config(config: &Config) -> Self {
        Self {
            interval: config.rotation.interval.clone(),
            category: config.rotation.category.clone().unwrap_or_default(),
            include_live: config.rotation.include_live,
            notify: config.rotation.notify,
            output: config.live.output.clone(),
            fps: config.live.fps.to_string(),
            muted: config.live.muted,
            assets: config
                .live
                .assets_dir
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
        }
    }
    pub fn apply_to(&self, current: &Config) -> Result<Config> {
        crate::rotation::parse_interval(self.interval.trim())?;
        let mut config = current.clone();
        config.rotation.interval = self.interval.trim().into();
        config.rotation.category = optional(&self.category);
        config.rotation.include_live = self.include_live;
        config.rotation.notify = self.notify;
        config.live.output = self.output.trim().into();
        config.live.fps = self
            .fps
            .trim()
            .parse()
            .context("FPS must be a whole number")?;
        config.live.muted = self.muted;
        config.live.assets_dir = optional(&self.assets).map(PathBuf::from);
        config.validate()?;
        Ok(config)
    }
    pub fn edit(&mut self, field: SettingsField, value: String) {
        match field {
            SettingsField::Interval => self.interval = value,
            SettingsField::Category => self.category = value,
            SettingsField::Output => self.output = value,
            SettingsField::Fps => self.fps = value,
            SettingsField::Assets => self.assets = value,
        }
    }
    pub fn toggle(&mut self, field: SettingsToggle, value: bool) {
        match field {
            SettingsToggle::IncludeLive => self.include_live = value,
            SettingsToggle::Notify => self.notify = value,
            SettingsToggle::Muted => self.muted = value,
        }
    }
}
fn optional(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.into())
}
pub(crate) fn service_args(action: RotationServiceAction) -> [&'static str; 3] {
    [
        "--user",
        match action {
            RotationServiceAction::Start => "restart",
            RotationServiceAction::Stop => "stop",
            RotationServiceAction::Refresh => "is-active",
        },
        ROTATION_UNIT,
    ]
}
fn session_import_args(
    present: impl Fn(&str) -> bool,
) -> std::result::Result<Vec<&'static str>, String> {
    if !present("WAYLAND_DISPLAY") {
        return Err("Wallpaper rotation needs a Wayland session. Open the picker from your Hyprland desktop before starting the service.".into());
    }
    let mut args = vec!["--user", "import-environment"];
    args.extend(
        [
            "WAYLAND_DISPLAY",
            "DISPLAY",
            "HYPRLAND_INSTANCE_SIGNATURE",
            "PATH",
        ]
        .into_iter()
        .filter(|name| present(name)),
    );
    Ok(args)
}
pub(crate) fn service_result(
    action: RotationServiceAction,
    output: Output,
) -> std::result::Result<String, String> {
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if action == RotationServiceAction::Refresh {
        match stdout.as_str() {
            "active" => return Ok("Running".into()),
            "inactive" => return Ok("Stopped".into()),
            "failed" => return Ok("Failed — inspect the service journal".into()),
            "unknown" => return Ok("Not installed — see README setup".into()),
            _ => {}
        }
    }
    if output.status.success() {
        return Ok(match action {
            RotationServiceAction::Start => "Running",
            RotationServiceAction::Stop => "Stopped",
            RotationServiceAction::Refresh => "Active",
        }
        .into());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = if stderr.trim().is_empty() {
        stdout.as_str()
    } else {
        stderr.trim()
    };
    Err(format!(
        "Could not {} {ROTATION_UNIT}: {}. Install the user service from the README if it is missing.",
        service_args(action)[1],
        if detail.is_empty() {
            output.status.to_string()
        } else {
            detail.into()
        }
    ))
}
pub(crate) fn run_service(action: RotationServiceAction) -> std::result::Result<String, String> {
    if action == RotationServiceAction::Start {
        let args = session_import_args(|name| {
            std::env::var_os(name).is_some_and(|value| !value.is_empty())
        })?;
        let output = Command::new("systemctl")
            .args(args)
            .output()
            .map_err(|error| {
                format!("Cannot import the Wayland session into the user service: {error}")
            })?;
        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "Cannot import the Wayland session into the user service: {}",
                if detail.trim().is_empty() {
                    output.status.to_string()
                } else {
                    detail.trim().into()
                }
            ));
        }
    }
    let output = Command::new("systemctl")
        .args(service_args(action))
        .output()
        .map_err(|e| format!("Cannot run systemctl --user: {e}"))?;
    service_result(action, output)
}
fn input_field<'a>(
    label: &'static str,
    value: &'a str,
    kind: SettingsField,
) -> Column<'a, Message> {
    let id = match kind {
        SettingsField::Interval => "settings-interval",
        SettingsField::Category => "settings-category",
        SettingsField::Output => "settings-output",
        SettingsField::Fps => "settings-fps",
        SettingsField::Assets => "settings-assets",
    };
    column![
        text(label).size(12),
        text_input(label, value)
            .id(id)
            .on_input(move |value| Message::SettingsText(kind, value))
            .on_submit(Message::Confirm)
            .padding(9)
    ]
    .spacing(4)
}
pub(crate) fn view(app: &App) -> Element<'_, Message> {
    let draft = &app.settings;

    let mut body=column![
  text("Automatic wallpaper changes").size(18),
  text("Tab / Shift+Tab move between fields · Enter or Ctrl+S saves · Esc closes").size(12),
  row![input_field("Interval: 30s, 15m, 2h, 1d",&draft.interval,SettingsField::Interval),input_field("Category (blank = last used in picker)",&draft.category,SettingsField::Category)].spacing(12),
  text("Use all, favorites, a manual category, or a folder category name.").size(12),
  checkbox(draft.include_live).label("Include videos and Wallpaper Engine projects in rotation (Ctrl+L)").on_toggle(|value|Message::SettingsToggle(SettingsToggle::IncludeLive,value)),
  checkbox(draft.notify).label("Notify when rotation changes the wallpaper (Ctrl+N)").on_toggle(|value|Message::SettingsToggle(SettingsToggle::Notify,value)),
  text("Live wallpaper playback").size(18),
  row![input_field("Output (ALL or a monitor name)",&draft.output,SettingsField::Output),input_field("Frames per second",&draft.fps,SettingsField::Fps)].spacing(12),
  checkbox(draft.muted).label("Mute live wallpaper audio (Ctrl+M)").on_toggle(|value|Message::SettingsToggle(SettingsToggle::Muted,value)),
  input_field("Wallpaper Engine assets directory (optional absolute path)",&draft.assets,SettingsField::Assets),
  text("Videos and projects play only after you apply them. Rotation excludes live wallpapers unless enabled above.").size(12),
  text("Rotation service").size(18),
  text(format!("Service: {}",app.service_status)).size(13),
  text("Save settings, then start or restart the installed user service. Starting does not enable it at login; installation and autostart are documented in the README.").size(12),
  row![button("Start / restart saved settings").on_press_maybe((!app.service_busy&&!app.config_saving).then_some(Message::RotationService(RotationServiceAction::Start))),button("Stop").on_press_maybe((!app.service_busy).then_some(Message::RotationService(RotationServiceAction::Stop))),button("Refresh status").on_press_maybe((!app.service_busy).then_some(Message::RotationService(RotationServiceAction::Refresh)))].spacing(8),
 ].spacing(12);
    if app.config_saving {
        body = body.push(text("Saving settings…").size(12));
    } else {
        body = body.push(text(&app.status).size(12));
    }
    scrollable(body)
        .height((app.height - 210.0).clamp(180.0, 580.0))
        .width(Length::Fill)
        .into()
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;
    fn output(code: i32, stdout: &str, stderr: &str) -> Output {
        Output {
            status: std::process::ExitStatus::from_raw(code << 8),
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }
    #[test]
    fn session_import_only_includes_available_names_and_requires_wayland() {
        let present = ["WAYLAND_DISPLAY", "PATH"];
        assert_eq!(
            session_import_args(|name| present.contains(&name)).unwrap(),
            ["--user", "import-environment", "WAYLAND_DISPLAY", "PATH"]
        );
        assert!(
            session_import_args(|name| name == "DISPLAY")
                .unwrap_err()
                .contains("Wayland session")
        );
        let args = session_import_args(|_| true).unwrap();
        assert_eq!(
            args,
            [
                "--user",
                "import-environment",
                "WAYLAND_DISPLAY",
                "DISPLAY",
                "HYPRLAND_INSTANCE_SIGNATURE",
                "PATH"
            ]
        );
    }
    #[test]
    fn commands_only_control_the_named_user_service() {
        assert_eq!(
            service_args(RotationServiceAction::Start),
            ["--user", "restart", "wallpicker-rotation.service"]
        );
        assert_eq!(
            service_args(RotationServiceAction::Stop),
            ["--user", "stop", "wallpicker-rotation.service"]
        );
        assert_eq!(
            service_args(RotationServiceAction::Refresh),
            ["--user", "is-active", "wallpicker-rotation.service"]
        );
    }
    #[test]
    fn service_results_surface_failures_and_inactive_state() {
        assert_eq!(
            service_result(RotationServiceAction::Refresh, output(3, "inactive\n", "")).unwrap(),
            "Stopped"
        );
        assert_eq!(
            service_result(RotationServiceAction::Start, output(0, "", "")).unwrap(),
            "Running"
        );
        assert!(
            service_result(
                RotationServiceAction::Start,
                output(5, "", "Unit not found")
            )
            .unwrap_err()
            .contains("Unit not found")
        );
    }
    #[test]
    fn draft_validates_numbers_and_keeps_unrelated_preferences() {
        let config = Config::default();
        let mut draft = SettingsDraft::from_config(&config);
        draft.interval = " 2h ".into();
        draft.category = " favorites ".into();
        draft.fps = "24".into();
        let applied = draft.apply_to(&config).unwrap();
        assert_eq!(applied.rotation.interval, "2h");
        assert_eq!(applied.rotation.category.as_deref(), Some("favorites"));
        assert_eq!(applied.live.fps, 24);
        assert_eq!(applied.theme.background, config.theme.background);
        draft.fps = "many".into();
        assert!(draft.apply_to(&config).is_err());
        draft.fps = "30".into();
        draft.interval = "0s".into();
        assert!(draft.apply_to(&config).is_err());
    }
}
