#!/bin/bash
# Stress test using pipes (closer to how test framework works)
# This captures stdout via pipe which is more likely to expose race conditions

set -e

cd "$(dirname "$0")/.."

echo "Building dd..."
cargo build --features dd --no-default-features -q

DD_BIN="./target/debug/coreutils"
FIXTURE_DIR="./tests/fixtures/dd"
TMP_DIR=$(mktemp -d)
trap "rm -rf $TMP_DIR" EXIT

TEST_FILES=(
    "zeros-620f0b67a91f7f74151bc5be745b7110.test"
    "ones-6ae59e64850377ee5470c854761551ea.test"
    "deadbeef-18d99661a1de1fc9af21b0ec2cd67ba3.test"
    "random-5828891cb1230748e146f34223bbd3b5.test"
)

ITERATIONS=${1:-1000}
PARALLEL=${2:-8}

echo "Running pipe-based stress test: $ITERATIONS iterations, $PARALLEL parallel"
echo ""

# Function to run a single test using pipe capture (like test framework)
run_pipe_test() {
    local id=$1
    local file=$2
    local input="$FIXTURE_DIR/$file"
    local output="$TMP_DIR/out_${id}"
    local expected_size=$(stat -c%s "$input")

    # Capture stdout via pipe to a temp file (mimics test framework)
    "$DD_BIN" dd "if=$input" 2>/dev/null | cat > "$output"

    local actual_size=$(stat -c%s "$output")

    if [ "$actual_size" != "$expected_size" ]; then
        echo "FAIL[$id]: $file - size mismatch (expected: $expected_size, got: $actual_size)"
        return 1
    fi

    if ! cmp -s "$input" "$output"; then
        echo "FAIL[$id]: $file - content mismatch"
        return 1
    fi

    rm -f "$output"
    return 0
}

# Run test loop
run_worker() {
    local wid=$1
    local iters=$2
    local fails=0

    for ((i=1; i<=iters; i++)); do
        for file in "${TEST_FILES[@]}"; do
            if ! run_pipe_test "${wid}_${i}" "$file"; then
                ((fails++))
            fi
        done
    done
    echo "$fails"
}

FAIL_FILE="$TMP_DIR/failures"
echo 0 > "$FAIL_FILE"

iters_per_worker=$((ITERATIONS / PARALLEL))
start_time=$(date +%s)

# Launch workers
for ((w=1; w<=PARALLEL; w++)); do
    (
        fails=$(run_worker $w $iters_per_worker)
        flock "$FAIL_FILE" bash -c "echo \$((\$(cat $FAIL_FILE) + $fails)) > $FAIL_FILE"
    ) &
done

wait

end_time=$(date +%s)
FAILURES=$(cat "$FAIL_FILE")
TOTAL=$((ITERATIONS * ${#TEST_FILES[@]}))

echo ""
echo "=========================================="
echo "Total: $TOTAL | Failures: $FAILURES | Time: $((end_time - start_time))s"
echo "=========================================="

[ $FAILURES -eq 0 ] && echo "PASS" || { echo "FAIL"; exit 1; }
