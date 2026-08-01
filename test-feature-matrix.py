#!/usr/bin/env python3

"""Feature combinations exercised by CI.

We deliberately do *not* test the powerset. With N optional features that is 2^N - 1 jobs (7
features once meant 127), and almost all of those combinations tell us nothing: `task` and
`prometheus` share no code, so testing them together adds no signal over testing each alone.

What actually catches breakage is: does the crate build with nothing enabled, does each feature
stand on its own, and do they all coexist. That is N + 2 jobs and finds the same bugs — a feature
that forgets a `#[cfg]` fails the "alone" job, and a conflict between two of them fails the "all"
job.
"""

import json
import os
import sys


def load_features():
    """Optional features from gotcha/Cargo.toml, in declaration order."""
    features = []
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
                feature = stripped.split("=")[0].strip()
                # `default` is implied, and `http1` is part of it rather than something to toggle.
                if feature not in ("default", "http1"):
                    features.append(feature)
    return features


def generate_combinations(features):
    # "" is no features at all; then each on its own; then everything together.
    combinations = [""] + list(features)
    if len(features) > 1:
        combinations.append(" ".join(features))
    return combinations


if __name__ == "__main__":
    features = load_features()
    combinations = generate_combinations(features)

    if len(sys.argv) > 1 and sys.argv[1] == "echo":
        print(f"features={json.dumps(combinations)}")
    else:
        print(f"Testing {len(combinations)} feature combinations:")
        for combo in combinations:
            command = f'cargo test --package gotcha --features "{combo}"'
            print(command)
            result = os.system(command)
            if result != 0:
                print(f"Test failed for features: {combo}")
                sys.exit(-1)
