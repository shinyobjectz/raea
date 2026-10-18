import SwiftUI

/// Runs rowlio-sql's CLI on the phone: launch with the CLI's arguments (devicectl … -- render hook.sql -o …).
/// Inputs are copied from the bundle into Documents/probe, which is also the working directory, so outputs land there.

/// The process's physical footprint (what jetsam counts): now and its peak, in MB.
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
struct ProbeApp: App {
    @State private var status = "Running rowlio…"

    var body: some Scene {
        WindowGroup {
            Text(status).font(.headline).padding()
                .onAppear(perform: run)
        }
    }

    private func run() {
        let fm = FileManager.default
        let dir = fm.urls(for: .documentDirectory, in: .userDomainMask)[0].appendingPathComponent("probe")
        try? fm.createDirectory(at: dir, withIntermediateDirectories: true)
        if let inputs = Bundle.main.url(forResource: "inputs", withExtension: nil),
           let names = try? fm.contentsOfDirectory(atPath: inputs.path) {
            for n in names {
                let to = dir.appendingPathComponent(n)
                try? fm.removeItem(at: to)
                try? fm.copyItem(at: inputs.appendingPathComponent(n), to: to)
            }
        }
        // Stylo and Bevy recurse deeply; a secondary thread's 512 KB default isn't enough
        // PROBE_SAMPLE=1: the footprint every 50 ms, to see what the engine's start-up and each phase cost
        if ProcessInfo.processInfo.environment["PROBE_SAMPLE"] != nil {
            print("BASE \(footprint().now) MB")
            let t0 = Date()
            let s = Thread {
                while true {
                    let f = footprint()
                    print(String(format: "MEM %.2fs %d MB", Date().timeIntervalSince(t0), f.now))
                    Thread.sleep(forTimeInterval: 0.05)
                }
            }
            s.start()
        }
        let t = Thread {
            let start = Date()
            dir.path.withCString { rowlio_probe_main($0) }
            let s = String(format: "%.2f", Date().timeIntervalSince(start))
            var info = task_vm_info_data_t()
            var count = mach_msg_type_number_t(MemoryLayout<task_vm_info_data_t>.size / MemoryLayout<natural_t>.size)
            let kr = withUnsafeMutablePointer(to: &info) {
                $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) { task_info(mach_task_self_, task_flavor_t(TASK_VM_INFO), $0, &count) }
            }
            let peak = kr == KERN_SUCCESS ? Double(info.ledger_phys_footprint_peak) / 1_048_576 : -1
            print("PROBE DONE \(s)s peak \(Int(peak)) MB")
            fflush(stdout)
            exit(0)
        }
        t.stackSize = 64 << 20
        t.start()
    }
}
