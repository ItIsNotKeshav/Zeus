//! Where the platform keeps its colour emoji font.
//!
//! - macOS: Apple Color Emoji, at the same path since OS X 10.7.
//! - Linux and other Unix: fontconfig's answer for `emoji:color=true`,
//!   asked through `fc-match` as `fastframe-text` asks for hinting, else
//!   the first colour bitmap face a scan of the font directories finds,
//!   in the order fontconfig's own configuration prefers emoji families.
//! - Windows: Segoe UI Emoji is drawn through DirectWrite (`windows.rs`),
//!   not from its file.

use std::path::{Path, PathBuf};

use crate::bitmap::{BitmapFont, Bytes};

/// Maps a font file read-only.
#[cfg_attr(
    windows,
    allow(
        dead_code,
        reason = "Windows draws its own emoji font through DirectWrite"
    )
)]
#[allow(
    unsafe_code,
    reason = "memory mapping is the only way to use a 190 MB font without reading it whole"
)]
pub(crate) fn map(path: &Path) -> Option<memmap2::Mmap> {
    let file = std::fs::File::open(path).ok()?;
    // SAFETY: the mapping is read-only. A font file rewritten underneath it
    // could fault, the same bet every font renderer on the platform makes;
    // package managers replace font files by renaming new ones into place,
    // which leaves this mapping on the old file.
    unsafe { memmap2::Mmap::map(&file) }.ok()
}

/// Opens a colour bitmap face from a file, logging why one is refused.
#[cfg_attr(
    windows,
    allow(
        dead_code,
        reason = "Windows draws its own emoji font through DirectWrite"
    )
)]
pub(crate) fn open(path: &Path, index: u32) -> Option<BitmapFont> {
    let map = map(path)?;
    match BitmapFont::new(Bytes::Mapped(map), index, path.display().to_string()) {
        Ok(font) => Some(font),
        Err(reason) => {
            log::debug!("{} is not a colour emoji font: {reason}", path.display());
            None
        }
    }
}

/// The system's colour bitmap emoji font, if it has one this can draw.
#[cfg(target_os = "macos")]
pub(crate) fn bitmap_font() -> Option<BitmapFont> {
    open(Path::new("/System/Library/Fonts/Apple Color Emoji.ttc"), 0)
}

/// Windows' own emoji font is drawn by DirectWrite instead.
#[cfg(windows)]
pub(crate) fn bitmap_font() -> Option<BitmapFont> {
    None
}

/// The system's colour bitmap emoji font, if it has one this can draw.
#[cfg(not(any(target_os = "macos", windows)))]
pub(crate) fn bitmap_font() -> Option<BitmapFont> {
    if let Some((path, index)) = ask_fontconfig()
        && let Some(font) = open(&path, index)
    {
        return Some(font);
    }
    let mut found = Vec::new();
    for dir in fastframe_fonts::system::font_directories() {
        collect(&dir, 0, &mut found);
    }
    rank(&mut found);
    found.iter().find_map(|path| open(path, 0))
}

