//! The chat themes Settings offers: Zeus's own, WhatsApp's and Messages',
//! and the named palettes ported from Agent Orchestrator
//! (github.com/OrchestratorInc/agent-orchestrator,
//! `packages/shared/theme/palettes.ts`, Apache-2.0). Each comes in a dark and
//! a light version; the mode and an optional accent are chosen beside it.

use egui::Color32;
use serde::{Deserialize, Serialize};

use crate::theme::{Palette, blend};

/// The family of colours the window is drawn in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChatTheme {
    /// Nothing OS inspired monochrome glass, Zeus's own look.
    Zeus,
    /// WhatsApp's own colours.
    #[default]
    WhatsApp,
    /// macOS's greys with bubbles shaped as Messages draws them.
    Messages,
    Orchestrate,
    Vercel,
    #[serde(rename = "github")]
    GitHub,
    One,
    Catppuccin,
    TokyoNight,
    Dracula,
    Nord,
    Gruvbox,
    RosePine,
    Solarized,
    Kanagawa,
    #[serde(rename = "vscode")]
    VsCode,
    Monokai,
    Material,
    Ayu,
    NightOwl,
    Everforest,
    Vitesse,
    Flexoki,
}

impl ChatTheme {
    /// Every theme, in the order Settings shows them.
    pub const ALL: [ChatTheme; 23] = [
        Self::Zeus,
        Self::WhatsApp,
        Self::Messages,
        Self::Orchestrate,
        Self::Vercel,
        Self::GitHub,
        Self::One,
        Self::Catppuccin,
        Self::TokyoNight,
        Self::Dracula,
        Self::Nord,
        Self::Gruvbox,
        Self::RosePine,
        Self::Solarized,
        Self::Kanagawa,
        Self::VsCode,
        Self::Monokai,
        Self::Material,
        Self::Ayu,
        Self::NightOwl,
        Self::Everforest,
        Self::Vitesse,
        Self::Flexoki,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Zeus => "Zeus",
            Self::WhatsApp => "WhatsApp",
            Self::Messages => "Messages",
            Self::Orchestrate => "Orchestrate",
            Self::Vercel => "Vercel",
            Self::GitHub => "GitHub",
            Self::One => "One",
            Self::Catppuccin => "Catppuccin",
            Self::TokyoNight => "Tokyo Night",
            Self::Dracula => "Dracula",
            Self::Nord => "Nord",
            Self::Gruvbox => "Gruvbox",
            Self::RosePine => "Rosé Pine",
            Self::Solarized => "Solarized",
            Self::Kanagawa => "Kanagawa",
            Self::VsCode => "VS Code",
            Self::Monokai => "Monokai",
            Self::Material => "Material",
            Self::Ayu => "Ayu",
            Self::NightOwl => "Night Owl",
            Self::Everforest => "Everforest",
            Self::Vitesse => "Vitesse",
            Self::Flexoki => "Flexoki",
        }
    }

    /// Whether the theme draws glass: a frosted header and composer, and
    /// where the platform can, the desktop through the window. Every theme
    /// but WhatsApp's.
    pub fn vibrant(self) -> bool {
        self != Self::WhatsApp
    }

    /// Whether an accent chosen in Settings applies. Messages keeps the
    /// system blue its bubbles are drawn in.
    pub fn takes_accent(self) -> bool {
        self != Self::Messages
    }

    /// The theme's palette in the dark or light mode, with `accent` over its
    /// own where it takes one.
    pub fn palette(self, dark: bool, accent: Accent) -> Palette {
        let base = match self {
            Self::Zeus if dark => Palette::nothing_dark(),
            Self::Zeus => Palette::nothing_light(),
            Self::WhatsApp if dark => Palette::dark(),
            Self::WhatsApp => Palette::light(),
            Self::Messages if dark => Palette::messages(),
            Self::Messages => Palette::messages_light(),
            ported => {
                let definition = DEFINITIONS
                    .iter()
                    .find(|definition| definition.theme == ported)
                    .expect("every ported theme has a definition");
                let source = if dark {
                    &definition.dark
                } else {
                    &definition.light
                };
                source.palette(dark)
            }
        };
        match accent.rgb() {
            Some(rgb) if self.takes_accent() => base.with_accent(rgb),
            _ => base,
        }
    }
}

