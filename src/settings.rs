//! User preferences stored in JSON.

use std::collections::HashSet;
use std::path::Path;

use crate::chat_themes::{Accent, ChatTheme, Mode, Style};
use serde::{Deserialize, Serialize};

/// Verifying the locked-chat code costs about 20 ms, paid once per distinct
/// typed string. ponytail: fixed cost, revisit if it lags the search field.
const CHAT_LOCK_ROUNDS: std::num::NonZeroU32 = std::num::NonZeroU32::new(200_000).unwrap();

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) fn unhex(value: &str) -> Option<Vec<u8>> {
    value
        .len()
        .is_multiple_of(2)
        .then(|| {
            (0..value.len())
                .step_by(2)
                .map(|at| u8::from_str_radix(&value[at..at + 2], 16).ok())
                .collect()
        })
        .flatten()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeChoice {
    #[default]
    Dark,
    Light,
    System,
    /// macOS's greys, light or dark as the system is, with bubbles shaped as
    /// Messages draws them.
    Messages,
    /// Nothing OS inspired monochrome themes with translucent surfaces,
    /// offered as plain Dark and Light; `Dark` and `Light` above are
    /// WhatsApp's own colours. The names stay for saved settings.
    NothingDark,
    NothingLight,
    /// Dark with a coloured accent and own bubbles.
    Red,
    Blue,
    Green,
    Pink,
    Purple,
    Yellow,
}

impl ThemeChoice {
    /// The choices Settings offers, on every platform. Messages shows the
    /// desktop through its window where the platform can, and is opaque in
    /// its own colours elsewhere (see [`crate::backdrop`]).
    pub const ALL: &[ThemeChoice] = &[
        Self::System,
        Self::NothingLight,
        Self::NothingDark,
        Self::Red,
        Self::Blue,
        Self::Green,
        Self::Pink,
        Self::Purple,
        Self::Yellow,
        Self::Light,
        Self::Dark,
        Self::Messages,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Dark => "WhatsApp Dark",
            Self::Light => "WhatsApp Light",
            Self::System => "Follow system",
            Self::Messages => "Messages",
            Self::NothingDark => "Dark",
            Self::NothingLight => "Light",
            Self::Red => "Red",
            Self::Blue => "Blue",
            Self::Green => "Green",
            Self::Pink => "Pink",
            Self::Purple => "Purple",
            Self::Yellow => "Yellow",
        }
    }

    /// The accent of a colour theme, as RGB.
    pub fn accent(self) -> Option<[u8; 3]> {
        match self {
            Self::Red => Some([0xff, 0x45, 0x3a]),
            Self::Blue => Some([0x0a, 0x84, 0xff]),
            Self::Green => Some([0x30, 0xd1, 0x58]),
            Self::Pink => Some([0xff, 0x37, 0x8f]),
            Self::Purple => Some([0xbf, 0x5a, 0xf2]),
            Self::Yellow => Some([0xff, 0xd6, 0x0a]),
            _ => None,
        }
    }

    /// Whether the window shows the desktop through it where it can.
    pub fn vibrant(self) -> bool {
        matches!(
            self,
            Self::Messages | Self::NothingDark | Self::NothingLight
        ) || self.accent().is_some()
    }
}

/// The typeface the interface is drawn with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FontChoice {
    /// The platform's own interface font.
    System,
    /// The bundled Inter, the same on every machine.
    Inter,
    /// Geist, the bundled typeface from Vercel.
    Geist,
    /// Bundled Google Sans Flex, used by the Nothing inspired appearance.
    #[default]
    GoogleSansFlex,
    /// Figtree, a friendly geometric sans.
    Figtree,
    /// Plus Jakarta Sans, a soft modern sans.
    PlusJakartaSans,
    /// Manrope, a clean semi-condensed sans.
    Manrope,
    /// DM Sans, a low contrast geometric sans.
    DmSans,
}

impl FontChoice {
    pub const ALL: [FontChoice; 8] = [
        Self::System,
        Self::Inter,
        Self::Geist,
        Self::GoogleSansFlex,
        Self::Figtree,
        Self::PlusJakartaSans,
        Self::Manrope,
        Self::DmSans,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Inter => "Inter",
            Self::Geist => "Geist",
            Self::GoogleSansFlex => "Google Sans Flex",
            Self::Figtree => "Figtree",
            Self::PlusJakartaSans => "Plus Jakarta Sans",
            Self::Manrope => "Manrope",
            Self::DmSans => "DM Sans",
        }
    }
}

/// What the Halloween look is made of, each part switchable on its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HalloweenParts {
    /// The orange colours.
    pub colors: bool,
    /// The night sky and skyline behind the chat.
    pub wallpaper: bool,
    /// The Halloween doodles and their orange ink.
    pub doodles: bool,
    /// The pumpkin, ghost and other Halloween icons.
    pub icons: bool,
    /// The spooky notification sounds.
    pub sounds: bool,
}

impl Default for HalloweenParts {
    fn default() -> Self {
        Self {
            colors: true,
            wallpaper: true,
            doodles: true,
            icons: true,
            sounds: true,
        }
    }
}

/// One part of the Halloween look.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HalloweenPart {
    Colors,
    Wallpaper,
    Doodles,
    Icons,
    Sounds,
}

impl HalloweenPart {
    pub const ALL: [Self; 5] = [
        Self::Colors,
        Self::Wallpaper,
        Self::Doodles,
        Self::Icons,
        Self::Sounds,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Colors => "Orange colours",
            Self::Wallpaper => "Night wallpaper",
            Self::Doodles => "Halloween doodles",
            Self::Icons => "Halloween icons",
            Self::Sounds => "Spooky sounds",
        }
    }
}

/// The doodle pattern drawn over the wallpaper colour. Each is a tile of
/// Lucide icons (ISC), see `assets/doodles` and `scripts/generate-doodles.py`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DoodleStyle {
    /// The everyday icons the wallpaper has always had.
    #[default]
    Classic,
    Nature,
    Space,
    Food,
    Travel,
    Play,
    /// Pumpkins, bats, ghosts and webs, drawn for Zeus.
    Halloween,
}

impl DoodleStyle {
    pub const ALL: [Self; 7] = [
        Self::Halloween,
        Self::Classic,
        Self::Nature,
        Self::Space,
        Self::Food,
        Self::Travel,
        Self::Play,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Halloween => "Halloween",
            Self::Classic => "Classic",
            Self::Nature => "Nature",
            Self::Space => "Space",
            Self::Food => "Food",
            Self::Travel => "Travel",
            Self::Play => "Play",
        }
    }
}

/// The colour the doodles are drawn in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DoodleColor {
    /// Near-black or white, whichever stands out from the wallpaper colour.
    #[default]
    Gray,
    /// The accent colour.
    Accent,
}

impl DoodleColor {
    pub const ALL: [Self; 2] = [Self::Gray, Self::Accent];

    pub fn label(self) -> &'static str {
        match self {
            Self::Gray => "Gray",
            Self::Accent => "Accent",
        }
    }
}

/// How much of the default doodle strength the slider scales it by.
pub const DOODLE_INTENSITY: std::ops::RangeInclusive<f32> = 0.2..=2.5;

/// Background colours offered by WhatsApp's wallpaper picker, after the
/// active theme's own chat colour.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WallpaperColor {
    /// The active palette's `chat` colour, so a custom theme sets the wallpaper.
    #[default]
    Theme,
    Beige,
    Cruise,
    Scandal,
    MonteCarlo,
    HawkesBlue,
    Downy,
    Seagull,
    Quartz,
    VeryLightGrey,
    Orinoco,
    Tusk,
    CapeHoney,
    Caramel,
    RoseBud,
    Bittersweet,
    RadicalRed,
    MandarinOrange,
    Flamingo,
    Buccaneer,
    BreakerBay,
    Pelorous,
    ToryBlue,
    Fiord,
    Cinder,
    Tolopea,
    Solitude,
    Canary,
    WillowBrook,
    Black,
    Nordic,
    CardinGreen,
    Tangaroa,
    Tiber,
    BlackRussian,
    Nero,
    Marshland,
    Maire,
    BlackMagic,
    CocoaBrown,
    WoodBark,
    SealBrown,
    SealBrownDarker,
    SealBrownLight,
    Cyprus,
    BlueWhale,
    BlackPearl,
    DarkTolopea,
    Woodsmoke,
    MaireTwo,
}

impl WallpaperColor {
    pub const LIGHT: [Self; 29] = [
        Self::Theme,
        Self::Beige,
        Self::Cruise,
        Self::Scandal,
        Self::MonteCarlo,
        Self::HawkesBlue,
        Self::Downy,
        Self::Seagull,
        Self::Quartz,
        Self::VeryLightGrey,
        Self::Orinoco,
        Self::Tusk,
        Self::CapeHoney,
        Self::Caramel,
        Self::RoseBud,
        Self::Bittersweet,
        Self::RadicalRed,
        Self::MandarinOrange,
        Self::Flamingo,
        Self::Buccaneer,
        Self::BreakerBay,
        Self::Pelorous,
        Self::ToryBlue,
        Self::Fiord,
        Self::Cinder,
        Self::Tolopea,
        Self::Solitude,
        Self::Canary,
        Self::WillowBrook,
    ];

    pub const DARK: [Self; 22] = [
        Self::Theme,
        Self::Black,
        Self::Nordic,
        Self::CardinGreen,
        Self::Tangaroa,
        Self::Tiber,
        Self::BlackRussian,
        Self::Nero,
        Self::Marshland,
        Self::Maire,
        Self::BlackMagic,
        Self::CocoaBrown,
        Self::WoodBark,
        Self::SealBrown,
        Self::SealBrownDarker,
        Self::SealBrownLight,
        Self::Cyprus,
        Self::BlueWhale,
        Self::BlackPearl,
        Self::DarkTolopea,
        Self::Woodsmoke,
        Self::MaireTwo,
    ];

