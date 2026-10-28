//! UI actions and asynchronous completion messages.
use crate::{
    app::{Dialog, Sort},
    model::{CategoryRef, ScanResult},
};
use std::path::PathBuf;
/// The caller's requested behavior after a successful wallpaper application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyIntent {
    ClosePicker,
    KeepBrowsing,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsField {
    Interval,
    Category,
    Output,
    Fps,
    Assets,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsToggle {
    IncludeLive,
    Notify,
    Muted,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RotationServiceAction {
    Start,
    Stop,
    Refresh,
}
#[derive(Debug, Clone)]
pub enum Message {
    ActivateCommand,
    FirstRedraw,
    Tick,
    Event(iced::Event),
    Key(iced::keyboard::Key, iced::keyboard::Modifiers),
    Search(String),
    SearchSubmitted,
    Sort(Sort),
    Category(CategoryRef),
    Rescan,
    Scanned(ScanResult),
    Click(PathBuf),
    Apply(PathBuf),
    ApplyPreview(PathBuf),
    ApplySelected,
    Applied {
        path: PathBuf,
        intent: ApplyIntent,
        result: Result<(), String>,
    },
    OperationFailed(String),
    Favorite(PathBuf),
    FavoriteSelected,
    SelectAll,
    ClearSelection,
    Random,
    Hover(Option<PathBuf>),
    Context(PathBuf),
    Copy(PathBuf),
    Reveal(PathBuf),
    OpenDialog(Dialog),
    DialogText(String),
    DialogRecursive(bool),
    DialogAuto(bool),
    Confirm,
    Cancel,
    Browse,
    Browsed(Result<Option<String>, String>),
    AddTo(u64),
    AddImage(PathBuf),
    RemoveSelected,
    HideFolder(String),
    UnhideFolder(String),
    ToggleSourceRecursive(usize, bool),
    ToggleSourceAuto(usize, bool),
    Saved(Result<(), String>),
    SettingsText(SettingsField, String),
    SettingsToggle(SettingsToggle, bool),
    ConfigSaved(Result<(), String>),
    RotationService(RotationServiceAction),
    RotationServiceFinished {
        action: RotationServiceAction,
        result: Result<String, String>,
    },
    DismissError,
    Scroll(f32),
    CleanCache,
    DoCleanCache,
    CacheCleaned(Result<usize, String>),
    Close,
    Noop,
}
