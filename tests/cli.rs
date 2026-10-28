//! The CLI contract is checked without launching the GUI or wallpaper tools.
use clap::Parser;
use wallpicker::cli::{Cli, Command};

#[test]
fn default_and_explicit_picker_parse() {
    assert!(
        Cli::try_parse_from(["wallpicker"])
            .unwrap()
            .command
            .is_none()
    );
    assert!(matches!(
        Cli::try_parse_from(["wallpicker", "pick"]).unwrap().command,
        Some(Command::Pick)
    ));
}
#[test]
fn random_options_and_global_quiet_parse() {
    let cli = Cli::try_parse_from([
        "wallpicker",
        "random",
        "--category",
        "Dark skies",
        "--no-rescan",
        "--quiet",
    ])
    .unwrap();
    assert!(cli.quiet);
    assert!(
        matches!(cli.command, Some(Command::Random {category: Some(name), no_rescan:true}) if name == "Dark skies")
    );
    assert!(matches!(
        Cli::try_parse_from(["wallpicker", "random"])
            .unwrap()
            .command,
        Some(Command::Random {
            category: None,
            no_rescan: false
        })
    ));
}
#[test]
fn headless_commands_parse_and_invalid_arguments_fail() {
    assert!(
        matches!(Cli::try_parse_from(["wallpicker", "set", "/tmp/image with spaces.png"]).unwrap().command, Some(Command::Set {path}) if path.to_string_lossy()=="/tmp/image with spaces.png")
    );
    assert!(matches!(
        Cli::try_parse_from(["wallpicker", "restore"])
            .unwrap()
            .command,
        Some(Command::Restore)
    ));
    assert!(matches!(
        Cli::try_parse_from(["wallpicker", "rescan"])
            .unwrap()
            .command,
        Some(Command::Rescan)
    ));
    for args in [
        vec!["wallpicker", "set"],
        vec!["wallpicker", "random", "--category"],
        vec!["wallpicker", "delete"],
        vec!["wallpicker", "pick", "--no-rescan"],
    ] {
        assert!(Cli::try_parse_from(args).is_err());
    }
}
#[test]
fn help_and_version_exit_successfully_before_any_side_effects() {
    for flag in ["--help", "--version"] {
        let error = Cli::try_parse_from(["wallpicker", flag]).unwrap_err();
        assert_eq!(error.exit_code(), 0);
    }
}

#[test]
fn rotation_and_live_commands_validate_public_options() {
    use wallpicker::cli::LiveCommand;
    assert!(
        matches!(Cli::try_parse_from(["wallpicker","rotate","--interval","30m","--category","favorites","--include-live","--once"]).unwrap().command,Some(Command::Rotate{interval:Some(interval),include_live:true,once:true,..}) if interval=="30m")
    );
    assert!(matches!(
        Cli::try_parse_from(["wallpicker", "live", "stop"])
            .unwrap()
            .command,
        Some(Command::Live {
            command: LiveCommand::Stop
        })
    ));
    for interval in ["0s", "bad", "366d"] {
        assert!(Cli::try_parse_from(["wallpicker", "rotate", "--interval", interval]).is_err());
    }
    assert!(Cli::try_parse_from(["wallpicker", "live", "start"]).is_err());
}
