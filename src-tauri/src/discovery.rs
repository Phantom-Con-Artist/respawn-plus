// SPDX-License-Identifier: MIT
use serde::Serialize;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

#[derive(Serialize)]
pub struct Discovery {
    pub executables: Vec<PathBuf>,
    pub roots: Vec<String>,
    pub limited: bool,
    pub skipped_directories: usize,
}

pub fn default_roots(data: &Path) -> Vec<PathBuf> {
    let mut roots = vec![data.join("test-games")];
    if let Some(home) = dirs::home_dir() {
        roots.extend([
            home.join("Games"),
            home.join("Downloads"),
            home.join(".local/share/Steam/steamapps/common"),
            home.join(".steam/root/steamapps/common"),
            home.join(".var/app/com.valvesoftware.Steam/data/Steam/steamapps/common"),
        ]);
    }
    if let Some(downloads) = dirs::download_dir() {
        roots.push(downloads);
    }
    roots
}

pub fn scan(roots: Vec<PathBuf>) -> Discovery {
    let mut result = Discovery {
        executables: vec![],
        roots: vec![],
        limited: false,
        skipped_directories: 0,
    };
    let mut visited = HashSet::new();
    let mut budget = 20_000usize;
    for root in roots {
        if !root.is_dir() {
            continue;
        }
        result.roots.push(root.display().to_string());
        walk(&root, 0, &mut visited, &mut budget, &mut result);
    }
    result.executables.sort();
    result.executables.dedup();
    result
}

fn walk(
    directory: &Path,
    depth: usize,
    visited: &mut HashSet<PathBuf>,
    budget: &mut usize,
    result: &mut Discovery,
) {
    if *budget == 0 {
        result.limited = true;
        return;
    }
    let Ok(canonical) = directory.canonicalize() else {
        result.skipped_directories += 1;
        return;
    };
    if !visited.insert(canonical) {
        return;
    }
    let Ok(entries) = fs::read_dir(directory) else {
        result.skipped_directories += 1;
        return;
    };
    let mut candidates = vec![];
    let mut children = vec![];
    for entry in entries.flatten() {
        if *budget == 0 {
            result.limited = true;
            break;
        }
        *budget -= 1;
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        // Do not follow arbitrary symlink trees or walk Wine drive_c installations.
        if kind.is_symlink() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if kind.is_dir() {
            if [
                "_commonredist",
                "redist",
                "redistributables",
                "__installer",
                "support",
                "node_modules",
                "drive_c",
                "windows",
                ".git",
            ]
            .contains(&name.as_str())
            {
                continue;
            }
            if depth < 7 {
                children.push(entry.path());
            } else {
                result.limited = true;
            }
        } else if kind.is_file() && name.ends_with(".exe") && !is_helper(&name) {
            candidates.push(entry.path());
        }
    }
    candidates.sort_by_key(|p| {
        let n = p
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();
        (n.contains("launcher"), n)
    });
    // Prefer the actual game over an adjacent launcher. Manual import remains available.
    if let Some(executable) = candidates.into_iter().next() {
        if let Ok(path) = executable.canonicalize() {
            result.executables.push(path);
        }
    }
    children.sort();
    for child in children {
        walk(&child, depth + 1, visited, budget, result);
    }
}

fn is_helper(name: &str) -> bool {
    [
        "unins",
        "uninstall",
        "setup",
        "install",
        "crash",
        "reporter",
        "vc_redist",
        "vcredist",
        "dxsetup",
        "unitycrash",
        "helper",
        "updater",
    ]
    .iter()
    .any(|part| name.contains(part))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn skips_installers_support_files_and_symlink_cycles() {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path().join("Skyrim");
        fs::create_dir_all(game.join("_CommonRedist")).unwrap();
        for name in [
            "SkyrimSE.exe",
            "SkyrimSELauncher.exe",
            "unins000.exe",
            "_CommonRedist/vc_redist.exe",
        ] {
            fs::write(game.join(name), "MZ").unwrap();
        }
        std::os::unix::fs::symlink(temp.path(), game.join("cycle")).unwrap();
        let found = scan(vec![temp.path().to_owned(), game]);
        assert_eq!(found.executables.len(), 1);
        assert!(found.executables[0].ends_with("SkyrimSE.exe"));
    }
}
