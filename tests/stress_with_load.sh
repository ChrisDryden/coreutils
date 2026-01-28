#!/bin/bash
# Stress test with background load to simulate CI environment

set -e
cd "$(dirname "$0")/.."

ITERATIONS=${1:-100}

echo "Building..."
cargo build --features dd --no-default-features -q

DD_BIN="./target/debug/coreutils"
FIXTURE_DIR="./tests/fixtures/dd"
TMP_DIR=$(mktemp -d)

cleanup() {
    # Kill all background jobs
    jobs -p | xargs -r kill 2>/dev/null || true
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT

TEST_FILES=(
    "zeros-620f0b67a91f7f74151bc5be745b7110.test"
    "ones-6ae59e64850377ee5470c854761551ea.test"
    "deadbeef-18d99661a1de1fc9af21b0ec2cd67ba3.test"
    "random-5828891cb1230748e146f34223bbd3b5.test"
)

# Start background load generators
echo "Starting background load..."
for i in {1..4}; do
    while true; do dd if=/dev/zero of=/dev/null bs=1M count=10 2>/dev/null; done &
done

# Also do some file I/O stress
for i in {1..2}; do
    while true; do
        dd if=/dev/urandom of="$TMP_DIR/stress_$i" bs=64K count=100 2>/dev/null
        rm -f "$TMP_DIR/stress_$i"
    done &
done

echo "Running $ITERATIONS test iterations under load..."
echo ""

failures=0
for ((iter=1; iter<=ITERATIONS; iter++)); do
    for file in "${TEST_FILES[@]}"; do
        input="$FIXTURE_DIR/$file"
        output="$TMP_DIR/out_${iter}_${file}"

        # Run dd and capture via pipe
        "$DD_BIN" dd "if=$input" 2>/dev/null > "$output"

        if ! cmp -s "$input" "$output"; then
            expected=$(stat -c%s "$input")
            actual=$(stat -c%s "$output")
            echo "FAIL: iter=$iter file=$file expected=$expected actual=$actual"
            ((failures++))
        fi
        rm -f "$output"
    done

    if ((iter % 20 == 0)); then
        echo "Progress: $iter/$ITERATIONS iterations completed"
    fi
done

echo ""
echo "=========================================="
echo "Total: $((ITERATIONS * ${#TEST_FILES[@]})) | Failures: $failures"
echo "=========================================="

[ $failures -eq 0 ] && echo "PASS" || { echo "FAIL"; exit 1; }
