// SPDX-License-Identifier: MIT
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args
        .iter()
        .any(|arg| arg == "--list-games" || arg == "--scan-games")
    {
        let result = respawn_plus_lib::library::Launcher::new().and_then(|service| {
            if args.iter().any(|arg| arg == "--scan-games") {
                let report = service.scan(None)?;
                eprintln!(
                    "Discovered {} executable candidates; added {} games",
                    report.discovered, report.added
                );
            }
            serde_json::to_string_pretty(&service.games()?).map_err(|e| e.to_string())
        });
        match result {
            Ok(json) => println!("{json}"),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
    } else if args.iter().any(|arg| arg == "--diagnose") {
        match respawn_plus_lib::environment_status() {
            Ok(status) => println!("{}", serde_json::to_string_pretty(&status).unwrap()),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
    } else {
        respawn_plus_lib::run();
    }
}
