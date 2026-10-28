//! Low-overhead graphics bootstrap and startup timing diagnostics.
use anyhow::{Context, Result};
use std::{
    ffi::{OsStr, OsString},
    os::unix::process::CommandExt,
    path::Path,
    process::Command,
    sync::OnceLock,
    time::Instant,
};

static STARTED: OnceLock<Instant> = OnceLock::new();
static FIRST_REDRAW: OnceLock<()> = OnceLock::new();

pub fn start() {
    STARTED.get_or_init(Instant::now);
}

pub fn checkpoint(stage: &str) {
    if let Some(started) = STARTED.get() {
        // Write the app's inexpensive timing record directly. Keeping the global
        // log level at WARN avoids wgpu's expensive INFO-only adapter enumeration.
        log::logger().log(
            &log::Record::builder()
                .level(log::Level::Info)
                .target("wallpicker::startup")
                .args(format_args!(
                    "startup: {stage} after {:.1} ms",
                    started.elapsed().as_secs_f64() * 1000.0
                ))
                .build(),
        );
    }
}

pub fn first_redraw_pending() -> bool {
    FIRST_REDRAW.get().is_none()
}

pub fn first_redraw() {
    FIRST_REDRAW.get_or_init(|| checkpoint("first window redraw"));
}

/// Use the software renderer for prompt first-open behavior without cold GPU
/// driver/shader initialization. Explicit renderer or wgpu choices opt back into
/// hardware rendering; that path defaults to Vulkan and a low-power adapter.
///
/// Re-exec uses Command's child environment instead of mutating the process
/// environment with unsafe set_var. It happens before any app threads/files/locks
/// are created, and is skipped on the next entry once its renderer is selected.
pub fn prepare_gui_environment() -> Result<()> {
    let args = std::env::args_os().skip(1);
    let executable = std::env::current_exe().context("Finding wallpicker executable")?;
    let Some(mut command) = graphics_command(
        &executable,
        args,
        std::env::var_os("WGPU_BACKEND").as_deref(),
        std::env::var_os("WGPU_POWER_PREF").as_deref(),
        std::env::var_os("ICED_BACKEND").as_deref(),
    ) else {
        return Ok(());
    };
    Err(command.exec()).context("Starting wallpaper picker with graphics defaults")
}

fn graphics_command(
    executable: &Path,
    args: impl IntoIterator<Item = OsString>,
    backend: Option<&OsStr>,
    power: Option<&OsStr>,
    renderer: Option<&OsStr>,
) -> Option<Command> {
    if renderer == Some(OsStr::new("tiny-skia")) {
        return None;
    }
    let mut command = Command::new(executable);
    command.args(args);
    if renderer.is_none() && backend.is_none() && power.is_none() {
        command.env("ICED_BACKEND", "tiny-skia");
        return Some(command);
    }
    if renderer.is_some() && backend.is_some() && power.is_some() {
        return None;
    }
    if renderer.is_none() {
        command.env("ICED_BACKEND", "wgpu");
    }
    if backend.is_none() {
        command.env("WGPU_BACKEND", "vulkan");
    }
    if power.is_none() {
        command.env("WGPU_POWER_PREF", "low");
    }
    Some(command)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn graphics_defaults_preserve_arguments_and_explicit_user_overrides() {
        let args = vec![OsString::from("--quiet"), OsString::from("pick")];
        let command =
            graphics_command(Path::new("/wallpicker"), args.clone(), None, None, None).unwrap();
        assert_eq!(command.get_args().collect::<Vec<_>>(), args);
        assert_eq!(
            command.get_envs().collect::<Vec<_>>(),
            vec![(OsStr::new("ICED_BACKEND"), Some(OsStr::new("tiny-skia")))]
        );
        let command = graphics_command(
            Path::new("/wallpicker"),
            [],
            Some(OsStr::new("gl")),
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            command.get_envs().collect::<Vec<_>>(),
            vec![
                (OsStr::new("ICED_BACKEND"), Some(OsStr::new("wgpu"))),
                (OsStr::new("WGPU_POWER_PREF"), Some(OsStr::new("low")))
            ]
        );
        assert!(
            graphics_command(
                Path::new("/wallpicker"),
                [],
                Some(OsStr::new("gl")),
                Some(OsStr::new("high")),
                Some(OsStr::new("wgpu"))
            )
            .is_none()
        );
        assert!(
            graphics_command(
                Path::new("/wallpicker"),
                [],
                None,
                None,
                Some(OsStr::new("tiny-skia"))
            )
            .is_none()
        );
        let command = graphics_command(
            Path::new("/wallpicker"),
            [],
            None,
            None,
            Some(OsStr::new("wgpu")),
        )
        .unwrap();
        assert_eq!(
            command.get_envs().collect::<Vec<_>>(),
            vec![
                (OsStr::new("WGPU_BACKEND"), Some(OsStr::new("vulkan"))),
                (OsStr::new("WGPU_POWER_PREF"), Some(OsStr::new("low")))
            ]
        );
    }
    #[test]
    fn graphics_child_receives_defaults_without_changing_parent_environment() {
        let before_backend = std::env::var_os("WGPU_BACKEND");
        let before_power = std::env::var_os("WGPU_POWER_PREF");
        let before_renderer = std::env::var_os("ICED_BACKEND");
        let mut command = graphics_command(
            Path::new("/bin/sh"),
            [
                OsString::from("-c"),
                OsString::from(
                    "printf '%s/%s/%s' \"$ICED_BACKEND\" \"$WGPU_BACKEND\" \"$WGPU_POWER_PREF\"",
                ),
            ],
            None,
            None,
            None,
        )
        .unwrap();
        command
            .env_remove("WGPU_BACKEND")
            .env_remove("WGPU_POWER_PREF");
        let output = command.output().unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"tiny-skia//");
        assert_eq!(std::env::var_os("WGPU_BACKEND"), before_backend);
        assert_eq!(std::env::var_os("WGPU_POWER_PREF"), before_power);
        assert_eq!(std::env::var_os("ICED_BACKEND"), before_renderer);
    }
}

#[cfg(test)]
mod color_policy_tests {
    #[test]
    fn native_renderer_performs_srgb_gamma_correction() {
        let mid = iced::advanced::graphics::color::pack(iced::Color::from_rgb8(128, 128, 128))
            .components();
        // sRGB 128 is about 0.216 in a linear shader. Passing 0.502 straight
        // through a float surface is what brightens midtones on screen.
        for channel in &mid[..3] {
            assert!(
                (0.215..0.217).contains(channel),
                "sRGB midtone was not linearized: {mid:?}"
            );
        }
        assert_eq!(mid[3], 1.0);
    }
}
