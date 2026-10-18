#!/bin/sh
# Type-checks the playtest DSL, the games' playtests and the DSL's type tests (dsl.typetest.ts: every invalid question
# must fail to compile, or tsc reports its @ts-expect-error as unused). Prints the errors in these files; exit 1 if any.
#   react/bench/playtest/typecheck.sh [TSC]     (TSC defaults to tsc on PATH)
cd "$(dirname "$0")"
out=$("${1:-tsc}" -p tsconfig.json 2>&1 | grep -E "playtest|typetest")
[ -z "$out" ] && echo "PASS the DSL types (clef's limits refuse every invalid question in dsl.typetest.ts)" && exit 0
echo "$out"; exit 1
