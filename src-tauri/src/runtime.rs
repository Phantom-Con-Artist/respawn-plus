// SPDX-License-Identifier: MIT
use serde::Serialize;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize)]
pub struct Runtime {
    pub name: String,
    pub path: String,
    pub kind: String,
}

pub fn discover() -> Vec<Runtime> {
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    for (name, binary, kind) in [
        ("UMU Launcher", "umu-run", "umu"),
        ("Wine", "wine", "wine"),
        ("Wine (64-bit)", "wine64", "wine"),
        ("Proton", "proton", "proton"),
    ] {
        if let Ok(path) = which::which(binary) {
            add(&mut found, &mut seen, name, &path, kind);
        }
    }
    if let Some(home) = dirs::home_dir() {
        add(
            &mut found,
            &mut seen,
            "UMU Launcher",
            &home.join(".local/bin/umu-run"),
            "umu",
        );
        for relative in [
            ".local/share/Steam/compatibilitytools.d",
            ".local/share/Steam/steamapps/common",
            ".steam/root/compatibilitytools.d",
            ".steam/root/steamapps/common",
            ".var/app/com.valvesoftware.Steam/data/Steam/compatibilitytools.d",
            ".var/app/com.valvesoftware.Steam/data/Steam/steamapps/common",
            ".local/share/umu/compatibilitytools",
            ".local/share/lutris/runners/wine",
        ] {
            scan_directory(&home.join(relative), &mut found, &mut seen);
        }
    }
    for directory in [
        "/usr/share/steam/compatibilitytools.d",
        "/usr/share/steam/steamapps/common",
    ] {
        scan_directory(Path::new(directory), &mut found, &mut seen);
    }
    if let Some(paths) = std::env::var_os("RESPAWN_RUNTIME_DIRS") {
        for directory in std::env::split_paths(&paths) {
            scan_directory(&directory, &mut found, &mut seen);
        }
    }
    found.sort_by(|a, b| a.name.cmp(&b.name).then(a.path.cmp(&b.path)));
    found
}

fn scan_directory(directory: &Path, found: &mut Vec<Runtime>, seen: &mut HashSet<PathBuf>) {
    let Ok(entries) = directory.read_dir() else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.join("proton").is_file() {
            add(found, seen, &name, &path.join("proton"), "proton");
        } else {
            add(found, seen, &name, &path.join("bin/wine"), "wine");
        }
    }
}

fn add(found: &mut Vec<Runtime>, seen: &mut HashSet<PathBuf>, name: &str, path: &Path, kind: &str) {
    let Ok(executable) = which::which(path) else {
        return;
    };
    let Ok(canonical) = executable.canonicalize() else {
        return;
    };
    if seen.insert(canonical.clone()) {
        found.push(Runtime {
            name: name.into(),
            path: canonical.display().to_string(),
            kind: kind.into(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn finds_proton_in_paths_with_spaces_and_deduplicates_symlinks() {
        let directory = tempfile::tempdir().unwrap();
        let proton = directory.path().join("GE Proton Test");
        std::fs::create_dir(&proton).unwrap();
        std::fs::write(proton.join("proton"), "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(
            proton.join("proton"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        std::os::unix::fs::symlink(&proton, directory.path().join("Alias")).unwrap();
        let mut found = Vec::new();
        scan_directory(directory.path(), &mut found, &mut HashSet::new());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, "proton");
        assert!(found[0].path.ends_with("GE Proton Test/proton"));
    }
}
