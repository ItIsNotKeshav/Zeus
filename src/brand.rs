//! The app's name and identity, kept in one place while the name settles.
//!
//! Zeus runs beside ZapFast on the same machine: its own folders, keyring
//! entries (keyed by those folders), instance lock and tray item, its own
//! update feed (`updates::CONFIG`), and no claim on ZapFast's fixed port.

/// The name people see.
pub const NAME: &str = "Zeus";
/// The id for folders, the instance lock, the tray, autostart and logs.
pub const ID: &str = "zeus";
/// The app Zeus came from, whose settings it copies once on first start.
pub const PREVIOUS: &str = "zapfast";
/// The Windows application id: notifications, their Start menu shortcut and
/// the taskbar group are filed under it, apart from ZapFast's.
pub const APP_USER_MODEL_ID: &str = "app.zeus.Zeus";
/// Whether to check for, download and offer updates, from Zeus's own
/// releases (`packaging/windows/release-zeus.cmd` publishes them).
pub const UPDATES: bool = true;
