#!/bin/sh
# The Mac linker for raea's crates (.cargo/config.toml): cc links as always, then an executable it made is signed ad hoc
# with the JIT entitlement, which JavaScriptCore needs before it turns its JIT on (without it every script ran in its
# interpreter: pong's React work 3.3 ms a frame against 0.5 with the JIT). A failed signing never fails the build.
cc "$@" || exit $?
out=""
prev=""
for a in "$@"; do
  case "$a" in
    @*) f="${a#@}"; [ -f "$f" ] && o=$(grep -A1 -x -- '-o' "$f" | tail -1 | tr -d '"') && [ -n "$o" ] && out="$o" ;;
  esac
  [ "$prev" = "-o" ] && out="$a"
  prev="$a"
done
case "$out" in ""|*.dylib|*.so|*.a|*.rlib|*.o) exit 0 ;; esac
if file "$out" 2>/dev/null | grep -q "Mach-O.*executable"; then
  codesign -s - --force --entitlements "$(dirname "$0")/jit.entitlements" "$out" >/dev/null 2>&1 || true
fi
exit 0
