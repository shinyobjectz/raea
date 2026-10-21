import SwiftUI

/// Times the HDC sandwich's layers on the phone (ai/meaning/src/hdc_time.rs over k.rs, the code `raea-meaning hdc-time`
/// runs on the Mac). The bundled column is one corpus clip's SigLIP frames (inputs/col.f32, 768 floats a frame).
/// Prints `HDC_JSON {...}` to the console (devicectl device process launch --console) for each of three runs.
@main
struct HdcApp: App {
    @State private var status = "Timing HDC layers…"
    var body: some Scene {
        WindowGroup {
            ScrollView { Text(status).font(.system(.footnote, design: .monospaced)).padding() }
                .onAppear {
                    Thread {
                        if CommandLine.arguments.contains("--encode") {
                            let r = (try? Encode.run()) ?? "{\"error\": \"encode failed\"}"
                            print("ENC_JSON \(r)")
                            DispatchQueue.main.async { status = r }
                            return
                        }
                        let path = Bundle.main.path(forResource: "inputs/col", ofType: "f32")!
                        var last = ""
                        for i in 1...3 {
                            let p = raea_hdc_time(path, 768)!
                            last = String(cString: p); free(p)
                            print("HDC_JSON run \(i) \(last)")
                        }
                        DispatchQueue.main.async { status = last }
                    }.start()
                }
        }
    }
}
