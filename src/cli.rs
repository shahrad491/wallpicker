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
}
