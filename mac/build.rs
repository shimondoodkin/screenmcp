/// Expose SCREENMCP_VERSION derived from the git tag.
/// Priority: CI tag (GITHUB_REF_NAME) -> `git describe --tags` -> Cargo version.
fn main() {
    let version = std::env::var("GITHUB_REF_NAME")
        .ok()
        .filter(|s| s.starts_with('v'))
        .or_else(git_describe)
        .unwrap_or_else(|| format!("v{}", std::env::var("CARGO_PKG_VERSION").unwrap_or_default()));

    println!("cargo:rustc-env=SCREENMCP_VERSION={version}");
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/refs/tags");
    println!("cargo:rerun-if-env-changed=GITHUB_REF_NAME");
}

fn git_describe() -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}
