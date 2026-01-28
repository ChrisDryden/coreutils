#!/bin/bash
# Stress test to reproduce intermittent test_identity failures
# This script runs the test repeatedly with various stress conditions

set -e

cd "$(dirname "$0")/.."

# Build the dd binary first
echo "Building dd..."
cargo build --features dd --no-default-features -q

DD_BIN="./target/debug/coreutils"
FIXTURE_DIR="./tests/fixtures/dd"
TMP_DIR=$(mktemp -d)
trap "rm -rf $TMP_DIR" EXIT

# Test files from test_identity
TEST_FILES=(
    "zeros-620f0b67a91f7f74151bc5be745b7110.test"
    "ones-6ae59e64850377ee5470c854761551ea.test"
    "deadbeef-18d99661a1de1fc9af21b0ec2cd67ba3.test"
    "random-5828891cb1230748e146f34223bbd3b5.test"
)

ITERATIONS=${1:-1000}
PARALLEL=${2:-4}
FAILURES=0
TOTAL=0

echo "Running stress test with $ITERATIONS iterations, $PARALLEL parallel processes"
echo "Temp dir: $TMP_DIR"
echo ""

# Function to run a single test
run_single_test() {
    local id=$1
    local file=$2
    local input="$FIXTURE_DIR/$file"
    local output="$TMP_DIR/out_${id}_$$"

    # Run dd and capture stdout to a file
    "$DD_BIN" dd "if=$input" > "$output" 2>/dev/null

    # Compare output with input
    if ! cmp -s "$input" "$output"; then
        local input_size=$(stat -c%s "$input")
        local output_size=$(stat -c%s "$output")
        echo "FAIL: $file (input: $input_size bytes, output: $output_size bytes)"
        return 1
    fi

    rm -f "$output"
    return 0
}

# Function to run tests in a loop
run_test_loop() {
    local worker_id=$1
    local iterations=$2
    local failures=0

    for ((i=1; i<=iterations; i++)); do
        for file in "${TEST_FILES[@]}"; do
            if ! run_single_test "${worker_id}_${i}" "$file"; then
                ((failures++))
            fi
        done
    done

    echo "$failures"
}

# Run tests in parallel
echo "Starting parallel test runs..."
start_time=$(date +%s)

# Create a file to collect failure counts
FAIL_FILE="$TMP_DIR/failures"
echo 0 > "$FAIL_FILE"

# Calculate iterations per worker
iters_per_worker=$((ITERATIONS / PARALLEL))

# Launch parallel workers
for ((w=1; w<=PARALLEL; w++)); do
    (
        fails=$(run_test_loop $w $iters_per_worker)
        # Atomic add to failure count
        flock "$FAIL_FILE" bash -c "echo \$((\$(cat $FAIL_FILE) + $fails)) > $FAIL_FILE"
    ) &
done

# Wait for all workers
wait

end_time=$(date +%s)
duration=$((end_time - ${start_time%.*}))

FAILURES=$(cat "$FAIL_FILE")
TOTAL=$((ITERATIONS * ${#TEST_FILES[@]}))

echo ""
echo "=========================================="
echo "Stress test complete"
echo "Total tests: $TOTAL"
echo "Failures: $FAILURES"
echo "Duration: ${duration}s"
echo "=========================================="

if [ $FAILURES -gt 0 ]; then
    echo "RESULT: FAILURES DETECTED!"
    exit 1
else
    echo "RESULT: All tests passed"
    exit 0
fi
