#!/bin/zsh
# just-bash's own test suite on raea's V8: the upstream source at the pinned tag (cloned outside the repo), its vitest
# files bundled the way upstream bundles the browser build (__BROWSER__, node:async_hooks/dns/module to its own
# browser-unsupported shim) by raea-motion test, one run per folder, each folder's summary line and failures kept.
#   sdk/shell/tools/upstream.sh SRC_DIR [OUT_DIR]     SRC_DIR = .../just-bash/packages/just-bash (with `bun install --ignore-scripts`)
# The parts that need Node by design (python3 and sqlite3 workers, js-exec's QuickJS, the real-disk filesystems, the
# CLI) are listed, not run: in raea those commands are native (shell.feature).
set -u
src=${1:?SRC_DIR}; out=${2:-$PWD/upstream-out}
M=${MOTION:-${0:A:h}/../../react/target/release/raea-motion}
mkdir -p $out; : > $out/summary.txt
cd $src
U=src/shims/browser-unsupported.js
JB=(--define __BROWSER__=true --alias node:async_hooks=$U --alias node:dns=$U --alias node:module=$U --node-stub url,worker_threads,fs,os,child_process,crypto,util,events,stream,buffer,process,net,http,https,tty,readline,vm,perf_hooks,string_decoder,fs/promises,zlib)
NODE_ONLY='python3|sqlite3|js-exec|read-write-fs|overlay-fs|/cli|real-fs|worker|browser.bundle|spec-tests|curl|network'
dirs=($(find src -name '*.test.ts' -not -path '*/node_modules/*' | sed 's#/[^/]*$##' | sort -u))
for d in $dirs; do
  files=($d/*.test.ts)
  if print -r -- $d | grep -qE "$NODE_ONLY"; then print "NODE   $d (${#files} files)" >> $out/summary.txt; continue; fi
  log=$out/${d//\//_}.log
  $M test $files $JB --timeout 180 > $log 2>&1
  s=$(grep -E '^[0-9]+ pass, ' $log | tail -1)
  if [ -z "$s" ] && grep -q "couldn't bundle" $log; then
    # a file that needs Node refuses the folder's bundle: file by file, so only those are left out
    p=0; fl=0; sk=0; nodef=0
    for f in $files; do
      flog=$out/${f//\//_}.log
      $M test $f $JB --timeout 180 > $flog 2>&1
      fs=$(grep -E '^[0-9]+ pass, ' $flog | tail -1)
      if [ -n "$fs" ]; then
        p=$((p + ${${(s: :)fs}[1]})); fl=$((fl + ${${(s: :)fs}[3]})); sk=$((sk + ${${(s: :)fs}[5]}))
      elif grep -q "couldn't bundle" $flog; then nodef=$((nodef + 1)); print "NODE   $f: $(grep -m1 -oE 'error: [^|]{0,120}' $flog)" >> $out/node-files.txt
      else print "STUCK  $f: $(grep -m1 -oE 'REFUSED.{0,120}' $flog)" >> $out/node-files.txt; fi
    done
    s="$p pass, $fl fail, $sk skipped ($nodef files need Node)"
  fi
  [ -z "$s" ] && s="REFUSED $(grep -m1 -oE 'REFUSED.{0,160}' $log)"
  print "RUN    $d (${#files} files): $s" >> $out/summary.txt
done
print -- "--- totals"
grep -oE '[0-9]+ pass, [0-9]+ fail, [0-9]+ skipped' $out/summary.txt | awk '{p+=$1; f+=$3; s+=$5} END {print p" pass, "f" fail, "s" skipped"}'
grep -c REFUSED $out/summary.txt | xargs echo "folders refused:"
grep -oE '\([0-9]+ files need Node\)' $out/summary.txt | grep -oE '[0-9]+' | awk '{n+=$1} END {print n+0" test files need Node (listed in node-files.txt)"}' 
