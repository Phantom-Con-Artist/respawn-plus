// SPDX-License-Identifier: MIT
//! Per-game display mode (fullscreen, borderless, windowed). Games choose their own window mode,
//! so Respawn+ can only set it for engines whose settings file it understands; others keep the
//! game's own setting.
use std::{
    fs,
    path::{Path, PathBuf},
};

pub const MODES: [&str; 4] = ["default", "fullscreen", "borderless", "windowed"];

/// Creation Engine games keep display settings in `<Documents>/My Games/<folder>/<file>`.
struct Adapter {
    executable: &'static str,
    folder: &'static str,
    file: &'static str,
}

const ADAPTERS: [Adapter; 2] = [
    Adapter {
        executable: "skyrimse.exe",
        folder: "Skyrim Special Edition",
        file: "SkyrimPrefs.ini",
    },
    Adapter {
        executable: "fallout4.exe",
        folder: "Fallout4",
        file: "Fallout4Prefs.ini",
    },
];

fn adapter(executable: &Path) -> Option<&'static Adapter> {
    let name = executable.file_name()?.to_str()?.to_ascii_lowercase();
    ADAPTERS.iter().find(|a| a.executable == name)
}

pub fn supported(executable: &Path) -> bool {
    adapter(executable).is_some()
}

/// The prefs file inside a prefix. Proton keeps the Wine prefix in `pfx/` with user `steamuser`;
/// plain Wine uses the prefix directly with the login name.
pub fn prefs_path(prefix: &Path, runtime_kind: &str, executable: &Path) -> Option<PathBuf> {
    let adapter = adapter(executable)?;
    let (root, user) = if runtime_kind == "proton" {
        (prefix.join("pfx"), "steamuser".to_string())
    } else {
        (
            prefix.to_path_buf(),
            std::env::var("USER").unwrap_or_else(|_| "user".into()),
        )
    };
    Some(
        root.join("drive_c/users")
            .join(user)
            .join("Documents/My Games")
            .join(adapter.folder)
            .join(adapter.file),
    )
}

/// Writes the display mode before launch. Returns a short description for the launch log, or
/// `None` when the mode is "default" or the game has no adapter.
pub fn apply(
    prefix: &Path,
    runtime_kind: &str,
    executable: &Path,
    mode: &str,
    width: u32,
    height: u32,
) -> Result<Option<String>, String> {
    if mode == "default" {
        return Ok(None);
    }
    let Some(path) = prefs_path(prefix, runtime_kind, executable) else {
        return Ok(None);
    };
    let (full, borderless) = match mode {
        "fullscreen" => ("1", "0"),
        "borderless" => ("0", "1"),
        "windowed" => ("0", "0"),
        _ => return Err(format!("Unknown display mode: {mode}")),
    };
    if !(320..=16384).contains(&width) || !(200..=16384).contains(&height) {
        return Err("Choose a display resolution between 320×200 and 16384×16384".into());
    }
    let text = match fs::read(&path) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("Cannot read {}: {e}", path.display())),
    };
    let (w, h) = (width.to_string(), height.to_string());
    let updated = set_keys(
        &text,
        "Display",
        &[
            ("bFull Screen", full),
            ("bBorderless", borderless),
            ("iSize W", &w),
            ("iSize H", &h),
        ],
    );
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&path, updated).map_err(|e| format!("Cannot write {}: {e}", path.display()))?;
    Ok(Some(format!(
        "{mode} {width}×{height} → {}",
        path.display()
    )))
}

