import SwiftUI

// The sandbox probe (raea docs/media.feature M5): at launch, the WASI tools in Tools/ run through Wasmtime's Pulley
// interpreter over a project folder in the app's caches; the report goes to Documents/report.json and the screen.
// WsxControl is this same app without the sandbox, to measure what it adds.
@main
struct ProbeApp: App {
    @State private var text = "running…"
    var body: some Scene {
        WindowGroup {
            ScrollView { Text(text).font(.system(size: 11, design: .monospaced)).padding() }
                .task { text = await Self.run() }
        }
    }

    static func run() async -> String {
        #if WSX
        let tools = Bundle.main.resourceURL!.appendingPathComponent("Tools").path
        let work = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0].path
        let started = Date()
        let report: String = await Task.detached {
            guard let p = wsx_suite(tools, work) else { return "{}" }
            defer { wsx_free(p) }
            return String(cString: p)
        }.value
        let docs = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
        try? report.write(to: docs.appendingPathComponent("report.json"), atomically: true, encoding: .utf8)
        print("WSX REPORT \(report)")
        return String(format: "%.0f ms\n", Date().timeIntervalSince(started) * 1000) + report
        #else
        return "control"
        #endif
    }
}