/// Whether the window is light or dark.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// As the operating system is.
    System,
    Light,
    #[default]
    Dark,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Self::System, Self::Light, Self::Dark];

    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }
}

/// A colour for buttons, links, ticks and your own bubbles, over the
/// theme's own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Accent {
    /// The theme's own accent.
    #[default]
    Theme,
    Red,
    Orange,
    Yellow,
    Green,
    Teal,
    Blue,
    Purple,
    Pink,
}

impl Accent {
    pub const ALL: [Accent; 9] = [
        Self::Theme,
        Self::Red,
        Self::Orange,
        Self::Yellow,
        Self::Green,
        Self::Teal,
        Self::Blue,
        Self::Purple,
        Self::Pink,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Theme => "Theme",
            Self::Red => "Red",
            Self::Orange => "Orange",
            Self::Yellow => "Yellow",
            Self::Green => "Green",
            Self::Teal => "Teal",
            Self::Blue => "Blue",
            Self::Purple => "Purple",
            Self::Pink => "Pink",
        }
    }

    /// The accent as RGB, the system colours of macOS in Dark Aqua; `None`
    /// keeps the theme's.
    pub fn rgb(self) -> Option<[u8; 3]> {
        match self {
            Self::Theme => None,
            Self::Red => Some([0xff, 0x45, 0x3a]),
            Self::Orange => Some([0xff, 0x9f, 0x0a]),
            Self::Yellow => Some([0xff, 0xd6, 0x0a]),
            Self::Green => Some([0x30, 0xd1, 0x58]),
            Self::Teal => Some([0x40, 0xc8, 0xe0]),
            Self::Blue => Some([0x0a, 0x84, 0xff]),
            Self::Purple => Some([0xbf, 0x5a, 0xf2]),
            Self::Pink => Some([0xff, 0x37, 0x8f]),
        }
    }
}

/// One mode of a ported theme, in the roles Agent Orchestrator names.
struct Source {
    /// Main canvas.
    bg: u32,
    /// The rail, a step darker than `bg` in dark mode.
    sidebar: u32,
    /// Raised cards.
    panel: u32,
    /// Hover, selected and input surfaces.
    element: u32,
    text: u32,
    /// Secondary text, readable (WCAG AA) on `bg`.
    muted: u32,
    accent: u32,
    red: u32,
    yellow: u32,
}

fn rgb(value: u32) -> Color32 {
    let [_, r, g, b] = value.to_be_bytes();
    Color32::from_rgb(r, g, b)
}

impl Source {
    /// Zeus's glass look in the theme's colours: the rail and window in its
    /// darkest step, the chat list on its canvas, the conversation between
    /// the two, and others' bubbles on its element colour (its raised card in
    /// light mode, where that is the lightest).
    fn palette(&self, dark: bool) -> Palette {
        let mut p = if dark {
            Palette::nothing_dark()
        } else {
            Palette::nothing_light()
        };
        let [bg, sidebar, panel, element, text, muted] = [
            self.bg,
            self.sidebar,
            self.panel,
            self.element,
            self.text,
            self.muted,
        ]
        .map(rgb);
        p.text = text;
        p.secondary = muted;
        p.dim = muted;
        p.surface = element;
        p.surface_hover = blend(element, text, 0.07);
        p.surface_active = blend(element, text, 0.13);
        p.outline = blend(element, text, 0.05);
        p.danger = rgb(self.red);
        p.warning = rgb(self.yellow);
        if dark {
            p.window = sidebar;
            p.panel = bg;
            p.chat = blend(bg, sidebar, 0.5);
            p.overlay = panel;
            p.bubble_in = element;
        } else {
            p.window = bg;
            p.panel = panel;
            p.chat = blend(bg, element, 0.4);
            p.overlay = panel;
            // Halfway from its card to white, so a theme whose card is its
            // canvas still sets others' bubbles off the conversation.
            p.bubble_in = blend(panel, Color32::WHITE, 0.5);
        }
        let [r, g, b, _] = rgb(self.accent).to_array();
        p.with_accent([r, g, b])
    }
}