    pub fn choices(dark: bool) -> &'static [Self] {
        if dark { &Self::DARK } else { &Self::LIGHT }
    }

    /// The colour's English name. The interface translates [`Self::Theme`]'s.
    pub fn label(self) -> &'static str {
        match self {
            Self::Theme => "Theme",
            Self::Beige => "Beige",
            Self::Cruise => "Cruise",
            Self::Scandal => "Scandal",
            Self::MonteCarlo => "Monte Carlo",
            Self::HawkesBlue => "Hawkes Blue",
            Self::Downy => "Downy",
            Self::Seagull => "Seagull",
            Self::Quartz => "Quartz",
            Self::VeryLightGrey => "Very light grey",
            Self::Orinoco => "Orinoco",
            Self::Tusk => "Tusk",
            Self::CapeHoney => "Cape honey",
            Self::Caramel => "Caramel",
            Self::RoseBud => "Rose bud",
            Self::Bittersweet => "Bittersweet",
            Self::RadicalRed => "Radical red",
            Self::MandarinOrange => "Mandarin Orange",
            Self::Flamingo => "Flamingo",
            Self::Buccaneer => "Buccaneer",
            Self::BreakerBay => "Breaker Bay",
            Self::Pelorous => "Pelorous",
            Self::ToryBlue => "Tory Blue",
            Self::Fiord => "Fiord",
            Self::Cinder => "Cinder",
            Self::Tolopea => "Tolopea",
            Self::Solitude => "Solitude",
            Self::Canary => "Canary",
            Self::WillowBrook => "Willow Brook",
            Self::Black => "Black",
            Self::Nordic => "Nordic",
            Self::CardinGreen => "Cardin Green",
            Self::Tangaroa => "Tangaroa",
            Self::Tiber => "Tiber",
            Self::BlackRussian => "Black Russian",
            Self::Nero => "Nero",
            Self::Marshland => "Marshland",
            Self::Maire => "Maire",
            Self::BlackMagic => "Black Magic",
            Self::CocoaBrown => "Cocoa Brown",
            Self::WoodBark => "Wood Bark",
            Self::SealBrown => "Seal Brown",
            Self::SealBrownDarker => "Seal Brown Darker",
            Self::SealBrownLight => "Seal Brown Light",
            Self::Cyprus => "Cyprus",
            Self::BlueWhale => "Blue Whale",
            Self::BlackPearl => "Black Pearl",
            Self::DarkTolopea => "Tolopea",
            Self::Woodsmoke => "Woodsmoke",
            Self::MaireTwo => "Maire 2",
        }
    }

    /// A fixed colour's value; `None` for [`Self::Theme`], which follows the
    /// palette.
    pub fn rgb(self) -> Option<[u8; 3]> {
        Some(match self {
            Self::Theme => return None,
            Self::Beige => [245, 241, 235],
            Self::Cruise => [187, 228, 229],
            Self::Scandal => [174, 216, 199],
            Self::MonteCarlo => [122, 203, 165],
            Self::HawkesBlue => [203, 218, 236],
            Self::Downy => [102, 210, 213],
            Self::Seagull => [99, 189, 207],
            Self::Quartz => [214, 208, 240],
            Self::VeryLightGrey => [206, 206, 206],
            Self::Orinoco => [209, 218, 190],
            Self::Tusk => [230, 225, 177],
            Self::CapeHoney => [254, 239, 169],
            Self::Caramel => [254, 210, 151],
            Self::RoseBud => [253, 154, 155],
            Self::Bittersweet => [253, 103, 105],
            Self::RadicalRed => [251, 70, 104],
            Self::MandarinOrange => [146, 32, 64],
            Self::Flamingo => [220, 110, 79],
            Self::Buccaneer => [100, 77, 82],
            Self::BreakerBay => [81, 126, 126],
            Self::Pelorous => [49, 144, 187],
            Self::ToryBlue => [53, 85, 138],
            Self::Fiord => [85, 98, 111],
            Self::Cinder => [29, 35, 38],
            Self::Tolopea => [48, 30, 52],
            Self::Solitude => [236, 240, 241],
            Self::Canary => [255, 254, 162],
            Self::WillowBrook => [231, 232, 210],
            Self::Black => [22, 23, 23],
            Self::Nordic => [15, 36, 36],
            Self::CardinGreen => [18, 38, 31],
            Self::Tangaroa => [17, 30, 39],
            Self::Tiber => [14, 33, 37],
            Self::BlackRussian => [31, 29, 37],
            Self::Nero => [33, 33, 33],
            Self::Marshland => [31, 33, 28],
            Self::Maire => [35, 35, 27],
            Self::BlackMagic => [38, 36, 25],
            Self::CocoaBrown => [38, 31, 23],
            Self::WoodBark => [38, 23, 23],
            Self::SealBrown => [38, 15, 16],
            Self::SealBrownDarker => [25, 5, 11],
            Self::SealBrownLight => [33, 16, 12],
            Self::Cyprus => [10, 29, 37],
            Self::BlueWhale => [13, 21, 35],
            Self::BlackPearl => [13, 15, 17],
            Self::DarkTolopea => [17, 11, 18],
            Self::Woodsmoke => [30, 31, 31],
            Self::MaireTwo => [35, 35, 31],
        })
    }

    /// The colour drawn under `palette`: [`Self::Theme`] is its chat colour,
    /// read at draw time so a theme switch or Omarchy reload shows at once.
    pub fn color32(self, palette: &crate::theme::Palette) -> egui::Color32 {
        match self.rgb() {
            Some([r, g, b]) => egui::Color32::from_rgb(r, g, b),
            None => palette.chat,
        }
    }
}

/// How long ZapFast may go without input before the app lock locks it:
/// WhatsApp Web's three choices.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoLock {
    OneMinute,
    #[default]
    FifteenMinutes,
    OneHour,
}

impl AutoLock {
    pub const ALL: [AutoLock; 3] = [Self::OneMinute, Self::FifteenMinutes, Self::OneHour];

    pub fn duration(self) -> std::time::Duration {
        std::time::Duration::from_secs(match self {
            Self::OneMinute => 60,
            Self::FifteenMinutes => 15 * 60,
            Self::OneHour => 60 * 60,
        })
    }

    pub fn label(self, locale: crate::i18n::Locale) -> std::borrow::Cow<'static, str> {
        match self {
            Self::OneMinute => crate::i18n::gettext(locale, "After 1 minute"),
            Self::FifteenMinutes => crate::i18n::gettext(locale, "After 15 minutes"),
            Self::OneHour => crate::i18n::gettext(locale, "After 1 hour"),
        }
    }
}

/// What a two-finger touchpad swipe does. The chat list and the open chat
/// each have a swipe left and a swipe right; [`Self::LIST`] and
/// [`Self::CHAT`] are the choices each place offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SwipeAction {
    None,
    /// Replies to the message under the fingers.
    Reply,
    OpenChat,
    CloseChat,
    NextChat,
    PreviousChat,
    ToggleRead,
    TogglePin,
    ToggleArchive,
    ToggleMute,
    ToggleFavorite,
    /// Asks to delete the chat, as the menu's Delete chat does.
    Delete,
}

impl SwipeAction {
    /// The choices for a chat in the list.
    pub const LIST: [SwipeAction; 8] = [
        Self::None,
        Self::OpenChat,
        Self::ToggleRead,
        Self::TogglePin,
        Self::ToggleArchive,
        Self::ToggleMute,
        Self::ToggleFavorite,
        Self::Delete,
    ];

    /// The choices for the open chat.
    pub const CHAT: [SwipeAction; 11] = [
        Self::None,
        Self::Reply,
        Self::CloseChat,
        Self::NextChat,
        Self::PreviousChat,
        Self::ToggleRead,
        Self::TogglePin,
        Self::ToggleArchive,
        Self::ToggleMute,
        Self::ToggleFavorite,
        Self::Delete,
    ];

    pub fn label(self, locale: crate::i18n::Locale) -> std::borrow::Cow<'static, str> {
        crate::i18n::gettext(
            locale,
            match self {
                Self::None => "Nothing",
                Self::Reply => "Reply to the message",
                Self::OpenChat => "Open the chat",
                Self::CloseChat => "Back to the chat list",
                Self::NextChat => "Next chat",
                Self::PreviousChat => "Previous chat",
                Self::ToggleRead => "Mark as read or unread",
                Self::TogglePin => "Pin or unpin",
                Self::ToggleArchive => "Archive or unarchive",
                Self::ToggleMute => "Mute or unmute",
                Self::ToggleFavorite => "Add or remove favorite",
                Self::Delete => "Delete chat",
            },
        )
    }
}

/// One of the four swipes [`Swipes`] holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SwipeSlot {
    ListLeft,
    ListRight,
    ChatLeft,
    ChatRight,
}

impl SwipeSlot {
    pub const ALL: [SwipeSlot; 4] = [
        Self::ListLeft,
        Self::ListRight,
        Self::ChatLeft,
        Self::ChatRight,
    ];

    /// Which way the fingers move: in the open chat or over the list.
    pub fn of(in_chat: bool, right: bool) -> Self {
        match (in_chat, right) {
            (false, false) => Self::ListLeft,
            (false, true) => Self::ListRight,
            (true, false) => Self::ChatLeft,
            (true, true) => Self::ChatRight,
        }
    }

    /// The actions this place offers.
    pub fn choices(self) -> &'static [SwipeAction] {
        match self {
            Self::ListLeft | Self::ListRight => &SwipeAction::LIST,
            Self::ChatLeft | Self::ChatRight => &SwipeAction::CHAT,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::ListLeft => "Swipe left on a chat",
            Self::ListRight => "Swipe right on a chat",
            Self::ChatLeft => "Swipe left in the open chat",
            Self::ChatRight => "Swipe right in the open chat",
        }
    }
}

/// What each two-finger touchpad swipe does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Swipes {
    pub list_left: SwipeAction,
    pub list_right: SwipeAction,
    pub chat_left: SwipeAction,
    pub chat_right: SwipeAction,
}

impl Default for Swipes {
    /// As on the phone: left archives a chat and replies to a message, right
    /// marks a chat read or unread and goes back from the open chat.
    fn default() -> Self {
        Self {
            list_left: SwipeAction::ToggleArchive,
            list_right: SwipeAction::ToggleRead,
            chat_left: SwipeAction::Reply,
            chat_right: SwipeAction::CloseChat,
        }
    }
}

