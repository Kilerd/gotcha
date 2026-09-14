#!/usr/bin/env python3
"""Test standalone consumers without workspace dependency feature unification."""

import os
from pathlib import Path
import subprocess


def main():
    root = Path(__file__).resolve().parent
    fixtures = root / "tests" / "fixtures" / "derive"
    env = dict(os.environ, CARGO_TARGET_DIR=str(root / "target" / "macro-dependencies"))
    for name in ("core-only", "gotcha-only", "renamed-core", "renamed-gotcha", "facade-only"):
        manifest = str(fixtures / name / "Cargo.toml")
        print(f"Testing standalone macro dependencies: {name}", flush=True)
        subprocess.run(["cargo", "fmt", "--manifest-path", manifest, "--", "--check"], check=True, env=env)
        subprocess.run(["cargo", "test", "--manifest-path", manifest], check=True, env=env)
        if name in ("core-only", "renamed-core", "facade-only"):
            tree = subprocess.check_output(
                ["cargo", "tree", "--manifest-path", manifest, "--edges", "normal,build", "--prefix", "none", "--format", "{p}"],
                text=True,
                env=env,
            )
            packages = {line.split()[0] for line in tree.splitlines()}
            unexpected = packages & {"gotcha", "axum", "tokio", "tower"}
            if unexpected:
                raise RuntimeError(f"{name} pulled in web dependencies: {sorted(unexpected)}")

    manifest = str(root / "tests" / "fixtures" / "no-default-features" / "Cargo.toml")
    subprocess.run(["cargo", "fmt", "--manifest-path", manifest, "--", "--check"], check=True, env=env)
    for features in ([], ["--features", "openapi"]):
        print(f"Checking startup without default features: {features}", flush=True)
        subprocess.run(["cargo", "check", "--manifest-path", manifest, *features], check=True, env=env)


if __name__ == "__main__":
    main()
