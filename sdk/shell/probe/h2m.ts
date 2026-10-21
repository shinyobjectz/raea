import { Shell } from "../src";
import { print } from "@raea/motion/native";
export default async function main() {
  const sh = new Shell({ root: "/tmp/h2m" + Date.now() });
  for (const c of ["echo '<h1>Hi</h1><p>a <b>b</b></p>' | html-to-markdown", "echo hello > a.txt && tar -czf a.tgz a.txt && tar -tzf a.tgz", "tar -cf b.tar a.txt && tar -tf b.tar", "echo x | gzip | gunzip"]) {
    const r = await sh.exec(c);
    print(`$ ${c} -> ${r.exitCode}\n${r.stdout}[err] ${r.stderr}`);
  }
}