impl Swipes {
    /// The slot's action; one a place does not offer does nothing there.
    pub fn get(&self, slot: SwipeSlot) -> SwipeAction {
        let action = match slot {
            SwipeSlot::ListLeft => self.list_left,
            SwipeSlot::ListRight => self.list_right,
            SwipeSlot::ChatLeft => self.chat_left,
            SwipeSlot::ChatRight => self.chat_right,
        };
        if slot.choices().contains(&action) {
            action
        } else {
            SwipeAction::None
        }
    }

    pub fn set(&mut self, slot: SwipeSlot, action: SwipeAction) {
        *match slot {
            SwipeSlot::ListLeft => &mut self.list_left,
            SwipeSlot::ListRight => &mut self.list_right,
            SwipeSlot::ChatLeft => &mut self.chat_left,
            SwipeSlot::ChatRight => &mut self.chat_right,
        } = action;
    }
}

/// Which chats show one notification, then stay quiet for ten minutes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationLimit {
    #[default]
    Off,
    Direct,
    Group,
    Both,
}

impl NotificationLimit {
    pub const ALL: [NotificationLimit; 4] = [Self::Off, Self::Direct, Self::Group, Self::Both];

    /// Whether a chat of this kind is limited.
    pub fn applies(self, is_group: bool) -> bool {
        match self {
            Self::Off => false,
            Self::Direct => !is_group,
            Self::Group => is_group,
            Self::Both => true,
        }
    }

    pub fn label(self, locale: crate::i18n::Locale) -> std::borrow::Cow<'static, str> {
        match self {
            Self::Off => crate::i18n::gettext(locale, "Off"),
            Self::Direct => crate::i18n::gettext(locale, "Direct messages"),
            Self::Group => crate::i18n::gettext(locale, "Groups"),
            Self::Both => crate::i18n::gettext(locale, "Direct messages and groups"),
        }
    }
}

/// The sound a new-message notification makes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationSound {
    /// Pidgin's message sound, the default for new messages.
    #[default]
    #[serde(alias = "chime")]
    Receive,
    /// Pidgin's alert sound, the default for mentions and replies to us.
    #[serde(alias = "ripple")]
    Alert,
    /// Whatever the operating system plays for notifications.
    System,
    /// No sound.
    None,
    /// Zeus's Halloween message sound: a music box that sinks into a ghost.
    Spooky,
    /// Zeus's Halloween mention sound: bat squeaks over a low gong.
    Witching,
    /// An audio file ZapFast plays itself.
    Custom(std::path::PathBuf),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// [`SETTINGS_VERSION`] when written; missing, and so 0, in older files.
    #[serde(default)]
    pub version: u32,
    /// The theme as releases before chat themes stored it, kept in step with
    /// the fields below so an older release still reads a fitting one. Read
    /// only by [`Settings::migrate`]; set through [`Settings::set_theme`].
    #[serde(rename = "theme")]
    pub(crate) legacy_theme: ThemeChoice,
    /// The family of colours, light or dark as [`Settings::mode`] says.
    pub chat_theme: ChatTheme,
    /// Light, dark, or as the system is.
    pub mode: Mode,
    /// An accent over the theme's own.
    pub accent: Accent,
    /// The look the Custom style last used, to return to from Themes.
    pub last_look: ChatTheme,
    /// The ready-made theme last used, to return to from Custom.
    pub last_theme: ChatTheme,
    /// Frosted glass under the header and composer, and the desktop through
    /// the window where the platform offers it, for themes that draw glass.
    pub blur: bool,
    /// Whether every bubble shows its time and ticks inside it. Zeus's own
    /// look otherwise shows them below the bubble on a click.
    pub always_show_time: bool,
    /// Whether a `"system"` theme is really Messages. The file says
    /// `"system"` for Messages, which releases before it read as Follow
    /// system instead of rejecting the whole file; this marks the rest.
    #[serde(skip_serializing)]
    pub(crate) messages_theme: bool,
    /// The interface's typeface.
    pub font: FontChoice,
    /// Interface language. `None` follows the operating system's locale.
    pub interface_language: Option<crate::i18n::Locale>,
    /// Filename of the selected local JSON palette.
    pub custom_theme: Option<String>,
    #[serde(
        default,
        deserialize_with = "fastframe_theme::read_cached_theme",
        skip_serializing_if = "Option::is_none"
    )]
    pub custom_theme_cache: Option<crate::theme::CustomTheme>,
    #[serde(
        default,
        deserialize_with = "fastframe_theme::read_cached_theme",
        skip_serializing_if = "Option::is_none"
    )]
    pub system_theme_cache: Option<crate::theme::CustomTheme>,
    /// egui zoom factor.
    pub zoom: f32,
    pub sidebar_width: f32,
    /// Width of the search pane beside the open chat.
    pub search_pane_width: f32,
    /// Width of the info panel beside the open chat.
    pub info_pane_width: f32,
    /// Whether Enter sends. Off, Enter adds a line and Ctrl+Enter (Cmd+Enter
    /// on macOS) sends.
    pub enter_sends: bool,
    /// Send read receipts, subject to the account privacy setting.
    pub send_read_receipts: bool,
    /// Send typing state while composing.
    pub send_typing: bool,
    /// Download attachments when they enter view instead of on click.
    #[serde(alias = "auto_download_images")]
    pub auto_download: bool,
    /// Zeus's Halloween look: an orange theme, a night wallpaper with
    /// pumpkins, bats and ghosts, a pumpkin for the chat icon, and spooky
    /// sounds. Turning it on or off also swaps the doodles and sounds it set;
    /// see [`Settings::set_halloween`].
    pub halloween: bool,
    /// Which parts of the Halloween look apply while it is on.
    pub halloween_parts: HalloweenParts,
    /// What two-finger touchpad swipes do in the chat list and the open chat.
    pub swipes: Swipes,
    /// Show the default doodle wallpaper behind conversations.
    pub show_wallpaper: bool,
    /// Which doodles `show_wallpaper` draws.
    pub doodle_style: DoodleStyle,
    /// The colour of those doodles.
    pub doodle_color: DoodleColor,
    /// How strongly they show, as a multiple of the default (1.0).
    pub doodle_intensity: f32,
    /// Colour selected in the wallpaper picker.
    pub wallpaper_color: WallpaperColor,
    /// Colour selected for the dark wallpaper picker.
    pub dark_wallpaper_color: WallpaperColor,
    /// ZapFast's own copy of the chosen wallpaper image, drawn in place of the
    /// colour and doodles in light and dark mode alike.
    pub wallpaper_image: Option<std::path::PathBuf>,
    /// Last open chat, restored at startup.
    pub last_chat: Option<String>,
    /// The hint bar under the composer, hidden with its × and shown again
    /// from the Keyboard shortcuts dialog.
    pub show_shortcut_hints: bool,
    /// The picker tab used last: the composer's button shows its icon and
    /// opens it.
    pub picker_mode: crate::model::PickerTab,
    /// Recently used emoji, newest first.
    pub recent_emoji: Vec<String>,
    /// Emoji reaction usage on this device, most-used first (recent breaks ties).
    pub reaction_emoji: Vec<(String, u32)>,
    /// User GIPHY API key. Empty uses the optional built-in key.
    pub giphy_key: String,
    /// Keep the app linked in the tray when the window closes.
    pub keep_running_in_background: bool,
    /// Desktop notifications while away from the chat.
    pub notifications: bool,
    /// Notifications name the chat and sender and show the message. Off,
    /// every one says only "New message", as behind the app lock.
    pub notification_content: bool,
    /// Sound for new messages, in one-to-one chats and groups alike.
    pub message_sound: NotificationSound,
    /// Sound for group messages that mention us or reply to one of ours, as
    /// Pidgin alerts when someone says your name in a chat.
    pub mention_sound: NotificationSound,
    /// Play the message sound for ordinary group messages. Mentions and
    /// replies to us sound either way.
    pub group_sounds: bool,
    /// Notification cooldown: when many messages arrive at once, such as
    /// right after the app opens, only the first sounds and each chat shows
    /// one notification.
    pub notification_cooldown: bool,
    /// Folder for new downloads. `None` keeps them in the cache. Files
    /// already downloaded stay where they are when this changes.
    pub download_folder: Option<std::path::PathBuf>,
    /// Archived chats stay archived when a message arrives or is sent, as
    /// with the phone's "Keep chats archived". Off, a new message brings the
    /// chat back to the list.
    pub keep_chats_archived: bool,
    /// Proxy for WhatsApp, media, and updates, such as
    /// `socks5h://127.0.0.1:9050`. Empty follows `ALL_PROXY` / `HTTPS_PROXY`.
    pub proxy: String,
    /// Ask GitHub every two hours whether a newer release exists.
    pub check_for_updates: bool,
    /// Whether updates follow stable releases or betas too.
    pub update_channel: crate::updates::Channel,
    /// Download verified updates in the background; restarting remains explicit.
    pub download_updates_automatically: bool,
    /// Voice and audio playback speed multiplier.
    pub voice_speed: f32,
    /// The microphone calls and voice messages use; `None` follows the
    /// system default.
    pub microphone: Option<SavedDevice>,
    /// Keeps a call's microphone silent until [`Self::push_to_talk_key`] is
    /// held.
    pub push_to_talk: bool,
    /// The key held to talk, by egui's name for it (`Key::name`).
    pub push_to_talk_key: String,
    /// Pause other apps' media while recording, or while a voice message,
    /// audio, or video plays with sound.
    pub pause_other_media: bool,
    /// The two switches `pause_other_media` replaced, read once and folded
    /// into it by [`Settings::load`].
    #[serde(skip_serializing)]
    pub pause_media_while_recording: Option<bool>,
    #[serde(skip_serializing)]
    pub pause_media_while_playing: Option<bool>,
    /// The last choice of the new-contact dialog's "Save to phone" box, which
    /// starts the next one and applies when a contact is renamed.
    pub save_contacts_to_phone: bool,
    /// Legacy plaintext code, accepted once and rewritten as a verifier.
    #[serde(skip_serializing)]
    pub chat_lock_code: Option<String>,
    /// Salted PBKDF2 verifier for the local locked-chats code, `salt$hash`.
    pub chat_lock_code_hash: Option<String>,
    /// The one-time locked-chat code hint has been opened.
    pub chat_lock_hint_dismissed: bool,
    /// Salted, slow verifier of the app lock password
    /// ([`crate::app_lock::verifier`]); `None` leaves the app lock off.
    pub app_lock_hash: Option<String>,
    /// How long ZapFast may go unused before the app lock locks it.
    pub app_lock_after: AutoLock,
    /// Last window width in logical pixels (points at zoom 1), remembered
    /// across restarts; see [`fastframe_shell::window::Geometry`]. `None`
    /// uses the default size.
    pub window_width: Option<f32>,
    /// Last window height in logical pixels. See [`Self::window_width`].
    pub window_height: Option<f32>,
    /// Last window position in logical pixels, as
    /// `ViewportBuilder::with_position` takes it. `None` lets the window
    /// manager place the window, which is also the case on Wayland where the
    /// position is not known.
    pub window_x: Option<f32>,
    /// Last window vertical position. See [`Self::window_x`].
    pub window_y: Option<f32>,
    /// Whether the window was maximized when it last closed.
    pub window_maximized: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            legacy_theme: ThemeChoice::Dark,
            chat_theme: ChatTheme::WhatsApp,
            mode: Mode::Dark,
            accent: Accent::Theme,
            last_look: ChatTheme::WhatsApp,
            last_theme: ChatTheme::Catppuccin,
            blur: true,
            always_show_time: false,
            messages_theme: false,
            font: FontChoice::GoogleSansFlex,
            interface_language: None,
            custom_theme: None,
            custom_theme_cache: None,
            system_theme_cache: None,
            zoom: 1.0,
            sidebar_width: 300.0,
            search_pane_width: 380.0,
            info_pane_width: 380.0,
            enter_sends: true,
            send_read_receipts: true,
            send_typing: true,
            auto_download: true,
            halloween: true,
            halloween_parts: HalloweenParts::default(),
            swipes: Swipes::default(),
            show_wallpaper: true,
            doodle_style: DoodleStyle::Halloween,
            doodle_color: DoodleColor::Accent,
            doodle_intensity: 1.0,
            wallpaper_color: WallpaperColor::Theme,
            dark_wallpaper_color: WallpaperColor::Theme,
            wallpaper_image: None,
            last_chat: None,
            show_shortcut_hints: true,
            picker_mode: crate::model::PickerTab::default(),
            recent_emoji: Vec::new(),
            reaction_emoji: Vec::new(),
            giphy_key: String::new(),
            keep_running_in_background: true,
            notifications: true,
            notification_content: true,
            message_sound: NotificationSound::Spooky,
            mention_sound: NotificationSound::Witching,
            group_sounds: true,
            notification_cooldown: true,
            download_folder: None,
            keep_chats_archived: true,
            proxy: String::new(),
            check_for_updates: true,
            update_channel: crate::updates::Channel::Stable,
            download_updates_automatically: false,
            save_contacts_to_phone: true,
            voice_speed: 1.0,
            microphone: None,
            push_to_talk: false,
            push_to_talk_key: DEFAULT_PUSH_TO_TALK_KEY.to_owned(),
            pause_other_media: true,
            pause_media_while_recording: None,
            pause_media_while_playing: None,
            chat_lock_code: None,
            chat_lock_code_hash: None,
            chat_lock_hint_dismissed: false,
            app_lock_hash: None,
            app_lock_after: AutoLock::default(),
            window_width: None,
            window_height: None,
            window_x: None,
            window_y: None,
            window_maximized: false,
        }
    }
}

