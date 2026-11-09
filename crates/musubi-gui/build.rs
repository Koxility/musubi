//! Hinterlegt den Git-Stand im Build, damit die Oberfläche zeigt, welche Version wirklich läuft.

use std::path::Path;
use std::process::Command;

fn main() {
    let commit = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|hash| hash.trim().to_owned())
        .filter(|hash| !hash.is_empty())
        .unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=MUSUBI_COMMIT={commit}");

    // Neu bauen, sobald sich der ausgecheckte Stand ändert, sonst bliebe der Hash veraltet.
    let git = Path::new("../../.git");
    let head = git.join("HEAD");
    let mut watched = vec![head.clone(), git.join("packed-refs")];
    if let Ok(content) = std::fs::read_to_string(&head)
        && let Some(reference) = content.strip_prefix("ref: ")
    {
        watched.push(git.join(reference.trim()));
    }
    for path in watched.iter().filter(|p| p.exists()) {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    println!("cargo:rerun-if-changed=build.rs");
}
