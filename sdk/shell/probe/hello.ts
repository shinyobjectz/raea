import { Bash } from "just-bash/browser";
import { print } from "@raea/motion/native";
export default async function main() {
  const bash = new Bash({ files: { "/data/a.txt": "banana\napple\ncherry\napple\n" } });
  const cmds = [
    "echo hello | tr a-z A-Z",
    "sort /data/a.txt | uniq -c | sort -rn | head -2",
    "for i in 1 2 3; do echo $((i*i)); done | paste -sd+ | bc 2>/dev/null || echo no-bc",
    "echo '{\"a\":[1,2,3]}' | jq '.a | add'",
    "cat /data/a.txt | grep -c apple; wc -l < /data/a.txt",
    "f() { echo \"arg:$1\"; }; f x; ls /data; mkdir -p /w && echo hi > /w/t && cat /w/t",
    "printf '%05.1f|%s\\n' 3.14159 ok; seq 3 | awk '{s+=$1} END {print s}'; echo abc | sed 's/b/B/'",
  ];
  for (const c of cmds) {
    const r = await bash.exec(c);
    print(`$ ${c}\n${r.stdout.trimEnd()}${r.stderr ? "\n[stderr] " + r.stderr.trimEnd() : ""}\n[exit ${r.exitCode}]`);
  }
}
