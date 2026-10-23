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
use fastframe_update::{MacConfig, Prereleases, ReqwestTransport, UpdateConfig};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

/// Zeus's releases, signed with Zeus's own key, so a Zeus copy never takes a
/// ZapFast release or the other way round. The repository is its name as
/// GitHub has it now: download links must match it exactly, so a rename
/// leaves installed copies unable to update until they get it.
pub const CONFIG: UpdateConfig = UpdateConfig {
    // The bundle packaging/macos/bundle-zeus.sh makes.
    macos: MacConfig {
        bundle_ids: &["app.zeus.Zeus"],
        executable_names: &[],
        legacy_bundle_names: &[],
    },
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

/// Which releases the reader follows: the latest stable one, or the latest
/// of any kind, pre-releases included.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    #[default]
    Stable,
    Beta,
}

impl Channel {
    pub const ALL: [Channel; 2] = [Channel::Stable, Channel::Beta];

    pub fn label(self) -> &'static str {
        match self {
            Channel::Stable => "Stable",
            Channel::Beta => "Beta",
        }
    }
}

static BETA: AtomicBool = AtomicBool::new(false);

/// Picks the channel every later check and download follows. Settings sets
/// it at start and when the reader changes it.
pub fn set_channel(channel: Channel) {
    BETA.store(channel == Channel::Beta, Ordering::Relaxed);
}

pub fn channel() -> Channel {
    if BETA.load(Ordering::Relaxed) {
        Channel::Beta
    } else {
        Channel::Stable
    }
}

/// The configuration of the beta channel. The library offers pre-releases
/// only to a build that is itself one, so a stable build asks as the
/// pre-release just below its own version (`0.25.0-0`): any later release,
/// stable or not, is newer, and [`check`] drops the build's own version.
/// Only the update calls see this; the installed app still reports `CONFIG`.
fn beta_config() -> &'static UpdateConfig {
    static BETA_CONFIG: OnceLock<UpdateConfig> = OnceLock::new();
    BETA_CONFIG.get_or_init(|| {
        let running = CONFIG.current_version;
        let current = if running.contains('-') {
            running
        } else {
            Box::leak(format!("{running}-0").into_boxed_str())
        };
        UpdateConfig {
            prereleases: Prereleases::WhenRunningPrerelease,
            current_version: current,
            ..CONFIG
        }
    })
}

/// Asks for the newest release on the chosen channel.
pub fn check(updater: &Updater) -> anyhow::Result<Option<Release>> {
    Ok(updater
        .check()?
        .filter(|release| release.version != CONFIG.current_version))
}

/// An updater on the proxy-aware reqwest client.
pub fn updater() -> anyhow::Result<Updater> {
    let mut builder = reqwest::blocking::Client::builder();
    if let Some(proxy) = crate::proxy::reqwest_proxy() {
        builder = builder.proxy(proxy);
    }
    let config = match channel() {
        Channel::Stable => CONFIG,
        Channel::Beta => *beta_config(),
    };
    Ok(Updater::new(config, ReqwestTransport::new(builder)?))
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
    fn the_beta_channel_sees_prereleases_from_a_stable_build() {
        beta_config().validate().unwrap();
        assert!(
            beta_config()
                .current_version
                .starts_with(CONFIG.current_version)
        );
        assert!(beta_config().current_version.contains('-'));
        assert_eq!(beta_config().publisher_key, CONFIG.publisher_key);
    }

    #[test]
    fn the_channel_is_stable_until_chosen() {
        assert_eq!(Channel::default(), Channel::Stable);
        assert_eq!(serde_json::to_string(&Channel::Beta).unwrap(), "\"beta\"");
    }

    #[test]
    fn the_updater_starts_on_github() {
        assert!(updater().unwrap().source().is_github());
    }
}