/// The settings file's format. Files without a version predate it and are
/// migrated once by [`Settings::load`]:
///
/// 1. The wallpaper colours that were the defaults (Beige, and Black in dark
///    mode) become [`WallpaperColor::Theme`], the new default.
/// 2. The one theme menu becomes a chat theme, a mode and an accent
///    ([`Settings::set_theme`]).
pub const SETTINGS_VERSION: u32 = 3;

/// The key push to talk starts on.
pub const DEFAULT_PUSH_TO_TALK_KEY: &str = "Space";

/// An audio device picked in Settings: the system's id for it, which opens
/// it, and the name it had, to say which one is missing when it is gone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedDevice {
    pub id: String,
    pub name: String,
}

/// Optional build-time GIPHY key from `ZAPFAST_GIPHY_KEY`.
/// The previous name remains accepted for existing build setups.
pub const BUILT_IN_GIPHY_KEY: Option<&str> = match option_env!("ZAPFAST_GIPHY_KEY") {
    Some(key) if !key.is_empty() => Some(key),
    _ => option_env!("FASTSAPP_GIPHY_KEY"),
};

impl Settings {
    pub fn wallpaper_color_for(&self, dark: bool) -> WallpaperColor {
        if dark {
            self.dark_wallpaper_color
        } else {
            self.wallpaper_color
        }
    }

    /// The wallpaper colour for `palette`, with Theme resolved against it.
    pub fn wallpaper_background(&self, palette: &crate::theme::Palette) -> egui::Color32 {
        self.wallpaper_color_for(palette.dark).color32(palette)
    }

    /// Picks a theme as releases before chat themes named them: WhatsApp's
    /// Dark and Light, Zeus's Dark, Light and Follow system, Messages, and
    /// Zeus's dark colour themes.
    pub fn set_theme(&mut self, choice: ThemeChoice) {
        self.legacy_theme = choice;
        (self.chat_theme, self.mode, self.accent) = match choice {
            ThemeChoice::Dark => (ChatTheme::WhatsApp, Mode::Dark, Accent::Theme),
            ThemeChoice::Light => (ChatTheme::WhatsApp, Mode::Light, Accent::Theme),
            ThemeChoice::System => (ChatTheme::Zeus, Mode::System, Accent::Theme),
            ThemeChoice::Messages => (ChatTheme::Messages, Mode::System, Accent::Theme),
            ThemeChoice::NothingDark => (ChatTheme::Zeus, Mode::Dark, Accent::Theme),
            ThemeChoice::NothingLight => (ChatTheme::Zeus, Mode::Light, Accent::Theme),
            ThemeChoice::Red => (ChatTheme::Zeus, Mode::Dark, Accent::Red),
            ThemeChoice::Blue => (ChatTheme::Zeus, Mode::Dark, Accent::Blue),
            ThemeChoice::Green => (ChatTheme::Zeus, Mode::Dark, Accent::Green),
            ThemeChoice::Pink => (ChatTheme::Zeus, Mode::Dark, Accent::Pink),
            ThemeChoice::Purple => (ChatTheme::Zeus, Mode::Dark, Accent::Purple),
            ThemeChoice::Yellow => (ChatTheme::Zeus, Mode::Dark, Accent::Yellow),
        };
    }

    /// The nearest theme a release before chat themes knows.
    pub fn legacy_theme(&self) -> ThemeChoice {
        let dark = self.mode != Mode::Light;
        match self.chat_theme {
            ChatTheme::Messages => ThemeChoice::Messages,
            ChatTheme::WhatsApp if dark => ThemeChoice::Dark,
            ChatTheme::WhatsApp => ThemeChoice::Light,
            ChatTheme::Zeus if self.mode == Mode::System => ThemeChoice::System,
            _ if !dark => ThemeChoice::NothingLight,
            ChatTheme::Zeus => match self.accent {
                Accent::Red => ThemeChoice::Red,
                Accent::Blue => ThemeChoice::Blue,
                Accent::Green => ThemeChoice::Green,
                Accent::Pink => ThemeChoice::Pink,
                Accent::Purple => ThemeChoice::Purple,
                Accent::Yellow => ThemeChoice::Yellow,
                _ => ThemeChoice::NothingDark,
            },
            // A ready-made theme keeps its own accent.
            _ => ThemeChoice::NothingDark,
        }
    }

    /// Whether the colours are a look made your own or a ready-made theme.
    /// A theme file counts as a ready-made theme.
    pub fn style(&self) -> Style {
        if self.custom_theme.is_some() || !self.chat_theme.is_look() {
            Style::Themes
        } else {
            Style::Custom
        }
    }

    /// Draws the window in `choice`, leaving any theme file, and remembers
    /// it for when its style is chosen again.
    pub fn choose_chat_theme(&mut self, choice: ChatTheme) {
        self.chat_theme = choice;
        self.custom_theme = None;
        self.custom_theme_cache = None;
        if choice.is_look() {
            self.last_look = choice;
        } else {
            self.last_theme = choice;
        }
    }

    /// Switches to `style`, returning to the look or theme it last had.
    /// Returns whether anything changed.
    pub fn choose_style(&mut self, style: Style) -> bool {
        if self.style() == style {
            return false;
        }
        let choice = match style {
            Style::Custom if self.last_look.is_look() => self.last_look,
            Style::Custom => ChatTheme::WhatsApp,
            Style::Themes if !self.last_theme.is_look() => self.last_theme,
            Style::Themes => ChatTheme::Catppuccin,
        };
        self.choose_chat_theme(choice);
        true
    }

    /// Whether the window's colours are light, dark, or the system's, for
    /// egui's own controls as well as ours. A theme file decides for itself.
    pub fn preference(&self) -> egui::ThemePreference {
        match self.cached_palette() {
            Some(palette) if palette.dark => egui::ThemePreference::Dark,
            Some(_) => egui::ThemePreference::Light,
            None => match self.mode {
                Mode::System => egui::ThemePreference::System,
                Mode::Light => egui::ThemePreference::Light,
                Mode::Dark => egui::ThemePreference::Dark,
            },
        }
    }

    /// The palette to draw with, given whether the window is dark.
    pub fn palette(&self, dark: bool) -> crate::theme::Palette {
        if self.halloween_active() {
            return crate::theme::Palette::halloween(dark, self.accent.rgb());
        }
        self.cached_palette()
            .unwrap_or_else(|| self.chat_theme.palette(dark, self.accent))
    }

    /// Whether the Halloween colours are drawn: switched on, over Zeus's or
    /// WhatsApp's own Dark and Light. A ready-made or custom theme chosen on
    /// purpose keeps its colours.
    pub fn halloween_active(&self) -> bool {
        self.halloween
            && self.halloween_parts.colors
            && self.chat_theme.takes_accent()
            && self.cached_palette().is_none()
    }

