// SPDX-License-Identifier: MIT
//! Disposable real-game smoke/corruption/recovery lab. Never imports the original game.
//! cargo run --example room_lab -- <new-lab-directory> <game.exe> <wine|proton> <runner>
use base64::Engine;
use respawn_plus_lib::{
    library::{GameSettings, Launcher},
    rooms::RoomProfile,
};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 4 {
        return Err("Usage: room_lab <new-lab-directory> <game.exe> <wine|proton> <runner>".into());
    }
    let base = PathBuf::from(&args[0]);
    if base.exists() {
        return Err("Lab directory must not already exist".into());
    }
    fs::create_dir_all(&base)?;
    let base = base.canonicalize()?;
    fs::write(
        base.join("RESPAWN-DISPOSABLE-LAB"),
        "Only disposable copies are modified by this test.\n",
    )?;
    let source = Path::new(&args[1]).canonicalize()?;
    let copied = base.join("game");
    let result = Command::new("cp")
        .args(["--archive", "--reflink=auto", "--"])
        .arg(source.parent().ok_or("Missing game directory")?)
        .arg(&copied)
        .status()?;
    if !result.success() {
        return Err("Could not create an independent game copy".into());
    }
    let exe = copied.join(source.file_name().ok_or("Missing game executable")?);
    let original = fs::read(&exe)?;
    let service = Launcher::at(base.join("state"), base.join("logs"))?;
    let game = service.import(&exe, None, "room-lab")?;
    service.save(GameSettings {
        id: game.id,
        name: game.name.clone(),
        runtime_kind: args[2].clone(),
        runtime_path: args[3].clone(),
        arguments: vec![],
        display_mode: "default".into(),
        display_width: None,
        display_height: None,
        launch_via_tool_id: None,
    })?;
    service.configure_room(
        game.id,
        RoomProfile {
            network: "offline".into(),
            display: "nested-x11".into(),
            audio: false,
            controllers: false,
            game_save_paths: vec!["lab-saves".into()],
            ..RoomProfile::default()
        },
    )?;
    fs::create_dir(copied.join("lab-saves"))?;
    fs::write(copied.join("lab-saves/progress.txt"), "old progress")?;
    fs::write(copied.join("lab-config.ini"), "healthy configuration")?;
    let private_home = base.join(format!("state/rooms/game-{}/home", game.id));
    fs::create_dir_all(&private_home)?;
    fs::write(
        private_home.join("lab-settings.ini"),
        "healthy private home",
    )?;
    let checkpoint = service.create_checkpoint(game.id)?;
    let mut reports = Vec::new();
    for cycle in 0..2 {
        service.launch(game.id)?;
        // Allow initial Wine/Proton boot and a real game window to appear.
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut diagnostics = json!({});
        while Instant::now() < deadline {
            std::thread::sleep(Duration::from_secs(1));
            diagnostics = service.inspect_room(game.id)?;
            let windows = diagnostics["windows"].as_str().unwrap_or("");
            if windows.contains("Mines") || windows.contains("Skyrim") {
                break;
            }
            let game = service
                .games()?
                .into_iter()
                .find(|g| g.id == game.id)
                .ok_or("Game disappeared")?;
            if game
                .launch
                .as_ref()
                .is_some_and(|l| l.state == "failed" || l.state == "finished")
            {
                break;
            }
        }
        let log = service.log(game.id).unwrap_or_default();
        let snapshot = service.inspect_room_image(game.id)?;
        if let Some(image) = snapshot["image_base64"].as_str() {
            fs::write(
                base.join(format!("cycle-{cycle}.png")),
                base64::engine::general_purpose::STANDARD.decode(image)?,
            )?;
        }
        let windows = diagnostics["windows"].as_str().unwrap_or("");
        let game_window_visible = windows.contains("Mines") || windows.contains("Skyrim");
        let runtime_launcher =
            fs::read_to_string(private_home.join(".local/share/umu/steamrt4/run"))
                .unwrap_or_default();
        reports.push(json!({"cycle":cycle,"diagnostics":diagnostics,"launch_log":log,"runtime_launcher":runtime_launcher}));
        fs::write(
            base.join("report.json"),
            serde_json::to_vec_pretty(&reports)?,
        )?;
        if !game_window_visible {
            return Err(format!(
                "No real game window appeared; inspect {}",
                base.join("report.json").display()
            )
            .into());
        }
        service.stop_room(game.id)?;
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            let game = service
                .games()?
                .into_iter()
                .find(|g| g.id == game.id)
                .ok_or("Game disappeared")?;
            if game
                .launch
                .as_ref()
                .is_none_or(|l| l.state != "running" && l.state != "starting")
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        // These writes only target the independently copied game and the disposable prefix.
        fs::write(
            copied.join("lab-saves/progress.txt"),
            format!("latest progress {cycle}"),
        )?;
        fs::write(copied.join("lab-config.ini"), "POISONED CONFIGURATION")?;
        fs::write(
            private_home.join("lab-settings.ini"),
            "POISONED PRIVATE HOME",
        )?;
        let prefix = if args[2] == "proton" {
            Path::new(&game.prefix).join("pfx")
        } else {
            PathBuf::from(&game.prefix)
        };
        fs::write(
            prefix.join("system.reg"),
            "INTENTIONALLY CORRUPTED REGISTRY",
        )?;
        fs::write(&exe, b"INTENTIONALLY CORRUPTED EXECUTABLE")?;
        if service.launch(game.id).is_ok() {
            return Err("Corrupt executable was unexpectedly launched".into());
        }
        service.restore_checkpoint(game.id, checkpoint)?;
        assert_eq!(fs::read(&exe)?, original);
        assert_eq!(
            fs::read_to_string(private_home.join("lab-settings.ini"))?,
            "healthy private home"
        );
        assert_eq!(
            fs::read_to_string(copied.join("lab-config.ini"))?,
            "healthy configuration"
        );
        assert_eq!(
            fs::read_to_string(copied.join("lab-saves/progress.txt"))?,
            format!("latest progress {cycle}")
        );
    }
    println!(
        "{}",
        json!({"lab":base,"cycles":2,"independent_copy":true,"executable_and_config_restored":true,"newest_test_saves_preserved":true,"report":base.join("report.json")})
    );
    Ok(())
}
