#!/usr/bin/env python3

# Define feature list
import json
import os
import sys

# Load features from Cargo.toml
def load_features():
    features = []
    with open("gotcha/Cargo.toml", "r") as f:
        cargo_toml = f.read()

        # Extract features from [features] section
        in_features = False
        for line in cargo_toml.split('\n'):
            if line.startswith('[features]'):
                in_features = True
                continue
            elif line.startswith('['):
                in_features = False
            elif in_features and '=' in line:
                feature = line.split('=')[0].strip()
                if feature != 'default':
                    features.append(feature)
    return features

def generate_combinations(features):
    n = len(features)
    combinations = []
    
    # Generate all combinations
    for i in range(1, 1 << n):
        combo = []
        for j in range(n):
            if (i & (1 << j)) != 0:
                combo.append(features[j])
        combinations.append(" ".join(combo))
        
    return combinations

if __name__ == "__main__":
    features = load_features()
    features = [feature for feature in features if feature not in ["cloudflare_worker", "http1"]]

    if len(sys.argv) > 1 and sys.argv[1] == "echo":
        combinations = generate_combinations(features)
        print(f"features={json.dumps(combinations)}")
    elif len(sys.argv) > 1 and sys.argv[1] == "echo-cf-worker":
        features = ["cors", "openapi", "cloudflare_worker"]
        combinations = generate_combinations(features)
        print(f"features={json.dumps(combinations)}")
    else:
        # Get combinations
        combinations = generate_combinations(features)
        # Print combination matrix
        print("Feature Combinations:")
        for combo in combinations:
            command = f"cargo test --package gotcha --features \"{combo}\""
            print(command)
            result = os.system(command)
            if result != 0:
                print(f"Test failed for features: {combo}")
                sys.exit(-1)
                break
