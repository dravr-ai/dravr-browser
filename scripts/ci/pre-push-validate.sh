#!/bin/bash
# ABOUTME: Pre-push gate for dravr-browser: the shared satellite gate from dravr-build-config
# ABOUTME: Sets browser's cargo features and execs .build/validation/satellite-pre-push-validate.sh
#
# SPDX-License-Identifier: MIT OR Apache-2.0
# Copyright (c) 2026 dravr.ai

cd "$(dirname "${BASH_SOURCE[0]}")/../.." || exit 1
GATE=.build/validation/satellite-pre-push-validate.sh
if [ ! -f "$GATE" ]; then
    echo "BLOCKED: $GATE is missing. Run: git submodule update --init --recursive .build"
    exit 1
fi
# The crate declares no features, so --all-features builds what CI builds.
SATELLITE_FEATURES="--all-features" exec bash "$GATE" "$@"