    /// Whether the Halloween night wallpaper is drawn.
    pub fn halloween_wallpaper(&self) -> bool {
        self.halloween && self.halloween_parts.wallpaper
    }

    /// Whether the Halloween icons are drawn.
    pub fn halloween_icons(&self) -> bool {
        self.halloween && self.halloween_parts.icons
    }

    /// Turns the Halloween look on or off as a whole.
    pub fn set_halloween(&mut self, on: bool) {
        self.halloween = on;
        self.sync_halloween();
    }

    /// Turns one part of the Halloween look on or off.
    pub fn set_halloween_part(&mut self, part: HalloweenPart, on: bool) {
        match part {
            HalloweenPart::Colors => self.halloween_parts.colors = on,
            HalloweenPart::Wallpaper => self.halloween_parts.wallpaper = on,
            HalloweenPart::Doodles => self.halloween_parts.doodles = on,
            HalloweenPart::Icons => self.halloween_parts.icons = on,
            HalloweenPart::Sounds => self.halloween_parts.sounds = on,
        }
        self.sync_halloween();
    }

    /// Brings the doodles and sounds in line with the look: its doodles,
    /// orange doodle ink and sounds when they are on; the classic doodles,
    /// grey ink and Pidgin's sounds back when they are off. Anything the
    /// reader picked in their place stays.
    fn sync_halloween(&mut self) {
        if self.halloween && self.halloween_parts.doodles {
            if self.doodle_style == DoodleStyle::Classic {
                self.doodle_style = DoodleStyle::Halloween;
            }
            if self.doodle_color == DoodleColor::Gray {
                self.doodle_color = DoodleColor::Accent;
            }
        } else {
            if self.doodle_style == DoodleStyle::Halloween {
                self.doodle_style = DoodleStyle::Classic;
            }
            if self.doodle_color == DoodleColor::Accent {
                self.doodle_color = DoodleColor::Gray;
            }
        }
        if self.halloween && self.halloween_parts.sounds {
            if self.message_sound == NotificationSound::Receive {
                self.message_sound = NotificationSound::Spooky;
            }
            if self.mention_sound == NotificationSound::Alert {
                self.mention_sound = NotificationSound::Witching;
            }
        } else {
            if self.message_sound == NotificationSound::Spooky {
                self.message_sound = NotificationSound::Receive;
            }
            if self.mention_sound == NotificationSound::Witching {
                self.mention_sound = NotificationSound::Alert;
            }
        }
    }

    /// Whether the window should let the desktop show through it.
    pub fn vibrant_window(&self) -> bool {
        self.blur && self.chat_theme.vibrant()
    }

    pub(crate) fn cached_palette(&self) -> Option<crate::theme::Palette> {
        let theme = if self.custom_theme.is_some() {
            self.custom_theme_cache.as_ref()
        } else if self.chat_theme == ChatTheme::Zeus && self.mode == Mode::System {
            // Follow system on Linux follows Omarchy's colours.
            self.system_theme_cache.as_ref()
        } else {
            None
        };
        theme.map(|theme| theme.palette)
    }

    /// The push to talk key, or the default one when the saved name is not a
    /// key this egui knows.
    pub fn push_to_talk_key(&self) -> egui::Key {
        egui::Key::from_name(&self.push_to_talk_key)
            .filter(|key| *key != egui::Key::Escape)
            .unwrap_or(egui::Key::Space)
    }

    /// The id of the microphone calls and voice messages open.
    pub fn microphone_id(&self) -> Option<String> {
        self.microphone.as_ref().map(|device| device.id.clone())
    }

    /// Returns the user key, built-in key, or `None`.
    pub fn effective_giphy_key(&self) -> Option<String> {
        let own = self.giphy_key.trim();
        if !own.is_empty() {
            return Some(own.to_owned());
        }
        BUILT_IN_GIPHY_KEY
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .map(str::to_owned)
    }

    /// The remembered window geometry for the next window. A size or
    /// position counts only when both of its values are there.
    pub fn window_geometry(&self) -> fastframe_shell::window::Geometry {
        fastframe_shell::window::Geometry {
            size: self
                .window_width
                .zip(self.window_height)
                .map(<[f32; 2]>::from),
            position: self.window_x.zip(self.window_y).map(<[f32; 2]>::from),
            maximized: self.window_maximized,
        }
    }

    /// Stores the window geometry in the settings' own fields.
    pub fn set_window_geometry(&mut self, geometry: fastframe_shell::window::Geometry) {
        (self.window_width, self.window_height) = geometry.size.map(|[w, h]| (w, h)).unzip();
        (self.window_x, self.window_y) = geometry.position.map(|[x, y]| (x, y)).unzip();
        self.window_maximized = geometry.maximized;
    }

    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(contents) => match serde_json::from_str::<Self>(&contents) {
                Ok(mut settings) => {
                    settings.migrate();
                    settings.fold_legacy_media_pause();
                    if let Some(code) = settings.chat_lock_code.take() {
                        settings.set_chat_lock_code(Some(&code));
                        if let Err(error) = settings.save(path) {
                            log::warn!("could not replace the legacy locked-chat code: {error}");
                        }
                    }
                    settings
                }
                Err(_error) => {
                    log::warn!("settings file is unreadable, using defaults");
                    Self::default()
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(error) => {
                log::warn!("could not read settings: {error}");
                Self::default()
            }
        }
    }

    /// Replaces the settings file whole, never leaving it half written.
    /// Waits for the disk, so never on the thread that draws the window:
    /// there, [`Self::save_to`].
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        fastframe_file::write(path, self.contents()?)
    }

    /// Hands the settings file to `files`, which replaces it off this thread.
    pub fn save_to(&self, files: &fastframe_file::Writer, path: &Path) -> std::io::Result<()> {
        files.write(path, self.contents()?, fastframe_file::Access::Default);
        Ok(())
    }

    fn contents(&self) -> std::io::Result<String> {
        serde_json::to_string_pretty(&self.to_file()?).map_err(std::io::Error::other)
    }

    /// The settings as the file stores them: Messages is written as Follow
    /// system plus a mark, so older releases still read the file.
    fn to_file(&self) -> std::io::Result<serde_json::Value> {
        let mut file = self.clone();
        file.legacy_theme = self.legacy_theme();
        let mut value = serde_json::to_value(&file).map_err(std::io::Error::other)?;
        if file.legacy_theme == ThemeChoice::Messages
            && let Some(fields) = value.as_object_mut()
        {
            fields.insert("theme".into(), serde_json::to_value(ThemeChoice::System)?);
            fields.insert("messages_theme".into(), true.into());
        }
        Ok(value)
    }

    /// Brings a file written before [`SETTINGS_VERSION`] up to date. Each
    /// step runs once: the next save records the version, so a colour chosen
    /// again afterwards is kept.
    fn migrate(&mut self) {
        if std::mem::take(&mut self.messages_theme) && self.legacy_theme == ThemeChoice::System {
            self.legacy_theme = ThemeChoice::Messages;
        }
        if self.version < 1 {
            // Old files store every field, so a default colour cannot be told
            // from a chosen one; the old defaults move to the new one.
            if self.wallpaper_color == WallpaperColor::Beige {
                self.wallpaper_color = WallpaperColor::Theme;
            }
            if self.dark_wallpaper_color == WallpaperColor::Black {
                self.dark_wallpaper_color = WallpaperColor::Theme;
            }
        }
        if self.version < 2 {
            self.set_theme(self.legacy_theme);
        }
        if self.version < 3 {
            // Halloween ships on: older files keep the look they had until
            // now, so it brings its doodles and sounds in as turning it on does.
            self.set_halloween(true);
        }
        self.version = self.version.max(SETTINGS_VERSION);
    }

    /// Folds the former recording and playback switches into
    /// `pause_other_media`. A file that still has them was last written by a
    /// version without the merged switch, so they win: media keeps pausing
    /// only if neither was turned off, which never pauses music for someone
    /// who asked it not to be. The next save drops them.
    fn fold_legacy_media_pause(&mut self) {
        let recording = self.pause_media_while_recording.take();
        let playing = self.pause_media_while_playing.take();
        if recording.is_some() || playing.is_some() {
            self.pause_other_media = recording.unwrap_or(true) && playing.unwrap_or(true);
        }
    }

    pub fn set_chat_lock_code(&mut self, code: Option<&str>) {
        self.chat_lock_code = None;
        self.chat_lock_code_hash = code
            .map(str::trim)
            .filter(|code| !code.is_empty())
            .map(Self::chat_lock_verifier);
    }

    /// Checking is deliberately slow, so callers memoize the answer.
    pub fn verifies_chat_lock_code(&self, code: &str) -> bool {
        let Some((salt, expected)) = self
            .chat_lock_code_hash
            .as_deref()
            .and_then(|stored| stored.split_once('$'))
        else {
            return false;
        };
        let (Some(salt), Some(expected)) = (unhex(salt), unhex(expected)) else {
            return false;
        };
        ring::pbkdf2::verify(
            ring::pbkdf2::PBKDF2_HMAC_SHA256,
            CHAT_LOCK_ROUNDS,
            &salt,
            code.trim().as_bytes(),
            &expected,
        )
        .is_ok()
    }

    /// `salt$hash`, both hex. Codes are short enough to be guessed offline,
    /// so the stored form is salted and slow rather than a bare digest.
    fn chat_lock_verifier(code: &str) -> String {
        let salt: [u8; 16] = ring::rand::generate(&ring::rand::SystemRandom::new())
            .expect("the system random generator is unavailable")
            .expose();
        let mut hash = [0u8; 32];
        ring::pbkdf2::derive(
            ring::pbkdf2::PBKDF2_HMAC_SHA256,
            CHAT_LOCK_ROUNDS,
            &salt,
            code.as_bytes(),
            &mut hash,
        );
        format!("{}${}", hex(&salt), hex(&hash))
    }
}