/// A ported theme: its dark and light palettes.
struct Definition {
    theme: ChatTheme,
    dark: Source,
    light: Source,
}

/// Agent Orchestrator's themes, as its `THEMES` lists them.
#[rustfmt::skip]
const DEFINITIONS: [Definition; 20] = [
    Definition {
        theme: ChatTheme::Orchestrate,
        dark: Source { bg: 0x181819, sidebar: 0x111212, panel: 0x212223, element: 0x2c2d2d, text: 0xf4f5f6, muted: 0xa4a5a6, accent: 0x79b0dc, red: 0xff6467, yellow: 0xfacc15 },
        light: Source { bg: 0xf5f5f5, sidebar: 0xeeeeee, panel: 0xffffff, element: 0xe9e9e9, text: 0x121212, muted: 0x636363, accent: 0x304c83, red: 0xe7000b, yellow: 0xca8a04 },
    },
    Definition {
        theme: ChatTheme::Vercel,
        dark: Source { bg: 0x0a0a0a, sidebar: 0x000000, panel: 0x111111, element: 0x1f1f1f, text: 0xededed, muted: 0xa1a1a1, accent: 0x52a8ff, red: 0xff6166, yellow: 0xffb224 },
        light: Source { bg: 0xffffff, sidebar: 0xfafafa, panel: 0xfafafa, element: 0xebebeb, text: 0x171717, muted: 0x666666, accent: 0x0068d6, red: 0xcb2a2f, yellow: 0xa35200 },
    },
    Definition {
        theme: ChatTheme::GitHub,
        dark: Source { bg: 0x0d1117, sidebar: 0x010409, panel: 0x151b23, element: 0x262c36, text: 0xe6edf3, muted: 0x9198a1, accent: 0x4493f8, red: 0xf85149, yellow: 0xd29922 },
        light: Source { bg: 0xffffff, sidebar: 0xf6f8fa, panel: 0xf6f8fa, element: 0xeaeef2, text: 0x1f2328, muted: 0x59636e, accent: 0x0969da, red: 0xd1242f, yellow: 0x9a6700 },
    },
    Definition {
        theme: ChatTheme::One,
        dark: Source { bg: 0x282c34, sidebar: 0x21252b, panel: 0x2c313a, element: 0x3a3f4b, text: 0xc8ccd4, muted: 0x9da5b4, accent: 0x61afef, red: 0xe06c75, yellow: 0xe5c07b },
        light: Source { bg: 0xfafafa, sidebar: 0xeaeaeb, panel: 0xf2f2f2, element: 0xe5e5e6, text: 0x383a42, muted: 0x696c77, accent: 0x4078f2, red: 0xe45649, yellow: 0x986801 },
    },
    Definition {
        theme: ChatTheme::Catppuccin,
        dark: Source { bg: 0x1e1e2e, sidebar: 0x181825, panel: 0x24243a, element: 0x313244, text: 0xcdd6f4, muted: 0xa6adc8, accent: 0xcba6f7, red: 0xf38ba8, yellow: 0xf9e2af },
        light: Source { bg: 0xeff1f5, sidebar: 0xe6e9ef, panel: 0xf5f6f9, element: 0xdce0e8, text: 0x474a63, muted: 0x696c81, accent: 0x8839ef, red: 0xd20f39, yellow: 0xc77800 },
    },
    Definition {
        theme: ChatTheme::TokyoNight,
        dark: Source { bg: 0x1a1b26, sidebar: 0x16161e, panel: 0x1f2335, element: 0x292e42, text: 0xc0caf5, muted: 0x9aa5ce, accent: 0x7aa2f7, red: 0xf7768e, yellow: 0xe0af68 },
        light: Source { bg: 0xe1e2e7, sidebar: 0xd5d6db, panel: 0xe9e9ed, element: 0xc4c8da, text: 0x153894, muted: 0x50609d, accent: 0x2e7de9, red: 0xf42864, yellow: 0x8c6c3e },
    },
    Definition {
        theme: ChatTheme::Dracula,
        dark: Source { bg: 0x282a36, sidebar: 0x21222c, panel: 0x2e303e, element: 0x44475a, text: 0xf8f8f2, muted: 0xa7acc8, accent: 0xbd93f9, red: 0xff5555, yellow: 0xf1fa8c },
        light: Source { bg: 0xfffbeb, sidebar: 0xf5f0dc, panel: 0xfffdf4, element: 0xecead8, text: 0x1f1f1f, muted: 0x6c664b, accent: 0x644ac9, red: 0xcb3a2a, yellow: 0x846e15 },
    },
    Definition {
        theme: ChatTheme::Nord,
        dark: Source { bg: 0x2e3440, sidebar: 0x272c36, panel: 0x343a47, element: 0x3b4252, text: 0xeceff4, muted: 0xa5afc2, accent: 0x88c0d0, red: 0xbf616a, yellow: 0xebcb8b },
        light: Source { bg: 0xeceff4, sidebar: 0xe5e9f0, panel: 0xf4f6f9, element: 0xd8dee9, text: 0x2e3440, muted: 0x4c566a, accent: 0x5e81ac, red: 0xa5434c, yellow: 0x8a6d1f },
    },
    Definition {
        theme: ChatTheme::Gruvbox,
        dark: Source { bg: 0x282828, sidebar: 0x1d2021, panel: 0x32302f, element: 0x3c3836, text: 0xebdbb2, muted: 0xa89984, accent: 0x83a598, red: 0xfb4934, yellow: 0xfabd2f },
        light: Source { bg: 0xfbf1c7, sidebar: 0xf2e5bc, panel: 0xf9f5d7, element: 0xebdbb2, text: 0x3c3836, muted: 0x776a5f, accent: 0x076678, red: 0x9d0006, yellow: 0xb57614 },
    },
    Definition {
        theme: ChatTheme::RosePine,
        dark: Source { bg: 0x191724, sidebar: 0x15131f, panel: 0x1f1d2e, element: 0x26233a, text: 0xe0def4, muted: 0x908caa, accent: 0xc4a7e7, red: 0xeb6f92, yellow: 0xf6c177 },
        light: Source { bg: 0xfaf4ed, sidebar: 0xf4ede8, panel: 0xfffaf3, element: 0xf2e9e1, text: 0x4f4a71, muted: 0x706c89, accent: 0x907aa9, red: 0xb4637a, yellow: 0xc77d00 },
    },
    Definition {
        theme: ChatTheme::Solarized,
        dark: Source { bg: 0x002b36, sidebar: 0x00212b, panel: 0x04313c, element: 0x073642, text: 0xc5cdcd, muted: 0x93a1a1, accent: 0x268bd2, red: 0xdc322f, yellow: 0xb58900 },
        light: Source { bg: 0xfdf6e3, sidebar: 0xeee8d5, panel: 0xfefaf0, element: 0xeee8d5, text: 0x073642, muted: 0x586e75, accent: 0x268bd2, red: 0xdc322f, yellow: 0xb28700 },
    },
    Definition {
        theme: ChatTheme::Kanagawa,
        dark: Source { bg: 0x1f1f28, sidebar: 0x16161d, panel: 0x2a2a37, element: 0x363646, text: 0xdcd7ba, muted: 0xa6a69c, accent: 0x7e9cd8, red: 0xe46876, yellow: 0xe6c384 },
        light: Source { bg: 0xf2ecbc, sidebar: 0xe7dba0, panel: 0xf5f0cc, element: 0xe5ddb0, text: 0x424251, muted: 0x6c695c, accent: 0x4d699b, red: 0xc84053, yellow: 0x77713f },
    },
    Definition {
        theme: ChatTheme::VsCode,
        dark: Source { bg: 0x1f1f1f, sidebar: 0x181818, panel: 0x252526, element: 0x313131, text: 0xcccccc, muted: 0x9d9d9d, accent: 0x4daafc, red: 0xf85149, yellow: 0xcca700 },
        light: Source { bg: 0xffffff, sidebar: 0xf8f8f8, panel: 0xf8f8f8, element: 0xe5e5e5, text: 0x3b3b3b, muted: 0x6e6e6e, accent: 0x005fb8, red: 0xcd3131, yellow: 0x895503 },
    },
    Definition {
        theme: ChatTheme::Monokai,
        dark: Source { bg: 0x2d2a2e, sidebar: 0x221f22, panel: 0x353236, element: 0x403e41, text: 0xfcfcfa, muted: 0xa9a8a9, accent: 0xffd866, red: 0xff6188, yellow: 0xffd866 },
        light: Source { bg: 0xfaf4f2, sidebar: 0xede7e5, panel: 0xfdf9f8, element: 0xe6e0de, text: 0x29242a, muted: 0x706b6e, accent: 0xcc7a0a, red: 0xe14775, yellow: 0xcc7a0a },
    },
    Definition {
        theme: ChatTheme::Material,
        dark: Source { bg: 0x212121, sidebar: 0x1a1a1a, panel: 0x292929, element: 0x323232, text: 0xeeffff, muted: 0xb0bec5, accent: 0x80cbc4, red: 0xf07178, yellow: 0xffcb6b },
        light: Source { bg: 0xfafafa, sidebar: 0xeeeeee, panel: 0xffffff, element: 0xe7e7e8, text: 0x37474f, muted: 0x546e7a, accent: 0x00897b, red: 0xe53935, yellow: 0xcd7e00 },
    },
    Definition {
        theme: ChatTheme::Ayu,
        dark: Source { bg: 0x1f2430, sidebar: 0x1c212b, panel: 0x232834, element: 0x2d3443, text: 0xcccac2, muted: 0x959ca8, accent: 0xffcc66, red: 0xf28779, yellow: 0xffd173 },
        light: Source { bg: 0xfcfcfc, sidebar: 0xf3f4f5, panel: 0xffffff, element: 0xe7eaed, text: 0x4e5357, muted: 0x717479, accent: 0xdb780d, red: 0xe65050, yellow: 0xc68409 },
    },
    Definition {
        theme: ChatTheme::NightOwl,
        dark: Source { bg: 0x011627, sidebar: 0x01111d, panel: 0x0b2942, element: 0x1d3b53, text: 0xd6deeb, muted: 0x8ca2b8, accent: 0x82aaff, red: 0xef5350, yellow: 0xecc48d },
        light: Source { bg: 0xfbfbfb, sidebar: 0xf0f0f0, panel: 0xffffff, element: 0xe0e7ea, text: 0x403f53, muted: 0x6a7179, accent: 0x4876d6, red: 0xde3d3b, yellow: 0xb98900 },
    },
    Definition {
        theme: ChatTheme::Everforest,
        dark: Source { bg: 0x2d353b, sidebar: 0x232a2e, panel: 0x343f44, element: 0x3d484d, text: 0xddd0b4, muted: 0x9da9a0, accent: 0xa7c080, red: 0xe67e80, yellow: 0xdbbc7f },
        light: Source { bg: 0xfdf6e3, sidebar: 0xefebd4, panel: 0xf4f0d9, element: 0xe6e2cc, text: 0x414e56, muted: 0x667465, accent: 0x849700, red: 0xf85552, yellow: 0xbf8200 },
    },
    Definition {
        theme: ChatTheme::Vitesse,
        dark: Source { bg: 0x121212, sidebar: 0x0d0d0d, panel: 0x181818, element: 0x232323, text: 0xdbd7ca, muted: 0x959da5, accent: 0x4d9375, red: 0xcb7676, yellow: 0xe6cc77 },
        light: Source { bg: 0xffffff, sidebar: 0xf7f7f7, panel: 0xfafafa, element: 0xeeeeee, text: 0x393a34, muted: 0x6b6b6b, accent: 0x1c6b48, red: 0xab5959, yellow: 0x8c6a10 },
    },
    Definition {
        theme: ChatTheme::Flexoki,
        dark: Source { bg: 0x100f0f, sidebar: 0x0a0909, panel: 0x1c1b1a, element: 0x282726, text: 0xcecdc3, muted: 0x878580, accent: 0xda702c, red: 0xd14d41, yellow: 0xd0a215 },
        light: Source { bg: 0xfffcf0, sidebar: 0xf2f0e5, panel: 0xfffefa, element: 0xe6e4d9, text: 0x100f0f, muted: 0x6f6e69, accent: 0xbc5215, red: 0xaf3029, yellow: 0xad8301 },
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{BubbleStyle, contrast};

    /// Every theme in both modes keeps text readable where it is drawn:
    /// body text on the conversation, the list and both kinds of bubble,
    /// secondary text on the list, and the label on an accent button.
    #[test]
    fn every_theme_keeps_its_text_readable() {
        for theme in ChatTheme::ALL {
            for dark in [true, false] {
                for accent in Accent::ALL {
                    let p = theme.palette(dark, accent);
                    let name = format!("{} {dark} {accent:?}", theme.label());
                    assert_eq!(p.dark, dark, "{name}");
                    assert!(contrast(p.text, p.panel) >= 4.5, "{name}: text on panel");
                    assert!(
                        contrast(p.text, p.bubble_in) >= 4.5,
                        "{name}: text on bubble"
                    );
                    // Messages draws white on its own blue, shading down the
                    // window, as the Mac does.
                    let own = p.on_bubble(true);
                    assert!(
                        theme == ChatTheme::Messages || contrast(own.text, p.bubble_out) >= 4.5,
                        "{name}: text on own bubble"
                    );
                    assert!(contrast(p.secondary, p.panel) >= 3.0, "{name}: secondary");
                    assert!(
                        contrast(p.on_accent, p.accent) >= 3.0,
                        "{name}: label on accent"
                    );
                }
            }
        }
    }

    /// The old dark colour themes look as they did.
    #[test]
    fn an_accent_on_zeus_dark_is_the_old_colour_theme() {
        let p = ChatTheme::Zeus.palette(true, Accent::Red);
        assert_eq!(p.accent, Color32::from_rgb(0xff, 0x45, 0x3a));
        assert_eq!(p, Palette::nothing_tinted([0xff, 0x45, 0x3a]));
    }

    #[test]
    fn messages_keeps_its_blue() {
        assert_eq!(
            ChatTheme::Messages.palette(true, Accent::Pink),
            Palette::messages()
        );
    }

    #[test]
    fn ported_themes_keep_their_canvas_and_accent() {
        let p = ChatTheme::Dracula.palette(true, Accent::Theme);
        assert_eq!(p.panel, Color32::from_rgb(0x28, 0x2a, 0x36));
        assert_eq!(p.accent, Color32::from_rgb(0xbd, 0x93, 0xf9));
        assert_eq!(p.bubbles, BubbleStyle::Nothing);
        let names: std::collections::HashSet<_> =
            ChatTheme::ALL.iter().map(|theme| theme.label()).collect();
        assert_eq!(names.len(), ChatTheme::ALL.len());
    }

    #[test]
    fn choices_are_stored_by_name() {
        assert_eq!(
            serde_json::to_string(&ChatTheme::TokyoNight).unwrap(),
            "\"tokyo-night\""
        );
        assert_eq!(
            serde_json::to_string(&ChatTheme::VsCode).unwrap(),
            "\"vscode\""
        );
        assert_eq!(serde_json::to_string(&Accent::Teal).unwrap(), "\"teal\"");
    }

    #[test]
    fn ported_light_themes_separate_bubbles_from_the_conversation() {
        for theme in &ChatTheme::ALL[3..] {
            let p = theme.palette(false, Accent::Theme);
            assert_ne!(p.bubble_in, p.chat, "{}", theme.label());
        }
    }
}
