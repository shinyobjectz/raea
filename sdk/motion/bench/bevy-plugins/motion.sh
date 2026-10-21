#!/bin/sh
# raea-motion with the Bevy plugins compiled in (react's avian, hanabi and tweening features), for the bench harnesses'
# RAEA_MOTION_BIN. Build: (cd react && lane build -- cargo build --release --features avian,hanabi,tweening --bin raea-bevy-sdk)
exec "$(cd "$(dirname "$0")/../../../.." && pwd)/sdk/react/target/release/raea-bevy-sdk" motion "$@"
