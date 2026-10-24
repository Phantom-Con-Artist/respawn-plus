// SPDX-License-Identifier: MIT
//! Per-game display mode. Every game is shown through its room's private X screen, so Vessel
//! sets the mode on that screen instead of in any game's own settings files. This works the same
//! for every game: the game picks its own render resolution and Proton scales its fullscreen
//! output to the screen.
use std::fmt;

pub const MODES: [&str; 4] = ["default", "fullscreen", "borderless", "windowed"];

/// The room's private X screen: a fullscreen surface on the user's monitor, or a decorated window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Screen {
    pub fullscreen: bool,
    pub width: u32,
    pub height: u32,
}

impl fmt::Display for Screen {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = if self.fullscreen {
            "fullscreen"
        } else {
            "window"
        };
        write!(f, "{kind} {}×{}", self.width, self.height)
    }
}

/// Whether a room with this display profile shows games on its private X screen. Native Wayland
/// and headless rooms have no such screen, so their games keep their own window settings.
pub fn room_shows(profile_display: &str) -> bool {
    matches!(profile_display, "auto" | "nested-x11")
}

/// Fullscreen and windowed use the user's resolution (fullscreen scales it to fill the monitor);
/// default and borderless fill the monitor at its own resolution. `monitor` is the physical size
/// the host windowing system reports for the launcher's monitor. Nothing is guessed: with no
/// usable size the launch fails with a message saying what to set.
pub fn screen(
    mode: &str,
    width: Option<u32>,
    height: Option<u32>,
    monitor: Option<(u32, u32)>,
) -> Result<Screen, String> {
    let chosen = width.zip(height);
    let (fullscreen, size) = match mode {
        "default" | "borderless" => (true, monitor.or(chosen)),
        "fullscreen" => (true, chosen),
        "windowed" => (false, chosen),
        _ => return Err(format!("Unknown display mode: {mode}")),
    };
    let (width, height) = size.ok_or(
        "Cannot determine a size for this game's display. Set a resolution in its display settings.",
    )?;
    if !(320..=16384).contains(&width) || !(200..=16384).contains(&height) {
        return Err("Choose a display resolution between 320×200 and 16384×16384".into());
    }
    Ok(Screen {
        fullscreen,
        width,
        height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_follows_the_users_setting_else_the_launcher_monitor() {
        let monitor = Some((2560, 1440));
        let (w, h) = (Some(1600), Some(900));
        let full = |width, height| Screen {
            fullscreen: true,
            width,
            height,
        };
        assert_eq!(screen("default", None, None, monitor), Ok(full(2560, 1440)));
        assert_eq!(screen("borderless", w, h, monitor), Ok(full(2560, 1440)));
        assert_eq!(screen("borderless", w, h, None), Ok(full(1600, 900)));
        assert_eq!(screen("fullscreen", w, h, monitor), Ok(full(1600, 900)));
        assert_eq!(
            screen("windowed", w, h, None),
            Ok(Screen {
                fullscreen: false,
                width: 1600,
                height: 900
            })
        );
        assert_eq!(
            screen("windowed", w, h, None).unwrap().to_string(),
            "window 1600×900"
        );
    }

    #[test]
    fn refuses_to_guess_a_size_or_accept_an_unusable_one() {
        assert!(screen("default", None, None, None).is_err());
        assert!(screen("fullscreen", None, None, Some((1920, 1080))).is_err());
        assert!(screen("windowed", Some(10), Some(10), None).is_err());
        assert!(screen("stretched", Some(1920), Some(1080), None).is_err());
    }
}
