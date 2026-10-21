//! Self-update from GitHub releases, through fastframe-update.
//!
//! The crate checks for a newer release, refuses package-managed copies,
//! downloads and verifies the update against the publisher signature, and
//! hands it to a helper that installs it and rolls back if it does not start.
//! Zeus keeps its names, its key, its proxy and its interface.

pub use fastframe_update::{
    CHECK_INTERVAL, DownloadState, Installation, Kind, Prepared, Release, Source, Unsupported,
    Updater,
};
use fastframe_update::{ReqwestTransport, UpdateConfig};

/// Zeus's releases, signed with Zeus's own key, so a Zeus copy never takes a
/// ZapFast release or the other way round. The repository is its name as
/// GitHub has it now: download links must match it exactly, so a rename
/// leaves installed copies unable to update until they get it.
pub const CONFIG: UpdateConfig = UpdateConfig {
    publisher_key: Some(include_str!("../assets/zeus-update-public-key.hex")),
    ..UpdateConfig::new(
        "ItIsNotKeshav/Zeus",
        crate::brand::NAME,
        crate::brand::ID,
        env!("CARGO_PKG_VERSION"),
    )
};

/// Where a check the reader asked for stands, for Settings to say. Daily
/// checks run without it and stay quiet when they fail.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Check {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Failed(String),
}

/// An updater on the proxy-aware reqwest client.
pub fn updater() -> anyhow::Result<Updater> {
    let mut builder = reqwest::blocking::Client::builder();
    if let Some(proxy) = crate::proxy::reqwest_proxy() {
        builder = builder.proxy(proxy);
    }
    Ok(Updater::new(CONFIG, ReqwestTransport::new(builder)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_config_is_valid() {
        CONFIG.validate().unwrap();
        assert_eq!(CONFIG.current_version, env!("CARGO_PKG_VERSION"));
        assert_eq!(CONFIG.slug, "zeus");
    }

    #[test]
    fn the_updater_starts_on_github() {
        assert!(updater().unwrap().source().is_github());
    }
}
