//! The app's name and identity, kept in one place while the name settles.
//!
//! Zeus runs beside ZapFast on the same machine: its own folders, keyring
//! entries (keyed by those folders), instance lock and tray item, and no
//! claim on ZapFast's fixed port or its update feed.

/// The name people see.
pub const NAME: &str = "Zeus";
/// The id for folders, the instance lock, the tray, autostart and logs.
pub const ID: &str = "zeus";
/// The app Zeus came from, whose settings it copies once on first start.
pub const PREVIOUS: &str = "zapfast";
/// The Windows application id: notifications, their Start menu shortcut and
/// the taskbar group are filed under it, apart from ZapFast's.
pub const APP_USER_MODEL_ID: &str = "app.zeus.Zeus";
/// Whether to check for, download and offer updates. Zeus has no release
/// feed of its own yet; ZapFast's would replace it with ZapFast.
pub const UPDATES: bool = false;
