//! Bakes the release version and git commit into `mazbri_build_info`.
//!
//! - `MAZBRI_VERSION`: from the repository `VERSION` file (single version for all services).
//! - `MAZBRI_GIT_SHA`: build-time env `MAZBRI_GIT_SHA` when set, otherwise `git rev-parse`
//!   of the checkout; `unknown` when neither is available.
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set"));
    let repo_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("mazbri-core must live at <repo>/mazbri-libs/mazbri-core");

    let version_file = repo_root.join("VERSION");
    println!("cargo:rerun-if-changed={}", version_file.display());
    let version = std::fs::read_to_string(&version_file)
        .ok()
        .and_then(|raw| raw.split_whitespace().next().map(str::to_owned))
        .unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=MAZBRI_VERSION={version}");

    println!("cargo:rerun-if-env-changed=MAZBRI_GIT_SHA");
    let sha = std::env::var("MAZBRI_GIT_SHA")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| git_sha(repo_root))
        .unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=MAZBRI_GIT_SHA={}", sha.trim());
}

fn git(repo_root: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8(out.stdout).ok()?.trim().to_owned();
    (!text.is_empty()).then_some(text)
}

fn git_sha(repo_root: &Path) -> Option<String> {
    let git_dir = PathBuf::from(git(repo_root, &["rev-parse", "--absolute-git-dir"])?);
    println!("cargo:rerun-if-changed={}", git_dir.join("HEAD").display());
    println!(
        "cargo:rerun-if-changed={}",
        git_dir.join("packed-refs").display()
    );
    if let Some(head_ref) = git(repo_root, &["symbolic-ref", "-q", "HEAD"]) {
        println!(
            "cargo:rerun-if-changed={}",
            git_dir.join(head_ref).display()
        );
    }
    git(repo_root, &["rev-parse", "--short=12", "HEAD"])
}
