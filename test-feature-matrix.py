#!/usr/bin/env python3

"""Feature combinations exercised by CI.

We deliberately do *not* test the powerset. With N optional features that is 2^N - 1 jobs (7
features once meant 127), and almost all of those combinations tell us nothing: `task` and
`prometheus` share no code, so testing them together adds no signal over testing each alone.

What actually catches breakage is: does the crate build with nothing enabled, does each feature
stand on its own, and do they all coexist. That is N + 3 jobs: the default build, a bare
`--no-default-features` build, each optional feature alone, and every feature together. The
feature-alone jobs disable default features, so they also prove the optional surface compiles
without the data-type `Schematic` impls that are on by default. The final job is a real
`--all-features` build, which turns the default features back on and runs every test, including
those that need both `openapi` and a data-type impl.

The default features themselves (the data-type impls) are covered by the default and all-features
jobs. They are not jobs of their own: enabling one on top of the default build would test nothing
new.
"""

import json
import re
import shlex
import subprocess
import sys


def load_features():
    """Optional features from gotcha/Cargo.toml, in declaration order.

    Features listed in `default` are not optional here: the default and all-features jobs
    already enable them, and the per-feature jobs test the no-default build.
    """
    features = {}
    with open("gotcha/Cargo.toml", "r") as f:
        in_features = False
        for line in f.read().split("\n"):
            stripped = line.strip()
            if stripped.startswith("[features]"):
                in_features = True
                continue
            elif stripped.startswith("["):
                in_features = False
            elif in_features and "=" in stripped and not stripped.startswith("#"):
                name, _, value = stripped.partition("=")
                features[name.strip()] = re.findall(r'"([^"]+)"', value)
    defaults = set(features.pop("default", []))
    return [name for name in features if name not in defaults]


def generate_combinations(features):
    """One `{key, args}` entry per CI job; `args` is a shell-quoted cargo argument string."""

    def job(key, args):
        return {"key": key, "args": shlex.join(args)}

    combinations = [job("default", []), job("no-default", ["--no-default-features"])]
    combinations += [job(feature, ["--no-default-features", "--features", feature]) for feature in features]
    if len(features) > 1:
        combinations.append(job("all-features", ["--all-features"]))
    return combinations


if __name__ == "__main__":
    features = load_features()
    combinations = generate_combinations(features)

    if len(sys.argv) > 1 and sys.argv[1] == "echo":
        print(f"matrix={json.dumps({'case': combinations})}")
    else:
        print(f"Testing {len(combinations)} feature combinations:")
        for job in combinations:
            command = ["cargo", "test", "--package", "gotcha"] + shlex.split(job["args"])
            print(" ".join(command))
            result = subprocess.run(command).returncode
            if result != 0:
                print(f"Test failed for: {job['args'] or 'default features'}")
                sys.exit(-1)