/// fontconfig's colour emoji face: `fc-match` answers with the desktop's
/// configured choice, installed or not in the usual places.
#[cfg(not(any(target_os = "macos", windows)))]
fn ask_fontconfig() -> Option<(PathBuf, u32)> {
    let output = std::process::Command::new("fc-match")
        .args(["-f", FC_FORMAT, "emoji:color=true"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_fc_match(&String::from_utf8_lossy(&output.stdout))
}

/// The `fc-match` format [`parse_fc_match`] reads.
#[cfg_attr(any(target_os = "macos", windows), allow(dead_code))]
const FC_FORMAT: &str = "%{color}|%{index}|%{file}";

/// The file and face of `fc-match` output in [`FC_FORMAT`], when the face
/// is a colour one (fontconfig falls back to any face when none is).
#[cfg_attr(any(target_os = "macos", windows), allow(dead_code))]
fn parse_fc_match(output: &str) -> Option<(PathBuf, u32)> {
    let mut fields = output.lines().next()?.splitn(3, '|');
    let colour = fields.next()?.trim();
    let index: u32 = fields.next()?.trim().parse().ok()?;
    let file = fields.next()?.trim();
    if !colour.eq_ignore_ascii_case("true") || file.is_empty() {
        return None;
    }
    // A named instance of a variable face sets the upper bits.
    Some((PathBuf::from(file), index & 0xFFFF))
}

/// How deep to walk a font directory, as `fastframe-fonts` does.
const SCAN_DEPTH: usize = 4;

/// Collects font files whose names say they hold emoji. Opening every font
/// to look would cost a tenth of a second; emoji fonts name themselves.
#[cfg_attr(any(target_os = "macos", windows), allow(dead_code))]
fn collect(dir: &Path, depth: usize, found: &mut Vec<PathBuf>) {
    if depth >= SCAN_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() || (kind.is_symlink() && path.is_dir()) {
            collect(&path, depth + 1, found);
        } else if names_emoji(&path) && !found.contains(&path) {
            found.push(path);
        }
    }
}

/// Emoji font families in the order fontconfig's `60-generic.conf` prefers
/// them, as they appear in file names.
const PREFERRED: &[&str] = &[
    "notocoloremoji",
    "applecoloremoji",
    "twemoji",
    "twittercoloremoji",
    "emojionemozilla",
    "emojitwo",
    "joypixels",
    "emojione",
];

/// Whether a file name says it is an emoji font.
#[cfg_attr(any(target_os = "macos", windows), allow(dead_code))]
fn names_emoji(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let name = name.to_ascii_lowercase();
    let font = [".ttf", ".otf", ".ttc"]
        .iter()
        .any(|extension| name.ends_with(extension));
    font && (name.contains("emoji") || name.contains("joypixels"))
}

/// Orders emoji fonts by fontconfig's preference, then by path so two
/// machines with the same fonts choose the same one.
#[cfg_attr(any(target_os = "macos", windows), allow(dead_code))]
fn rank(found: &mut [PathBuf]) {
    let key = |path: &PathBuf| {
        let name: String = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .collect();
        let preference = PREFERRED
            .iter()
            .position(|family| name.starts_with(family))
            .unwrap_or(PREFERRED.len());
        (preference, path.clone())
    };
    found.sort_by_key(key);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fontconfig_names_a_colour_face_or_nothing() {
        assert_eq!(
            parse_fc_match("True|0|/usr/share/fonts/noto/NotoColorEmoji.ttf"),
            Some((PathBuf::from("/usr/share/fonts/noto/NotoColorEmoji.ttf"), 0))
        );
        assert_eq!(
            parse_fc_match("True|327681|/x/Emoji|Pipe.ttc\n"),
            Some((PathBuf::from("/x/Emoji|Pipe.ttc"), 1)),
            "the named instance bits are dropped and the path kept whole"
        );
        assert_eq!(
            parse_fc_match("False|0|/usr/share/fonts/TTF/DejaVuSans.ttf"),
            None,
            "fontconfig's fallback for a missing emoji font is not one"
        );
        assert_eq!(parse_fc_match(""), None);
        assert_eq!(parse_fc_match("True|x|/a.ttf"), None);
    }

    #[test]
    fn emoji_fonts_are_found_by_name_and_ranked_like_fontconfig() {
        assert!(names_emoji(Path::new("/f/NotoColorEmoji.ttf")));
        assert!(names_emoji(Path::new("/f/JoyPixels.TTF")));
        assert!(names_emoji(Path::new("/f/Twemoji.otf")));
        assert!(!names_emoji(Path::new("/f/NotoSans-Regular.ttf")));
        assert!(!names_emoji(Path::new("/f/emoji.txt")));
        let mut found = vec![
            PathBuf::from("/b/SomeEmoji.ttf"),
            PathBuf::from("/b/JoyPixels.ttf"),
            PathBuf::from("/a/Noto-Color-Emoji.ttf"),
            PathBuf::from("/a/Twemoji.ttf"),
        ];
        rank(&mut found);
        assert_eq!(
            found,
            vec![
                PathBuf::from("/a/Noto-Color-Emoji.ttf"),
                PathBuf::from("/a/Twemoji.ttf"),
                PathBuf::from("/b/JoyPixels.ttf"),
                PathBuf::from("/b/SomeEmoji.ttf"),
            ]
        );
    }

    #[test]
    fn a_directory_scan_collects_emoji_files_only() {
        let scratch = tempfile::tempdir().expect("scratch directory");
        let root = scratch.path();
        let nested = root.join("noto");
        std::fs::create_dir_all(&nested).expect("scratch directory");
        std::fs::write(nested.join("NotoColorEmoji.ttf"), b"").expect("file");
        std::fs::write(root.join("DejaVuSans.ttf"), b"").expect("file");
        let mut found = Vec::new();
        collect(root, 0, &mut found);
        assert_eq!(found, vec![nested.join("NotoColorEmoji.ttf")]);
    }
}
