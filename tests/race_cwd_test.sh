#!/bin/bash
# Aggressively test the cwd race condition
# This simulates what happens when test_du_long_path_from_unreadable changes cwd

cd "$(dirname "$0")/.."

echo "Building..."
cargo build --features dd --no-default-features -q 2>/dev/null

TMP_DIR=$(mktemp -d)
trap "rm -rf $TMP_DIR" EXIT

# Create a directory to change to (simulating test_du behavior)
mkdir -p "$TMP_DIR/wrong_cwd"

echo "Starting cwd-changing background process..."
# Background process that rapidly changes cwd (simulating test_du)
(
    while true; do
        cd "$TMP_DIR/wrong_cwd" 2>/dev/null
        sleep 0.001
        cd /tmp 2>/dev/null
        sleep 0.001
    done
) &
CWD_CHANGER_PID=$!

cleanup() {
    kill $CWD_CHANGER_PID 2>/dev/null
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT

echo "Running dd tests repeatedly while cwd is being changed..."
FAILURES=0
for i in {1..30}; do
    echo -n "Run $i: "
    if cargo test --features dd --no-default-features --test tests test_dd::test_identity -- --test-threads=4 2>&1 | grep -q "FAILED\|panicked"; then
        echo "FAILED!"
        ((FAILURES++))
    else
        echo "ok"
    fi
done

echo ""
echo "=========================================="
echo "Failures: $FAILURES / 30"
echo "=========================================="
