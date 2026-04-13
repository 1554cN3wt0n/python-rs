#!/bin/bash
set -e

EXAMPLES_DIR="examples"
FAILED_EXAMPLES=()

for example in "$EXAMPLES_DIR"/*.pyrs; do
    echo "Running $example..."
    if ! cargo run --quiet -- "$example"; then
        echo "FAILED: $example"
        FAILED_EXAMPLES+=("$example")
    else
        echo "PASSED: $example"
    fi
    echo "-----------------------------------"
done

if [ ${#FAILED_EXAMPLES[@]} -ne 0 ]; then
    echo "The following examples failed:"
    for failed in "${FAILED_EXAMPLES[@]}"; do
        echo "  - $failed"
    done
    exit 1
else
    echo "All examples passed!"
fi
