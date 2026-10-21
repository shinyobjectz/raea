//! `void runner`: this Mac as a runner for a check service's queue (RAEA_API). It claims jobs on
//! our own and eval footage (the service refuses any other), runs them, keeps the lease alive, and reports back.
//! Today a job is an eval run (`void eval` with the job's scenarios, procedure and run name); bench and render jobs
//! come when they're needed. Capabilities say what this machine has: apple_vision and gpu on a Mac, so jobs that need
//! Apple's perception only go to Macs until perception has portable backends.

use anyhow::{anyhow, Context};
use serde_json::{json, Value};

const HEARTBEAT_S: u64 = 60;

async fn token() -> anyhow::Result<String> {
    if let Ok(t) = std::env::var("RAEA_RUNNER_TOKEN") {
        return Ok(t);
    }
    crate::brain::api_session().await.context("no RAEA_RUNNER_TOKEN and no eval session")
}

async fn post(api: &str, token: &str, path: &str, body: Value) -> anyhow::Result<Value> {
    let res = reqwest::Client::new().post(format!("{}{path}", api.trim_end_matches('/'))).bearer_auth(token).json(&body)
        .timeout(std::time::Duration::from_secs(60)).send().await?;
    let status = res.status();
    let v: Value = res.json().await.unwrap_or(Value::Null);
    anyhow::ensure!(status.is_success(), "{path}: {status} {}", v["error"]);
    Ok(v)
}

/// Claim and run jobs until stopped (`once`: one claim, for tests and cron).
pub async fn run(api: &str, name: &str, once: bool) -> anyhow::Result<()> {
    let tok = token().await?;
    let caps = if cfg!(target_vendor = "apple") { vec!["apple_vision", "gpu"] } else { vec![] };
    let platform = if cfg!(target_vendor = "apple") { "mac" } else { "linux" };
    loop {
        let got = post(api, &tok, "/v1/runners/claim", json!({"runner": name, "platform": platform, "capabilities": caps})).await?;
        let job = got["job"].clone();
        if job.is_null() {
            if once { println!("no job"); return Ok(()); }
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            continue;
        }
        let id = job["id"].as_str().unwrap_or("").to_string();
        println!("job {id}: {} {}", job["kind"], job["spec"]);
        // the lease: a heartbeat every minute while it runs
        let (a, t, i, n) = (api.to_string(), tok.clone(), id.clone(), name.to_string());
        let beat = tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(HEARTBEAT_S)).await;
                if post(&a, &t, &format!("/v1/runners/jobs/{i}/heartbeat"), json!({"runner": n})).await.is_err() { break }
            }
        });
        let outcome = match job["kind"].as_str() {
            Some("eval") => eval_job(&job["spec"]).await,
            other => Err(anyhow!("this runner doesn't run {other:?} jobs yet")),
        };
        beat.abort();
        let (status, result) = match outcome {
            Ok(r) if r["exit"].as_i64().is_some_and(|c| c != 0) => ("failed", r),
            Ok(r) => ("done", r),
            Err(e) => ("failed", json!({"error": e.to_string()})),
        };
        post(api, &tok, &format!("/v1/runners/jobs/{id}/done"), json!({"runner": name, "status": status, "result": result})).await?;
        println!("job {id}: {status}");
        if once { return Ok(()); }
    }
}

/// An eval job: {scenarios: a folder or feature file on this machine, sop?, run, jobs?, rounds?}. Runs `void eval` as a
/// child (its own process, so a crash fails the job, not the runner) and returns its summary and results file.
async fn eval_job(spec: &Value) -> anyhow::Result<Value> {
    let scenarios = spec["scenarios"].as_str().ok_or_else(|| anyhow!("spec.scenarios is required"))?;
    let run = spec["run"].as_str().filter(|r| r.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))).ok_or_else(|| anyhow!("spec.run: letters, digits, - _ ."))?;
    let exe = std::env::current_exe()?;
    let mut cmd = tokio::process::Command::new(exe);
    cmd.arg("eval").arg(scenarios).arg("--run").arg(run);
    if let Some(s) = spec["sop"].as_str() { cmd.arg("--sop").arg(s); }
    cmd.arg("--jobs").arg(spec["jobs"].as_u64().unwrap_or(3).to_string());
    if let Some(r) = spec["rounds"].as_u64() { cmd.env("RAEA_VOID_ROUNDS", r.to_string()); }
    let out = cmd.output().await?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let summary: Vec<&str> = text.lines().rev().take(12).collect::<Vec<_>>().into_iter().rev().collect();
    let results = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("evals/results").join(format!("{run}.jsonl"));
    Ok(json!({"exit": out.status.code(), "summary": summary.join("\n"), "results": results, "stderr": String::from_utf8_lossy(&out.stderr).chars().rev().take(2000).collect::<String>().chars().rev().collect::<String>()}))
}
