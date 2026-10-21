//! Sends one small agent-shaped trace to the collector (Phoenix), then waits for the export.
use serde_json::{json, Map};
fn main() {
    let t = raea_otel::Tracer::new("raea-otel-probe", "void", Map::new());
    {
        let mut turn = t.root("agent.turn");
        turn.kind("AGENT").io("make the title red", "Red it is.");
        let mut call = turn.child("chat MiniMax-M3");
        call.llm("ugc", "MiniMax-M3", 120, 40, 4000, 0, 0.0012).io("system + make the title red", "hook.color = #ff3b30");
        call.end();
        let mut tool = turn.child("execute_tool apply");
        tool.tool("apply", &json!({"rrl": "hook.color = #ff3b30"})).set("output.value", "APPLIED  m4  replaced rule hook.color");
        tool.end();
        println!("trace {}", turn.trace_id());
    }
    std::thread::sleep(std::time::Duration::from_secs(2));
}
