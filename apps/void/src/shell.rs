//! One terminal per open space: an agent session's @raea/shell (just-bash on raea's V8 over the space folder, with raea,
//! rdb, raea-motion, sqlite3, js-exec and `browser` as commands; sdk/shell/shell.feature) kept alive as
//! `raea-motion shell-serve ROOT`, so a `cd`, an export or an open browser page carries over between the agent's calls.
//! Offered only to procedures that ask for it (a `Tools:` line in prompts/sops/<name>.md).

use anyhow::{anyhow, Context};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

pub struct Terminal {
    root: PathBuf,
    child: tokio::process::Child,
    stdin: tokio::process::ChildStdin,
    stdout: tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
    next: u64,
}

fn motion_bin() -> PathBuf {
    std::env::var_os("RAEA_MOTION").map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sdk/react/target/release/raea-motion"))
}

impl Terminal {
    /// A terminal over `root`, with no network (none is granted by default).
    pub async fn open(root: &Path) -> anyhow::Result<Self> {
        let mut child = tokio::process::Command::new(motion_bin()).arg("shell-serve").arg(root)
            .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null())
            .kill_on_drop(true).spawn().context("raea-motion shell-serve")?;
        let stdin = child.stdin.take().ok_or_else(|| anyhow!("no stdin"))?;
        let mut stdout = BufReader::new(child.stdout.take().ok_or_else(|| anyhow!("no stdout"))?).lines();
        let first = tokio::time::timeout(std::time::Duration::from_secs(60), stdout.next_line()).await
            .map_err(|_| anyhow!("the shell didn't start in 60 s"))??.unwrap_or_default();
        let ready: Value = serde_json::from_str(&first).unwrap_or_default();
        anyhow::ensure!(ready["ready"] == true, "the shell didn't start: {first}");
        Ok(Self { root: root.to_path_buf(), child, stdin, stdout, next: 0 })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// One request, its one reply.
    pub async fn ask(&mut self, mut req: Value, wait_s: u64) -> anyhow::Result<Value> {
        self.next += 1;
        let id = self.next;
        req["id"] = json!(id);
        self.stdin.write_all(format!("{req}\n").as_bytes()).await?;
        self.stdin.flush().await?;
        loop {
            let line = tokio::time::timeout(std::time::Duration::from_secs(wait_s), self.stdout.next_line()).await
                .map_err(|_| anyhow!("the shell gave no answer in {wait_s} s"))??
                .ok_or_else(|| anyhow!("the shell exited"))?;
            let v: Value = serde_json::from_str(&line).unwrap_or_default();
            // `raea` asking the host mid-command (sdk/shell/src/serve.ts): answer it, then keep waiting for the reply
            if let Some(h) = v.get("host") {
                let reply = match Box::pin(crate::host::handle(&self.root, h["op"].as_str().unwrap_or(""), &h["args"])).await {
                    Ok(r) => json!({"host_reply": r}),
                    Err(e) => json!({"host_error": format!("{e:#}")}),
                };
                self.stdin.write_all(format!("{reply}\n").as_bytes()).await?;
                self.stdin.flush().await?;
                continue;
            }
            if v["id"] == json!(id) {
                if let Some(e) = v["error"].as_str() {
                    return Err(anyhow!("{e}"));
                }
                return Ok(v);
            }
        }
    }

    /// A command's output as the agent reads it: stdout, stderr, and the exit when it isn't 0.
    pub async fn exec(&mut self, cmd: &str, timeout_ms: u64) -> anyhow::Result<String> {
        let v = self.ask(json!({"op": "exec", "cmd": cmd, "timeout_ms": timeout_ms}), timeout_ms / 1000 + 120).await?;
        let mut out = format!("{}{}", v["stdout"].as_str().unwrap_or(""), v["stderr"].as_str().unwrap_or(""));
        let code = v["exitCode"].as_i64().unwrap_or(0);
        if code != 0 {
            out += &format!("\n(exit {code})");
        }
        Ok(out)
    }

    pub fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}
