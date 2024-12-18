#!/usr/bin/env python3

# Define feature list
import os
import sys


features = ["openapi", "prometheus", "cors"]

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

# Get combinations
combinations = generate_combinations(features)

# Print combination matrix
print("Feature Combinations:")
for combo in combinations:
    command = f"cargo test --package gotcha --no-default-features --features \"{combo}\""
    print(command)
    result = os.system(command)
    if result != 0:
        print(f"Test failed for features: {combo}")
        sys.exit(-1)
        break
