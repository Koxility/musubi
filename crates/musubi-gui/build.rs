//! Embeds the git commit in the build, so the interface shows which version is really running.
//!
//! The commit comes from, in this order: the `MUSUBI_COMMIT` environment variable (for
//! packagers), the git checkout this crate lives in, and `commit.txt`, which `git archive`
//! and the source tarballs on GitHub fill in through `export-subst` (see `.gitattributes`).

use std::path::Path;
use std::process::Command;

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_owned());
    let root = Path::new(&manifest_dir).join("../..");
    let git = root.join(".git");

    let commit = std::env::var("MUSUBI_COMMIT")
        .ok()
        .map(|hash| hash.trim().to_owned())
        .filter(|hash| !hash.is_empty())
        .or_else(|| git_commit(&root, &git))
        .or_else(|| archive_commit(Path::new(&manifest_dir)))
        .unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=MUSUBI_COMMIT={commit}");

    // Rebuild whenever the checked-out commit changes, or the hash would go stale.
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
    println!("cargo:rerun-if-changed=commit.txt");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=MUSUBI_COMMIT");
}

/// Asks git only when the workspace root is a checkout itself. An unpacked source tarball
/// can sit inside an unrelated repository, such as an AUR package clone, whose commit
/// would be wrong here.
fn git_commit(root: &Path, git: &Path) -> Option<String> {
    if !git.exists() {
        return None;
    }
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|hash| hash.trim().to_owned())
        .filter(|hash| !hash.is_empty())
}

/// Reads the hash that `git archive` wrote into `commit.txt`. In a checkout the file still
/// holds the unexpanded placeholder, which is ignored.
fn archive_commit(manifest_dir: &Path) -> Option<String> {
    let content = std::fs::read_to_string(manifest_dir.join("commit.txt")).ok()?;
    let hash = content.trim();
    let is_hash = !hash.is_empty() && hash.chars().all(|c| c.is_ascii_hexdigit());
    is_hash.then(|| hash.to_owned())
}
