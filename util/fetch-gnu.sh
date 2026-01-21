#!/bin/bash -e
# Clone from master to get the latest GNU tests
repo=https://github.com/coreutils/coreutils
git clone --depth 1 "${repo}" .

# Replace tests not compatible with our binaries
sed -i -e 's/no-mtab-status.sh/no-mtab-status-masked-proc.sh/' -e 's/nproc-quota.sh/nproc-quota-systemd.sh/'  tests/local.mk
# Add tac-continue.sh to root tests (it requires root to mount tmpfs)
# Use sed -i.bak for macOS
sed -i.bak 's|tests/split/l-chunk-root.sh.*|tests/split/l-chunk-root.sh\t\t\t\\\n  tests/tac/tac-continue.sh\t\t\t\\|' tests/local.mk
