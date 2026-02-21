#!/usr/bin/env python3
"""
Release script for ScreenMCP.

Bumps version across all packages, commits, tags, and pushes to trigger
the GitHub Actions release workflow.

Usage:
    python release.py 0.2.0          # release v0.2.0
    python release.py 0.2.0-beta.1   # pre-release
    python release.py --dry-run 0.2.0 # show what would change
"""

import argparse
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).parent

# Files containing version strings to update.
# Each entry: (path, pattern, replacement_template)
# replacement_template uses {version} for the new version.
VERSION_FILES = [
    # Rust desktop clients
    ("windows/Cargo.toml", r'^version = ".*"', 'version = "{version}"'),
    ("mac/Cargo.toml", r'^version = ".*"', 'version = "{version}"'),
    ("linux/Cargo.toml", r'^version = ".*"', 'version = "{version}"'),
    # Rust worker
    ("worker/Cargo.toml", r'^version = ".*"', 'version = "{version}"'),
    # Node.js packages
    ("web/package.json", r'"version": ".*?"', '"version": "{version}"'),
    ("remote/package.json", r'"version": ".*?"', '"version": "{version}"'),
    ("sdk/typescript/package.json", r'"version": ".*?"', '"version": "{version}"'),
    # Python SDK
    ("sdk/python/pyproject.toml", r'^version = ".*"', 'version = "{version}"'),
    # Android app
    ("app/build.gradle.kts", r'versionName = ".*"', 'versionName = "{version}"'),
]


def validate_version(version: str) -> bool:
    """Check that version looks like a semver string."""
    return bool(re.match(r"^\d+\.\d+\.\d+(-[\w.]+)?$", version))


def bump_android_version_code(path: Path) -> None:
    """Increment the Android versionCode by 1."""
    text = path.read_text()
    match = re.search(r"versionCode = (\d+)", text)
    if match:
        old_code = int(match.group(1))
        new_code = old_code + 1
        text = text.replace(f"versionCode = {old_code}", f"versionCode = {new_code}")
        path.write_text(text)
        print(f"  {path}: versionCode {old_code} -> {new_code}")


def update_version_in_file(rel_path: str, pattern: str, replacement: str, version: str, dry_run: bool) -> bool:
    """Update the first occurrence of pattern in a file. Returns True if changed."""
    path = ROOT / rel_path
    if not path.exists():
        print(f"  SKIP {rel_path} (not found)")
        return False

    text = path.read_text()
    new_replacement = replacement.format(version=version)
    new_text, count = re.subn(pattern, new_replacement, text, count=1, flags=re.MULTILINE)

    if count == 0:
        print(f"  SKIP {rel_path} (pattern not found)")
        return False

    if text == new_text:
        print(f"  SKIP {rel_path} (already {version})")
        return False

    print(f"  {rel_path}: {new_replacement}")
    if not dry_run:
        path.write_text(new_text)
    return True


def run(cmd: list[str], dry_run: bool = False, check: bool = True) -> subprocess.CompletedProcess:
    """Run a command, or just print it in dry-run mode."""
    display = " ".join(cmd)
    if dry_run:
        print(f"  [dry-run] {display}")
        return subprocess.CompletedProcess(cmd, 0, "", "")
    print(f"  $ {display}")
    return subprocess.run(cmd, cwd=ROOT, check=check, capture_output=True, text=True)


def get_current_version() -> str:
    """Read the current version from windows/Cargo.toml."""
    path = ROOT / "windows" / "Cargo.toml"
    if not path.exists():
        return "unknown"
    match = re.search(r'^version = "(.*)"', path.read_text(), re.MULTILINE)
    return match.group(1) if match else "unknown"


def check_clean_working_tree() -> bool:
    """Ensure there are no uncommitted changes."""
    result = subprocess.run(
        ["git", "status", "--porcelain"],
        cwd=ROOT, capture_output=True, text=True,
    )
    return result.stdout.strip() == ""


def main():
    parser = argparse.ArgumentParser(description="Release a new version of ScreenMCP")
    parser.add_argument("version", help="Version to release (e.g. 0.2.0, 0.3.0-beta.1)")
    parser.add_argument("--dry-run", action="store_true", help="Show changes without applying them")
    parser.add_argument("--no-push", action="store_true", help="Commit and tag but don't push")
    args = parser.parse_args()

    version = args.version.lstrip("v")
    tag = f"v{version}"

    if not validate_version(version):
        print(f"Error: '{version}' is not a valid semver version")
        sys.exit(1)

    current = get_current_version()
    print(f"Current version: {current}")
    print(f"New version:     {version}")
    print(f"Git tag:         {tag}")
    print()

    if not args.dry_run and not check_clean_working_tree():
        print("Error: Working tree has uncommitted changes. Commit or stash them first.")
        sys.exit(1)

    # Check if tag already exists
    result = subprocess.run(
        ["git", "tag", "-l", tag],
        cwd=ROOT, capture_output=True, text=True,
    )
    if result.stdout.strip():
        print(f"Error: Tag {tag} already exists")
        sys.exit(1)

    # Step 1: Bump versions
    print("Bumping versions...")
    changed = False
    for rel_path, pattern, replacement in VERSION_FILES:
        if update_version_in_file(rel_path, pattern, replacement, version, args.dry_run):
            changed = True

    # Bump Android versionCode
    gradle_path = ROOT / "app" / "build.gradle.kts"
    if gradle_path.exists():
        if args.dry_run:
            match = re.search(r"versionCode = (\d+)", gradle_path.read_text())
            if match:
                print(f"  app/build.gradle.kts: versionCode {match.group(1)} -> {int(match.group(1)) + 1}")
        else:
            bump_android_version_code(gradle_path)
        changed = True

    if not changed:
        print("\nNo files changed — already at this version?")
        sys.exit(1)

    print()

    # Step 2: Stage and commit
    print("Committing...")
    files_to_stage = [rel_path for rel_path, _, _ in VERSION_FILES]
    files_to_stage.append("app/build.gradle.kts")
    existing_files = [f for f in files_to_stage if (ROOT / f).exists()]
    run(["git", "add"] + existing_files, dry_run=args.dry_run)
    run(["git", "commit", "-m", f"Release {tag}"], dry_run=args.dry_run)

    # Step 3: Tag
    print("Tagging...")
    run(["git", "tag", "-a", tag, "-m", f"Release {tag}"], dry_run=args.dry_run)

    # Step 4: Push
    if args.no_push:
        print(f"\nDone (--no-push). To publish the release:")
        print(f"  git push origin master {tag}")
    elif args.dry_run:
        print(f"\nDry run complete. Run without --dry-run to apply changes.")
    else:
        print("Pushing...")
        run(["git", "push", "origin", "master"], dry_run=args.dry_run)
        run(["git", "push", "origin", tag], dry_run=args.dry_run)
        print(f"\nReleased {tag}!")
        print(f"GitHub Actions will now build and publish assets.")
        print(f"Track progress: https://github.com/shimondoodkin/phonemcp/actions")

    return 0


if __name__ == "__main__":
    sys.exit(main() or 0)
