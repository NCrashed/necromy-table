#!/usr/bin/env bash
# The dedicated server for playing with friends (docs/design.md §17.1).
# Listens on 0.0.0.0:7878; friends connect to this machine's address.
# The gods' voice comes from scripts/oracle-server.sh if it runs
# (NECROMY_ORACLE to point elsewhere, --no-oracle to go without).
set -euo pipefail
cd "$(dirname "$0")/.."
exec cargo run --release -p necromy-server -- "$@"
