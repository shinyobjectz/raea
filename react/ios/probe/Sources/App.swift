import SwiftUI

// @raea/motion on the iPhone (raea/react/ios): at launch, Bundle/motion.js (a TypeScript program bundled by
// `raea-motion bundle`) runs on JavaScriptCore (jitless) with the native calls (Manifold solids, ECS tables); the
// report goes to Documents/report.json and the screen.
//   PROBE_FRAMES=0,150,...  (launch environment) frames mode: each composition's listed frames drawn to
//                           Documents/NAME-fN.png, each timed; no .mp4 renders and no motion.js unless PROBE_MP4=1
//   PROBE_COMP=NAME         only NAME.comp.js (other sessions' compositions in Bundle/ are skipped)
// The report carries the process's peak physical footprint (what jetsam counts) as "peak_mb".

/// The process's physical footprint now and its peak, in MB.
func footprint() -> (now: Int, peak: Int) {
    var info = task_vm_info_data_t()
    var count = mach_msg_type_number_t(MemoryLayout<task_vm_info_data_t>.size / MemoryLayout<natural_t>.size)
    let kr = withUnsafeMutablePointer(to: &info) {
        $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) { task_info(mach_task_self_, task_flavor_t(TASK_VM_INFO), $0, &count) }
    }
    guard kr == KERN_SUCCESS else { return (-1, -1) }
    return (Int(info.phys_footprint >> 20), Int(info.ledger_phys_footprint_peak >> 20))
}

@main
struct MotionProbeApp: App {
    @State private var text = "running…"
    var body: some Scene {
        WindowGroup {
            ScrollView { Text(text).font(.system(size: 11, design: .monospaced)).padding() }
                .task { text = await Self.run() }
        }
    }

    static func run() async -> String {
        let bundle = Bundle.main.resourceURL!.appendingPathComponent("Bundle/motion.js").path
        // a program that works on media finds Bundle/clip.mp4 (if bundled) beside it in Documents, its working folder
        let docs0 = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
        let clip = Bundle.main.resourceURL!.appendingPathComponent("Bundle/clip.mp4")
        if FileManager.default.fileExists(atPath: clip.path) {
            let dest = docs0.appendingPathComponent("clip.mp4")
            try? FileManager.default.removeItem(at: dest)
            try? FileManager.default.copyItem(at: clip, to: dest)
        }
        FileManager.default.changeCurrentDirectoryPath(docs0.path)
        let bundleDir = Bundle.main.resourceURL!.appendingPathComponent("Bundle")
        // tests find their fixtures under RAEA_ROOT (media/fixtures/...), the bundle's copy
        setenv("RAEA_ROOT", bundleDir.path, 1)
        // jitless: iOS gives an app no JIT anyway; said to JavaScriptCore too, before its first context
        setenv("JSC_useJIT", "false", 1)
        let env = ProcessInfo.processInfo.environment
        let frames = (env["PROBE_FRAMES"] ?? "").split(separator: ",").compactMap { Int64($0.trimmingCharacters(in: .whitespaces)) }
        let only = env["PROBE_COMP"]
        let full = frames.isEmpty || env["PROBE_MP4"] == "1"
        // the screen stays on while it works: a locked phone suspends the app, and the run is lost
        await MainActor.run { UIApplication.shared.isIdleTimerDisabled = true }
        let reportURL = docs0.appendingPathComponent("report.json")
        let report: String = await Task.detached {
            // compositions (NAME.comp.js) rendered to Documents/NAME.mp4; a program (motion.js) run
            var parts: [String] = []
            // written after each item, so a run that stops partway still says how far it got
            let save = { (done: Bool) in
                let mem = "\"peak_mb\": \(footprint().peak), \"jsc_useJIT\": \"\(ProcessInfo.processInfo.environment["JSC_useJIT"] ?? "unset")\""
                let body = "{" + (parts + [mem] + (done ? [] : ["\"running\": true"])).joined(separator: ", ") + "}"
                try? body.write(to: reportURL, atomically: true, encoding: .utf8)
            }
            save(false)
            let names = ((try? FileManager.default.contentsOfDirectory(atPath: bundleDir.path)) ?? []).filter { $0.hasSuffix(".comp.js") }
                .filter { only == nil || $0 == only! + ".comp.js" }.sorted()
            for n in names where !frames.isEmpty {
                var each: [String] = []
                for f in frames {
                    let out = docs0.appendingPathComponent(String(n.dropLast(".comp.js".count)) + "-f\(f).png").path
                    if let p = raea_motion_frame(bundleDir.appendingPathComponent(n).path, bundleDir.path, out, f) {
                        each.append("\"\(f)\": " + String(cString: p))
                        raea_motion_free(p)
                    }
                }
                parts.append("\"\(n) frames\": {" + each.joined(separator: ", ") + "}")
                save(false)
            }
            for n in names where full {
                let out = docs0.appendingPathComponent(String(n.dropLast(".comp.js".count)) + ".mp4").path
                if let p = raea_motion_render(bundleDir.appendingPathComponent(n).path, bundleDir.path, out) {
                    parts.append("\"\(n)\": " + String(cString: p))
                    raea_motion_free(p)
                }
                save(false)
            }
            if full, FileManager.default.fileExists(atPath: bundle), let p = raea_motion_run(bundle) {
                parts.append("\"motion.js\": " + String(cString: p))
                raea_motion_free(p)
            }
            save(true)
            return "{" + (parts + ["\"peak_mb\": \(footprint().peak)"]).joined(separator: ", ") + "}"
        }.value
        await MainActor.run { UIApplication.shared.isIdleTimerDisabled = false }
        return report
    }
}