/// Preferences that belong to one WhatsApp account.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AccountSettings {
    pub send_read_receipts: bool,
    pub send_typing: bool,
    #[serde(alias = "auto_download_images")]
    pub auto_download: bool,
    pub last_chat: Option<String>,
    pub notifications: bool,
    /// Which chats notify once, then stay quiet for ten minutes.
    pub limit_notifications: NotificationLimit,
    pub save_contacts_to_phone: bool,
    /// This account's copy of the chosen chat wallpaper image.
    pub wallpaper_image: Option<std::path::PathBuf>,
    /// Whether this account's phone is asked for older history in the
    /// background. Off by default: it asks the phone for history nobody
    /// scrolled to, so an account that has never turned it on does not.
    pub history_prefetch: bool,
    /// The chip or label the chat list last showed, as
    /// [`crate::model::ListFilter::key`] writes it; `None` shows every chat.
    pub list_filter: Option<String>,
    /// The chip the Calls page last showed, as
    /// [`crate::model::CallFilter::key`] writes it; `None` shows every call.
    pub call_filter: Option<String>,
}

impl Default for AccountSettings {
    fn default() -> Self {
        Self {
            send_read_receipts: true,
            send_typing: true,
            auto_download: true,
            last_chat: None,
            notifications: true,
            limit_notifications: NotificationLimit::Off,
            save_contacts_to_phone: true,
            wallpaper_image: None,
            history_prefetch: false,
            list_filter: None,
            call_filter: None,
        }
    }
}

impl AccountSettings {
    pub fn from_legacy(settings: &Settings) -> Self {
        Self {
            send_read_receipts: settings.send_read_receipts,
            send_typing: settings.send_typing,
            auto_download: settings.auto_download,
            last_chat: settings.last_chat.clone(),
            notifications: settings.notifications,
            limit_notifications: NotificationLimit::Off,
            save_contacts_to_phone: settings.save_contacts_to_phone,
            wallpaper_image: settings.wallpaper_image.clone(),
            history_prefetch: false,
            list_filter: None,
            call_filter: None,
        }
    }

    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_else(|error| {
                log::warn!("account settings are unreadable, using the defaults: {error}");
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    /// Replaces the file whole, waiting for the disk: only before the
    /// window's thread is running (the first account's migration).
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let contents = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        fastframe_file::write(path, contents)
    }

    /// Hands the file to `files`, which replaces it off this thread.
    pub fn save_to(&self, files: &fastframe_file::Writer, path: &Path) -> std::io::Result<()> {
        let contents = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        files.write(path, contents, fastframe_file::Access::Default);
        Ok(())
    }
}

/// Which accounts exist on this computer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AccountRoster {
    pub next_id: u32,
    pub active: String,
    pub order: Vec<String>,
}

impl Default for AccountRoster {
    fn default() -> Self {
        Self {
            next_id: 2,
            active: "1".into(),
            order: vec!["1".into()],
        }
    }
}

impl AccountRoster {
    pub fn load(path: &Path) -> std::io::Result<Self> {
        let mut roster = match std::fs::read_to_string(path) {
            Ok(contents) => serde_json::from_str(&contents).map_err(std::io::Error::other)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(error) => return Err(error),
        };
        roster.sanitize();
        Ok(roster)
    }

    /// Drops ids that cannot be folder names, so a hand-edited file cannot
    /// point state or a later delete at an arbitrary path. Keeps the first
    /// spelling of a repeated id so two workers never share one archive.
    pub fn sanitize(&mut self) {
        let mut seen = HashSet::new();
        self.order
            .retain(|id| crate::model::AccountId::is_safe(id) && seen.insert(id.clone()));
        if !self.order.iter().any(|id| id == &self.active) {
            self.active = self.order.first().cloned().unwrap_or_else(|| "1".into());
        }
        let highest = self
            .order
            .iter()
            .filter_map(|id| id.parse::<u32>().ok())
            .max()
            .unwrap_or(1);
        self.next_id = self.next_id.max(highest + 1).max(2);
    }

    /// Hands the file to `files`, which replaces it off this thread.
    pub fn save_to(&self, files: &fastframe_file::Writer, path: &Path) -> std::io::Result<()> {
        let contents = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        files.write(path, contents, fastframe_file::Access::Default);
        Ok(())
    }

