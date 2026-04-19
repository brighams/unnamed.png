#!/usr/bin/env bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

distrobox enter rust-tools -- bash -c "cd '$SCRIPT_DIR/rust' && cargo build --release 2>&1"

exec "$SCRIPT_DIR/rust/target/release/unnamed"
