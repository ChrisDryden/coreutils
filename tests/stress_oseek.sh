#!/bin/bash
# Stress test for oseek functionality - the flaky test area

set -e
cd "$(dirname "$0")/.."

echo "Building dd..."
cargo build --features dd --no-default-features -q

DD_BIN="./target/debug/coreutils"
TMP_DIR=$(mktemp -d)
trap "rm -rf $TMP_DIR" EXIT

ITERATIONS=${1:-500}
PARALLEL=${2:-8}

echo "Stress testing oseek with $ITERATIONS iterations, $PARALLEL parallel"

# Expected output: 8 null bytes + "abcdefghijklm"
printf '\0\0\0\0\0\0\0\0abcdefghijklm' > "$TMP_DIR/expected"
expected_size=$(stat -c%s "$TMP_DIR/expected")

run_test() {
    local id=$1
    local output="$TMP_DIR/out_$id"

    # Run dd with oseek - pipe in data and capture stdout
    echo -n "abcdefghijklm" | "$DD_BIN" dd oseek=8 oflag=seek_bytes bs=2 2>/dev/null > "$output"

    local actual_size=$(stat -c%s "$output")

    if [ "$actual_size" != "$expected_size" ]; then
        echo "FAIL[$id]: size mismatch - expected $expected_size, got $actual_size"
        xxd "$output" | head -5
        return 1
    fi

    if ! cmp -s "$TMP_DIR/expected" "$output"; then
        echo "FAIL[$id]: content mismatch"
        echo "Expected:"
        xxd "$TMP_DIR/expected"
        echo "Got:"
        xxd "$output"
        return 1
    fi

    rm -f "$output"
    return 0
}

FAIL_FILE="$TMP_DIR/failures"
echo 0 > "$FAIL_FILE"

iters_per_worker=$((ITERATIONS / PARALLEL))

for ((w=1; w<=PARALLEL; w++)); do
    (
        fails=0
        for ((i=1; i<=iters_per_worker; i++)); do
            if ! run_test "${w}_${i}"; then
                ((fails++))
            fi
        done
        flock "$FAIL_FILE" bash -c "echo \$((\$(cat $FAIL_FILE) + $fails)) > $FAIL_FILE"
    ) &
done

wait

FAILURES=$(cat "$FAIL_FILE")
echo ""
echo "=========================================="
echo "Total: $ITERATIONS | Failures: $FAILURES"
echo "=========================================="
[ $FAILURES -eq 0 ] && echo "PASS" || { echo "FAIL"; exit 1; }