    pub fn allocate(&mut self) -> crate::model::AccountId {
        let id = crate::model::AccountId(self.next_id.to_string());
        self.next_id += 1;
        self.order.push(id.0.clone());
        self.active = id.0.clone();
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swipes_default_without_a_setting_and_ignore_a_choice_out_of_place() {
        let swipes: Swipes = serde_json::from_str(r#"{"chat_left":"close_chat"}"#).unwrap();
        assert_eq!(swipes.get(SwipeSlot::ChatLeft), SwipeAction::CloseChat);
        assert_eq!(swipes.get(SwipeSlot::ListLeft), SwipeAction::ToggleArchive);
        let mut swipes = Swipes::default();
        swipes.set(SwipeSlot::ListRight, SwipeAction::Reply);
        assert_eq!(swipes.get(SwipeSlot::ListRight), SwipeAction::None);
        for slot in SwipeSlot::ALL {
            assert!(slot.choices().contains(&Swipes::default().get(slot)));
        }
    }

    #[test]
    fn halloween_ships_on_and_turning_it_off_restores_the_classic_look() {
        let mut settings = Settings::default();
        assert!(settings.halloween && settings.halloween_active());
        assert_eq!(settings.doodle_style, DoodleStyle::Halloween);
        assert_eq!(settings.message_sound, NotificationSound::Spooky);
        let spooky = settings.palette(true);
        settings.set_halloween(false);
        assert!(!settings.halloween_active());
        assert_eq!(settings.doodle_style, DoodleStyle::Classic);
        assert_eq!(settings.doodle_color, DoodleColor::Gray);
        assert_eq!(settings.message_sound, NotificationSound::Receive);
        assert_eq!(settings.mention_sound, NotificationSound::Alert);
        assert_ne!(settings.palette(true).accent, spooky.accent);
        settings.set_halloween(true);
        assert_eq!(settings.doodle_style, DoodleStyle::Halloween);
        assert_eq!(settings.mention_sound, NotificationSound::Witching);
    }

    #[test]
    fn each_halloween_part_switches_on_its_own() {
        let mut settings = Settings::default();
        settings.set_halloween_part(HalloweenPart::Colors, false);
        assert!(!settings.halloween_active());
        assert!(settings.halloween_wallpaper() && settings.halloween_icons());
        assert_eq!(settings.doodle_style, DoodleStyle::Halloween);
        settings.set_halloween_part(HalloweenPart::Doodles, false);
        assert_eq!(settings.doodle_style, DoodleStyle::Classic);
        settings.set_halloween_part(HalloweenPart::Sounds, false);
        assert_eq!(settings.message_sound, NotificationSound::Receive);
        settings.set_halloween_part(HalloweenPart::Icons, false);
        assert!(!settings.halloween_icons());
        settings.set_halloween(false);
        settings.set_halloween(true);
        assert_eq!(settings.doodle_style, DoodleStyle::Classic);
    }

    #[test]
    fn turning_halloween_off_keeps_what_the_reader_chose() {
        let mut settings = Settings::default();
        settings.doodle_style = DoodleStyle::Space;
        settings.message_sound = NotificationSound::None;
        settings.set_halloween(false);
        assert_eq!(settings.doodle_style, DoodleStyle::Space);
        assert_eq!(settings.message_sound, NotificationSound::None);
        settings.set_halloween(true);
        assert_eq!(settings.doodle_style, DoodleStyle::Space);
        assert_eq!(settings.message_sound, NotificationSound::None);
    }

    #[test]
    fn halloween_is_orange_in_light_and_dark_and_yields_to_a_chosen_accent() {
        let settings = Settings::default();
        for dark in [true, false] {
            let palette = settings.palette(dark);
            assert!(
                palette.accent.r() > 200 && palette.accent.b() < 80,
                "{dark}"
            );
        }
        let blue = Settings {
            accent: Accent::Blue,
            ..Settings::default()
        };
        assert!(blue.palette(true).accent.b() > 200);
    }

    #[test]
    fn a_ready_made_theme_chosen_on_purpose_keeps_its_colours() {
        let mut settings = Settings::default();
        settings.chat_theme = ChatTheme::Dracula;
        assert!(!settings.halloween_active());
    }

    #[test]
    fn earlier_bundled_sound_names_still_load() {
        let settings: Settings =
            serde_json::from_str(r#"{"message_sound":"chime","mention_sound":"ripple"}"#).unwrap();
        assert_eq!(settings.message_sound, NotificationSound::Receive);
        assert_eq!(settings.mention_sound, NotificationSound::Alert);
    }

    #[test]
    fn the_former_group_sound_gives_way_to_mentions_and_quiet_groups() {
        // Group sound used to play for every group message; it is dropped,
        // and groups follow the message sound until they are silenced.
        let settings: Settings =
            serde_json::from_str(r#"{"message_sound":"system","group_sound":"none"}"#).unwrap();
        assert_eq!(settings.message_sound, NotificationSound::System);
        assert_eq!(settings.mention_sound, NotificationSound::Witching);
        assert!(settings.group_sounds);
    }

    /// Settings written before the font could be chosen use the current
    /// default; explicit choices are saved by name.
    #[test]
    fn every_font_is_saved_by_a_name_it_reads_back() {
        for font in FontChoice::ALL {
            let saved = serde_json::to_value(Settings {
                font,
                ..Settings::default()
            })
            .unwrap();
            let read: Settings = serde_json::from_value(saved).unwrap();
            assert_eq!(read.font, font, "{}", font.label());
        }
    }

    #[test]
    fn the_font_defaults_to_google_sans_flex_and_is_saved_by_name() {
        let older: Settings = serde_json::from_str(r#"{"theme":"light"}"#).unwrap();
        assert_eq!(older.font, FontChoice::GoogleSansFlex);
        let chosen: Settings = serde_json::from_str(r#"{"font":"inter"}"#).unwrap();
        assert_eq!(chosen.font, FontChoice::Inter);
        let saved = serde_json::to_value(&chosen).unwrap();
        assert_eq!(saved["font"], "inter");
    }

    /// The Messages theme is saved by name, and palettes cached before
    /// bubble styles existed keep WhatsApp's bubbles.
    #[test]
    fn messages_theme_round_trips_and_old_palettes_keep_whatsapp_bubbles() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut settings = Settings::default();
        settings.set_theme(ThemeChoice::Messages);
        settings.save(&path).unwrap();
        // Releases before Messages read the file, and see Follow system.
        let written: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(written["theme"], "system");
        assert_eq!(written["messages_theme"], true);
        assert_eq!(Settings::load(&path).legacy_theme(), ThemeChoice::Messages);
        // Follow system stays Follow system, without the mark.
        let mut system = Settings::default();
        system.set_theme(ThemeChoice::System);
        system.save(&path).unwrap();
        assert!(
            !std::fs::read_to_string(&path)
                .unwrap()
                .contains("messages_theme")
        );
        assert_eq!(Settings::load(&path).legacy_theme(), ThemeChoice::System);
        // Files written by development builds before this still load.
        let parsed: Settings = serde_json::from_str(r#"{"theme":"messages"}"#).expect("parses");
        assert_eq!(parsed.legacy_theme, ThemeChoice::Messages);
        let mut cached = serde_json::to_value(crate::theme::Palette::dark()).unwrap();
        cached.as_object_mut().unwrap().remove("bubbles");
        let cached: crate::theme::Palette = serde_json::from_value(cached).unwrap();
        assert_eq!(cached.bubbles, crate::theme::BubbleStyle::WhatsApp);
    }

    /// A file from before chat themes keeps its look: its one theme becomes
    /// a chat theme, a mode and an accent, and an older release reading the
    /// file back finds the nearest theme it knows.
    #[test]
    fn each_style_returns_to_the_look_or_theme_it_last_had() {
        let mut settings = Settings {
            accent: Accent::Red,
            ..Settings::default()
        };
        assert_eq!(settings.style(), Style::Custom);
        settings.choose_chat_theme(ChatTheme::Zeus);
        assert!(settings.choose_style(Style::Themes));
        assert_eq!(settings.chat_theme, ChatTheme::Catppuccin);
        // A ready-made theme keeps its own accent; the chosen one waits.
        assert_eq!(
            settings.palette(true),
            ChatTheme::Catppuccin.palette(true, Accent::Theme)
        );
        settings.choose_chat_theme(ChatTheme::Dracula);
        assert!(!settings.choose_style(Style::Themes));
        assert!(settings.choose_style(Style::Custom));
        assert_eq!(settings.chat_theme, ChatTheme::Zeus);
        assert_eq!(settings.accent, Accent::Red);
        assert!(settings.choose_style(Style::Themes));
        assert_eq!(settings.chat_theme, ChatTheme::Dracula);
    }

    #[test]
    fn the_old_theme_menu_becomes_a_chat_theme_mode_and_accent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        for (old, chat_theme, mode, accent) in [
            ("dark", ChatTheme::WhatsApp, Mode::Dark, Accent::Theme),
            ("light", ChatTheme::WhatsApp, Mode::Light, Accent::Theme),
            ("system", ChatTheme::Zeus, Mode::System, Accent::Theme),
            ("nothingdark", ChatTheme::Zeus, Mode::Dark, Accent::Theme),
            ("nothinglight", ChatTheme::Zeus, Mode::Light, Accent::Theme),
            ("purple", ChatTheme::Zeus, Mode::Dark, Accent::Purple),
        ] {
            std::fs::write(&path, format!(r#"{{"version":1,"theme":"{old}"}}"#)).unwrap();
            let settings = Settings::load(&path);
            assert_eq!(
                (settings.chat_theme, settings.mode, settings.accent),
                (chat_theme, mode, accent),
                "{old}"
            );
            assert_eq!(settings.version, SETTINGS_VERSION);
            settings.save(&path).unwrap();
            let written: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            assert_eq!(written["theme"], old);
        }
        // A ported theme reads as Zeus's own in an older release, and comes
        // back as itself.
        let settings = Settings {
            chat_theme: ChatTheme::Dracula,
            mode: Mode::Light,
            blur: false,
            ..Settings::default()
        };
        settings.save(&path).unwrap();
        let written: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(written["theme"], "nothinglight");
        let loaded = Settings::load(&path);
        assert_eq!(loaded.chat_theme, ChatTheme::Dracula);
        assert_eq!(loaded.mode, Mode::Light);
        assert!(!loaded.blur);
        assert!(!loaded.vibrant_window());
    }

    #[test]
    fn unknown_and_missing_fields_are_tolerated() {
        let parsed: Settings =
            serde_json::from_str(r#"{"theme":"light","future_field":1}"#).expect("parses");
        assert_eq!(parsed.legacy_theme, ThemeChoice::Light);
        assert!(parsed.enter_sends);
        assert!(parsed.check_for_updates);
        assert!(!parsed.download_updates_automatically);
        assert!(parsed.show_wallpaper);
        assert_eq!(parsed.wallpaper_color, WallpaperColor::Theme);
        assert!(parsed.pause_other_media);
        assert_eq!(parsed.window_width, None);
        assert_eq!(parsed.window_height, None);
        assert_eq!(parsed.window_x, None);
        assert_eq!(parsed.window_y, None);
        assert!(!parsed.window_maximized);
    }

    #[test]
    fn the_window_geometry_defaults_to_unset() {
        // Files written before the window was remembered open with the
        // default size.
        let settings = Settings::default();
        assert_eq!(
            settings.window_geometry(),
            fastframe_shell::window::Geometry::default()
        );
        let older: Settings = serde_json::from_str(r#"{"zoom":1.25}"#).unwrap();
        assert_eq!(
            older.window_geometry(),
            fastframe_shell::window::Geometry::default()
        );
        // Half a size or position is no size or position.
        let half = Settings {
            window_width: Some(1280.0),
            window_x: Some(100.0),
            ..Settings::default()
        };
        assert_eq!(
            half.window_geometry(),
            fastframe_shell::window::Geometry::default()
        );
    }

    /// The geometry fastframe-shell remembers lands in the settings' own
    /// fields and comes back from a saved file unchanged.
    #[test]
    fn the_window_geometry_round_trips_through_the_settings_file() {
        let geometry = fastframe_shell::window::Geometry {
            size: Some([1280.0, 800.0]),
            position: Some([100.0, 50.0]),
            maximized: true,
        };
        let mut remembered = Settings {
            zoom: 1.25,
            ..Settings::default()
        };
        remembered.set_window_geometry(geometry);
        assert_eq!(
            (
                remembered.window_width,
                remembered.window_height,
                remembered.window_x,
                remembered.window_y,
                remembered.window_maximized
            ),
            (Some(1280.0), Some(800.0), Some(100.0), Some(50.0), true)
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        remembered.save(&path).unwrap();
        let loaded = Settings::load(&path);
        assert_eq!(loaded, remembered);
        assert_eq!(loaded.window_geometry(), geometry);

        // Without a position (Wayland) none is stored.
        remembered.set_window_geometry(fastframe_shell::window::Geometry {
            position: None,
            ..geometry
        });
        assert_eq!((remembered.window_x, remembered.window_y), (None, None));
    }

    fn load_from(contents: &str) -> (Settings, serde_json::Value) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, contents).unwrap();
        let settings = Settings::load(&path);
        settings.save(&path).unwrap();
        let stored = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        (settings, stored)
    }

    #[test]
    fn removed_settings_are_ignored_and_dropped_on_save() {
        let (settings, stored) = load_from(
            r#"{"enter_sends":false,"label_chips":true,"show_sender_pictures":true,
                "names_from_contacts":false,"collapse_chat_list":false,
                "save_contacts_to_phone":false,"keep_chat_list_position":true}"#,
        );
        assert!(!settings.enter_sends, "the rest of the file still loads");
        assert!(
            !settings.save_contacts_to_phone,
            "the old switch starts the new-contact box"
        );
        for key in [
            "label_chips",
            "show_sender_pictures",
            "names_from_contacts",
            "collapse_chat_list",
            "pause_media_while_recording",
            "pause_media_while_playing",
            "keep_chat_list_position",
        ] {
            assert!(stored.get(key).is_none(), "{key} is dropped on save");
        }
        assert_eq!(stored["pause_other_media"], true);
    }

    #[test]
    fn the_two_media_pause_switches_merge_into_one() {
        let merged = |contents: &str| {
            let (settings, stored) = load_from(contents);
            assert!(stored.get("pause_media_while_recording").is_none());
            assert!(stored.get("pause_media_while_playing").is_none());
            settings.pause_other_media
        };
        assert!(merged(
            r#"{"pause_media_while_recording":true,"pause_media_while_playing":true}"#
        ));
        assert!(!merged(
            r#"{"pause_media_while_recording":false,"pause_media_while_playing":true}"#
        ));
        assert!(!merged(
            r#"{"pause_media_while_recording":true,"pause_media_while_playing":false}"#
        ));
        assert!(!merged(r#"{"pause_media_while_playing":false}"#));
        assert!(merged(r#"{"pause_media_while_recording":true}"#));
        assert!(merged("{}"), "on by default");
        assert!(!merged(r#"{"pause_other_media":false}"#));
    }

    #[test]
    fn damaged_theme_cache_does_not_discard_other_settings() {
        let settings: Settings = serde_json::from_str(r#"{"custom_theme":"mine.json","custom_theme_cache":{"damaged":true},"enter_sends":false}"#).unwrap();
        assert!(!settings.enter_sends);
        assert!(settings.custom_theme_cache.is_none());
        assert_eq!(settings.custom_theme.as_deref(), Some("mine.json"));
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = std::env::temp_dir().join(format!("zapfast-settings-{}", std::process::id()));
        let path = dir.join("settings.json");
        let settings = Settings {
            zoom: 1.25,
            enter_sends: false,
            voice_speed: 1.5,
            pause_other_media: false,
            interface_language: Some(crate::i18n::Locale::German),
            message_sound: NotificationSound::None,
            mention_sound: NotificationSound::Custom("/sounds/ding.wav".into()),
            group_sounds: false,
            ..Settings::default()
        };
        settings.save(&path).expect("saves");
        assert_eq!(Settings::load(&path), settings);
        assert!(
            std::fs::read_to_string(&path)
                .expect("reads")
                .contains(r#""interface_language": "de""#),
            "the language keeps its stable serde name"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_locked_chat_verifier_is_salted_and_rejects_other_codes() {
        let mut settings = Settings::default();
        settings.set_chat_lock_code(Some(" 1234 "));
        assert!(settings.verifies_chat_lock_code("1234"));
        assert!(!settings.verifies_chat_lock_code("1235"));
        assert!(!settings.verifies_chat_lock_code(""));
        let first = settings.chat_lock_code_hash.clone();
        settings.set_chat_lock_code(Some("1234"));
        // A fresh salt every time, so the same code never stores the same value.
        assert_ne!(first, settings.chat_lock_code_hash);
        assert!(settings.verifies_chat_lock_code("1234"));
        settings.set_chat_lock_code(None);
        assert!(!settings.verifies_chat_lock_code("1234"));
    }

    #[test]
    fn legacy_locked_chat_code_is_rewritten_as_a_verifier() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{"chat_lock_code":"1234"}"#).unwrap();
        let settings = Settings::load(&path);
        assert!(settings.verifies_chat_lock_code("1234"));
        // The random hex verifier may contain "1234" by chance, so check
        // fields rather than searching the file for the digits.
        let stored: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert!(stored.get("chat_lock_code").is_none());
        let verifier = stored["chat_lock_code_hash"].as_str().unwrap();
        assert_ne!(verifier.trim(), "1234");
        assert!(verifier.contains('$'));
    }

    #[test]
    fn wallpaper_palette_contains_the_official_colours() {
        // WhatsApp's 28 light and 21 dark colours, after the theme's own.
        assert_eq!(WallpaperColor::LIGHT.len(), 29);
        assert_eq!(WallpaperColor::DARK.len(), 22);
        assert_eq!(WallpaperColor::LIGHT[0], WallpaperColor::Theme);
        assert_eq!(WallpaperColor::DARK[0], WallpaperColor::Theme);
        assert_eq!(WallpaperColor::Beige.rgb(), Some([245, 241, 235]));
        assert_eq!(WallpaperColor::Black.rgb(), Some([22, 23, 23]));
        assert_eq!(WallpaperColor::WillowBrook.label(), "Willow Brook");
    }

    #[test]
    fn the_theme_wallpaper_is_the_palettes_chat_colour() {
        use crate::theme::Palette;
        let settings = Settings::default();
        assert_eq!(settings.wallpaper_color, WallpaperColor::Theme);
        assert_eq!(settings.dark_wallpaper_color, WallpaperColor::Theme);
        let light = Palette::light();
        let dark = Palette::dark();
        assert_eq!(settings.wallpaper_background(&light), light.chat);
        assert_eq!(settings.wallpaper_background(&dark), dark.chat);
        let mut custom = Palette::dark();
        custom.chat = egui::Color32::from_rgb(30, 30, 46);
        assert_eq!(settings.wallpaper_background(&custom), custom.chat);
        // A fixed colour ignores the palette.
        let chosen = Settings {
            dark_wallpaper_color: WallpaperColor::Nordic,
            ..Settings::default()
        };
        assert_eq!(
            chosen.wallpaper_background(&custom),
            egui::Color32::from_rgb(15, 36, 36)
        );
        assert_eq!(chosen.wallpaper_background(&light), light.chat);
    }

    /// Saves handed to the file writer write the same files `save` does,
    /// once the writer is flushed.
    #[test]
    fn saves_through_the_writer_land_after_a_flush() {
        let dir = tempfile::tempdir().unwrap();
        let files = fastframe_file::Writer::new();
        let mut settings = Settings {
            check_for_updates: false,
            ..Settings::default()
        };
        settings.set_theme(ThemeChoice::Messages);
        let settings_path = dir.path().join("settings.json");
        settings.save_to(&files, &settings_path).unwrap();
        let account = AccountSettings {
            history_prefetch: true,
            ..AccountSettings::default()
        };
        let account_path = dir.path().join("accounts").join("2").join("settings.json");
        account.save_to(&files, &account_path).unwrap();
        let mut roster = AccountRoster::default();
        roster.allocate();
        let roster_path = dir.path().join("accounts.json");
        roster.save_to(&files, &roster_path).unwrap();

        assert!(files.flush(std::time::Duration::from_secs(10)));
        assert_eq!(Settings::load(&settings_path), settings);
        assert_eq!(AccountSettings::load(&account_path), account);
        assert_eq!(AccountRoster::load(&roster_path).unwrap(), roster);
    }

    #[test]
    fn the_old_default_wallpaper_colours_become_the_theme_once() {
        let (settings, stored) =
            load_from(r#"{"wallpaper_color":"beige","dark_wallpaper_color":"black"}"#);
        assert_eq!(settings.wallpaper_color, WallpaperColor::Theme);
        assert_eq!(settings.dark_wallpaper_color, WallpaperColor::Theme);
        assert_eq!(stored["version"], SETTINGS_VERSION);
        assert_eq!(stored["wallpaper_color"], "theme");

        // Chosen colours survive the migration.
        let (settings, _) =
            load_from(r#"{"wallpaper_color":"cruise","dark_wallpaper_color":"nordic"}"#);
        assert_eq!(settings.wallpaper_color, WallpaperColor::Cruise);
        assert_eq!(settings.dark_wallpaper_color, WallpaperColor::Nordic);
        let (settings, _) =
            load_from(r#"{"wallpaper_color":"beige","dark_wallpaper_color":"tiber"}"#);
        assert_eq!(settings.wallpaper_color, WallpaperColor::Theme);
        assert_eq!(settings.dark_wallpaper_color, WallpaperColor::Tiber);

        // Once migrated, choosing Beige or Black again sticks.
        let (settings, _) =
            load_from(r#"{"version":1,"wallpaper_color":"beige","dark_wallpaper_color":"black"}"#);
        assert_eq!(settings.wallpaper_color, WallpaperColor::Beige);
        assert_eq!(settings.dark_wallpaper_color, WallpaperColor::Black);

        // A current file round-trips unchanged, image path included.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let chosen = Settings {
            wallpaper_color: WallpaperColor::Beige,
            dark_wallpaper_color: WallpaperColor::Black,
            wallpaper_image: Some(dir.path().join("wallpaper.png")),
            ..Settings::default()
        };
        chosen.save(&path).unwrap();
        assert_eq!(Settings::load(&path), chosen);
        assert_eq!(
            Settings::load(&path),
            chosen,
            "loading again changes nothing"
        );
    }

    #[test]
    fn a_hand_edited_account_id_cannot_leave_the_accounts_folder() {
        use crate::model::AccountId;
        assert!(AccountId::is_safe("1"));
        assert!(AccountId::is_safe("12"));
        assert!(!AccountId::is_safe(""));
        assert!(!AccountId::is_safe("0"));
        assert!(!AccountId::is_safe("01"));
        assert!(!AccountId::is_safe(".."));
        assert!(!AccountId::is_safe("/tmp"));
        assert!(!AccountId::is_safe("1/../etc"));
    }

    #[test]
    fn a_roster_drops_unsafe_ids() {
        let mut roster = AccountRoster {
            next_id: 4,
            active: "../etc".into(),
            order: vec!["1".into(), "..".into(), "/tmp".into()],
        };
        roster.sanitize();
        assert_eq!(roster.order, ["1".to_string()]);
        assert_eq!(roster.active, "1");
        assert!(roster.next_id > 1);
    }

    #[test]
    fn a_roster_keeps_the_first_copy_of_a_repeated_id() {
        let mut roster = AccountRoster {
            next_id: 3,
            active: "1".into(),
            order: vec!["1".into(), "1".into(), "2".into()],
        };
        roster.sanitize();
        assert_eq!(roster.order, ["1".to_string(), "2".to_string()]);
    }

    #[test]
    fn a_broken_roster_file_does_not_become_the_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("accounts.json");
        std::fs::write(&path, "{").unwrap();
        assert!(AccountRoster::load(&path).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{");
    }

    #[test]
    fn allocating_an_account_advances_the_roster() {
        let mut roster = AccountRoster::default();
        assert_eq!(roster.order, ["1".to_string()]);
        let second = roster.allocate();
        assert_eq!(second.as_str(), "2");
        assert_eq!(roster.next_id, 3);
        assert_eq!(roster.active, "2");
        assert_eq!(roster.order, ["1".to_string(), "2".to_string()]);
    }

    #[test]
    fn a_missing_roster_file_starts_with_the_first_account() {
        let dir = tempfile::tempdir().unwrap();
        let roster = AccountRoster::load(&dir.path().join("accounts.json")).unwrap();
        assert_eq!(roster, AccountRoster::default());
    }
}

#[cfg(test)]
mod giphy_tests {
    use super::*;

    #[test]
    fn the_users_key_wins_and_is_trimmed() {
        let settings = Settings {
            giphy_key: "  abc  ".into(),
            ..Settings::default()
        };
        assert_eq!(settings.effective_giphy_key().as_deref(), Some("abc"));
    }

    #[test]
    fn without_a_key_of_their_own_the_built_in_one_is_used() {
        let settings = Settings {
            giphy_key: "   ".into(),
            ..Settings::default()
        };
        let expected = BUILT_IN_GIPHY_KEY
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .map(str::to_owned);
        assert_eq!(settings.effective_giphy_key(), expected);
    }

    #[test]
    fn notification_limits_are_off_unless_saved() {
        let settings: AccountSettings = serde_json::from_str(r#"{"notifications":true}"#).unwrap();
        assert_eq!(settings.limit_notifications, NotificationLimit::Off);
        let saved = AccountSettings {
            limit_notifications: NotificationLimit::Group,
            ..AccountSettings::default()
        };
        let again: AccountSettings =
            serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap();
        assert_eq!(again, saved);
    }

    #[test]
    fn a_notification_limit_names_the_chats_it_applies_to() {
        for (limit, direct, group) in [
            (NotificationLimit::Off, false, false),
            (NotificationLimit::Direct, true, false),
            (NotificationLimit::Group, false, true),
            (NotificationLimit::Both, true, true),
        ] {
            assert_eq!(limit.applies(false), direct, "{limit:?} for direct chats");
            assert_eq!(limit.applies(true), group, "{limit:?} for groups");
        }
    }
}