/// Sets `key=value` pairs inside `[section]`, keeping every other line, comment, key order and
/// the file's line endings. Missing keys are appended to the section; a missing section is added.
fn set_keys(text: &str, section: &str, pairs: &[(&str, &str)]) -> String {
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let header = |line: &str| {
        line.trim()
            .strip_prefix('[')
            .and_then(|l| l.strip_suffix(']'))
            .map(str::to_string)
    };
    let start = lines
        .iter()
        .position(|l| header(l).is_some_and(|h| h.eq_ignore_ascii_case(section)));
    let start = match start {
        Some(i) => i,
        None => {
            if lines.last().is_some_and(|l| !l.trim().is_empty()) {
                lines.push(String::new());
            }
            lines.push(format!("[{section}]"));
            lines.len() - 1
        }
    };
    let end = lines[start + 1..]
        .iter()
        .position(|l| header(l).is_some())
        .map_or(lines.len(), |i| start + 1 + i);
    let mut insert_at = end;
    while insert_at > start + 1 && lines[insert_at - 1].trim().is_empty() {
        insert_at -= 1;
    }
    for (key, value) in pairs {
        let found = lines[start + 1..insert_at].iter().position(|l| {
            l.split_once('=')
                .is_some_and(|(k, _)| k.trim().eq_ignore_ascii_case(key))
        });
        match found {
            Some(i) => lines[start + 1 + i] = format!("{key}={value}"),
            None => {
                lines.insert(insert_at, format!("{key}={value}"));
                insert_at += 1;
            }
        }
    }
    let mut out = lines.join(newline);
    out.push_str(newline);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn updates_only_display_keys_and_keeps_crlf_and_other_sections() {
        let input = "[Display]\r\nfShadowDistance=3000.0000\r\nbBorderless=0\r\nbFull Screen=0\r\niSize H=720\r\niSize W=1280\r\n\r\n[Imagespace]\r\nbDoDepthOfField=1\r\n";
        let out = set_keys(
            input,
            "Display",
            &[
                ("bFull Screen", "0"),
                ("bBorderless", "1"),
                ("iSize W", "2560"),
                ("iSize H", "1440"),
            ],
        );
        assert_eq!(out, "[Display]\r\nfShadowDistance=3000.0000\r\nbBorderless=1\r\nbFull Screen=0\r\niSize H=1440\r\niSize W=2560\r\n\r\n[Imagespace]\r\nbDoDepthOfField=1\r\n");
    }

    #[test]
    fn adds_missing_keys_and_section() {
        assert_eq!(
            set_keys(
                "[General]\nsLanguage=ENGLISH\n",
                "Display",
                &[("bFull Screen", "1")]
            ),
            "[General]\nsLanguage=ENGLISH\n\n[Display]\nbFull Screen=1\n"
        );
        assert_eq!(
            set_keys(
                "[Display]\nfGamma=1.0\n\n[Audio]\nfVolume=1\n",
                "Display",
                &[("bBorderless", "1")]
            ),
            "[Display]\nfGamma=1.0\nbBorderless=1\n\n[Audio]\nfVolume=1\n"
        );
    }

    #[test]
    fn applies_to_proton_prefix_and_ignores_unsupported_games_and_default_mode() {
        let temp = tempfile::tempdir().unwrap();
        let skyrim = Path::new("/games/Skyrim/SkyrimSE.exe");
        let note = apply(temp.path(), "proton", skyrim, "fullscreen", 1920, 1080)
            .unwrap()
            .unwrap();
        let path = temp.path().join(
            "pfx/drive_c/users/steamuser/Documents/My Games/Skyrim Special Edition/SkyrimPrefs.ini",
        );
        assert!(note.contains("fullscreen 1920×1080"));
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "[Display]\nbFull Screen=1\nbBorderless=0\niSize W=1920\niSize H=1080\n"
        );
        assert_eq!(
            apply(temp.path(), "proton", skyrim, "default", 1920, 1080).unwrap(),
            None
        );
        assert_eq!(
            apply(
                temp.path(),
                "proton",
                Path::new("/g/Mines.exe"),
                "windowed",
                1280,
                720
            )
            .unwrap(),
            None
        );
        assert!(apply(temp.path(), "proton", skyrim, "windowed", 10, 10).is_err());
        assert!(!supported(Path::new("/g/Mines.exe")) && supported(Path::new("/g/SKYRIMSE.EXE")));
    }
}
