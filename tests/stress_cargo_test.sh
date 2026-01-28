#!/bin/bash
# Stress test using actual cargo test framework
# This is the most accurate reproduction of the CI environment

set -e
cd "$(dirname "$0")/.."

ITERATIONS=${1:-100}
PARALLEL_TESTS=${2:-4}

echo "Running cargo test stress: $ITERATIONS iterations with --test-threads=$PARALLEL_TESTS"
echo ""

failures=0
for ((i=1; i<=ITERATIONS; i++)); do
    echo -n "Run $i/$ITERATIONS: "
    if cargo test --features dd --no-default-features --test tests test_dd::test_identity \
        -- --test-threads=$PARALLEL_TESTS 2>&1 | grep -q "FAILED\|panicked"; then
        echo "FAILED!"
        ((failures++))
    else
        echo "ok"
    fi
done

echo ""
echo "=========================================="
echo "Total runs: $ITERATIONS | Failures: $failures"
echo "=========================================="

[ $failures -eq 0 ] && echo "PASS" || { echo "FAIL"; exit 1; }
