//! Public command line contract.
use clap::{Parser, Subcommand};
use std::path::PathBuf;
#[derive(Debug, Parser)]
#[command(version, about = "A keyboard-friendly wallpaper picker for Hyprland")]
pub struct Cli {
    #[arg(long, global = true, help = "Suppress desktop notifications")]
    pub quiet: bool,
    #[command(subcommand)]
    pub command: Option<Command>,
}
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Open the wallpaper picker
    Pick,
    /// Apply a random image from the last selected or specified category
    Random {
        #[arg(long)]
        category: Option<String>,
        #[arg(long)]
        no_rescan: bool,
    },
    /// Apply one wallpaper
    Set { path: PathBuf },
    /// Quietly restore the last applied wallpaper
    Restore,
    /// Scan sources and print a summary
    Rescan,
    /// Randomly change wallpapers at a configurable interval (no GUI)
    Rotate {
        #[arg(long, value_parser=validate_interval)]
        interval: Option<String>,
        #[arg(long)]
        category: Option<String>,
        #[arg(long, help = "Include videos and Wallpaper Engine projects")]
        include_live: bool,
        #[arg(long, help = "Change once immediately and exit")]
        once: bool,
    },
    /// Control wallpicker's tracked live wallpaper
    Live {
        #[command(subcommand)]
        command: LiveCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum LiveCommand {
    /// Stop this app's tracked live player; keep the saved selection for restore
    Stop,
}
fn validate_interval(value: &str) -> Result<String, String> {
    crate::rotation::parse_interval(value)
        .map(|_| value.to_owned())
        .map_err(|error| error.to_string())
}
