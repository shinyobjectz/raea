import AVFoundation
import CoreML
import CoreGraphics

/// The whole clip encoded on the phone (the roadmap Track L, part 7): frames at 8 fps through the fused Core ML model
/// (SigLIP2SeenBits: SigLIP 2 then k.rs's projection; SEEN bits are the signs), the sound through YAMNet's Core ML
/// package in 10 s chunks every 9.6 s (20 frames of 0.48 s, aligned with the ONNX path), then record.rs (raea_encode).
enum Encode {
    static let fps = 8.0

    static func model(_ name: String) throws -> MLModel {
        let url = Bundle.main.url(forResource: name, withExtension: "mlmodelc")!
        let cfg = MLModelConfiguration(); cfg.computeUnits = .all
        return try MLModel(contentsOf: url, configuration: cfg)
    }

    /// 256 x 256 BGRA, squashed (as ffmpeg's scale=256:256 on the Mac)
    static func pixelBuffer(_ img: CGImage) -> CVPixelBuffer {
        var pb: CVPixelBuffer?
        CVPixelBufferCreate(nil, 256, 256, kCVPixelFormatType_32BGRA, [kCVPixelBufferCGImageCompatibilityKey: true] as CFDictionary, &pb)
        CVPixelBufferLockBaseAddress(pb!, [])
        let ctx = CGContext(data: CVPixelBufferGetBaseAddress(pb!), width: 256, height: 256, bitsPerComponent: 8,
                            bytesPerRow: CVPixelBufferGetBytesPerRow(pb!), space: CGColorSpaceCreateDeviceRGB(),
                            bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue | CGBitmapInfo.byteOrder32Little.rawValue)!
        ctx.interpolationQuality = .medium
        ctx.draw(img, in: CGRect(x: 0, y: 0, width: 256, height: 256))
        CVPixelBufferUnlockBaseAddress(pb!, [])
        return pb!
    }

    /// a 256 x 256 BGRA buffer's bytes, row by row
    static func raw(_ pb: CVPixelBuffer) -> Data {
        CVPixelBufferLockBaseAddress(pb, .readOnly); defer { CVPixelBufferUnlockBaseAddress(pb, .readOnly) }
        let base = CVPixelBufferGetBaseAddress(pb)!.assumingMemoryBound(to: UInt8.self), stride = CVPixelBufferGetBytesPerRow(pb)
        var d = Data(capacity: 256 * 256 * 4)
        for y in 0..<256 { d.append(base + y * stride, count: 256 * 4) }
        return d
    }

    static func pcm16k(_ asset: AVAsset) throws -> [Float] {
        guard let track = asset.tracks(withMediaType: .audio).first else { return [] }
        let reader = try AVAssetReader(asset: asset)
        let out = AVAssetReaderTrackOutput(track: track, outputSettings: [
            AVFormatIDKey: kAudioFormatLinearPCM, AVSampleRateKey: 16000, AVNumberOfChannelsKey: 1,
            AVLinearPCMBitDepthKey: 32, AVLinearPCMIsFloatKey: true, AVLinearPCMIsNonInterleaved: false, AVLinearPCMIsBigEndianKey: false])
        reader.add(out); reader.startReading()
        var pcm: [Float] = []
        while let sb = out.copyNextSampleBuffer(), let bb = CMSampleBufferGetDataBuffer(sb) {
            var len = 0; var ptr: UnsafeMutablePointer<CChar>?
            CMBlockBufferGetDataPointer(bb, atOffset: 0, lengthAtOffsetOut: nil, totalLengthOut: &len, dataPointerOut: &ptr)
            ptr!.withMemoryRebound(to: Float.self, capacity: len / 4) { pcm.append(contentsOf: UnsafeBufferPointer(start: $0, count: len / 4)) }
        }
        return pcm
    }

    static func run() throws -> String {
        let t0 = Date()
        let url = Bundle.main.url(forResource: "inputs/clip", withExtension: "mp4")!
        let asset = AVURLAsset(url: url)
        let n = Int(floor(asset.duration.seconds * fps))
        let gen = AVAssetImageGenerator(asset: asset)
        gen.requestedTimeToleranceBefore = .zero; gen.requestedTimeToleranceAfter = .zero
        gen.appliesPreferredTrackTransform = true
        let sig = try model("SigLIP2SeenBits")
        var vec = [Float](); vec.reserveCapacity(n * 768)
        var seen = [UInt64](repeating: 0, count: n * 128)
        var tSee = 0.0
        var probes: [(Int, Data)] = []
        for k in 0..<n {
            let img = try gen.copyCGImage(at: CMTime(seconds: Double(k) / fps, preferredTimescale: 600), actualTime: nil)
            let pb = pixelBuffer(img)
            if [0, 40, 120].contains(k) { probes.append((k, raw(pb))) }
            let s0 = Date()
            let o = try sig.prediction(from: MLDictionaryFeatureProvider(dictionary: ["image": MLFeatureValue(pixelBuffer: pb)]))
            tSee += Date().timeIntervalSince(s0)
            let p = o.featureValue(for: "projection")!.multiArrayValue!, v = o.featureValue(for: "vector")!.multiArrayValue!
            for b in 0..<8192 where p[b].floatValue > 0 { seen[k * 128 + b / 64] |= UInt64(1) << UInt64(b % 64) }
            var row = (0..<768).map { v[$0].floatValue }
            let norm = max(sqrt(row.reduce(0) { $0 + $1 * $1 }), 1e-12); row = row.map { $0 / norm }
            vec.append(contentsOf: row)
        }
        // sound: YAMNet in 10 s chunks every 9.6 s; rows as the ONNX path counts them
        let pcm = try pcm16k(asset)
        let rows = pcm.count >= 15600 ? (pcm.count - 15600) / 7680 + 1 : (pcm.isEmpty ? 0 : 1)
        var yam = [Float](); var tHear = 0.0
        if rows > 0 {
            let ym = try model("YAMNet")
            var off = 0
            while yam.count / 1024 < rows {
                let wave = try MLMultiArray(shape: [160000], dataType: .float32)
                for i in 0..<160000 { wave[i] = NSNumber(value: off + i < pcm.count ? pcm[off + i] : 0) }
                let h0 = Date()
                let e = try ym.prediction(from: MLDictionaryFeatureProvider(dictionary: ["waveform": wave])).featureValue(for: "embeddings")!.multiArrayValue!
                tHear += Date().timeIntervalSince(h0)
                for r in 0..<20 where yam.count / 1024 < rows { for c in 0..<1024 { yam.append(e[[r, c] as [NSNumber]].floatValue) } }
                off += 153600
            }
        }
        let dir = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0].appendingPathComponent("enc")
        try? FileManager.default.removeItem(at: dir); try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        // the inputs and vectors behind SEEN, to tell the picture path from the model's drift (compare.py vectors)
        vec.withUnsafeBufferPointer { try? Data(buffer: $0).write(to: dir.appendingPathComponent("sig.f32")) }
        for (k, d) in probes { try? d.write(to: dir.appendingPathComponent("frame\(k).bgra")) }
        let e0 = Date()
        let res = vec.withUnsafeBufferPointer { vp in seen.withUnsafeBufferPointer { sp in yam.withUnsafeBufferPointer { yp in
            raea_encode(vp.baseAddress, n, sp.baseAddress, yp.baseAddress, yam.count / 1024, fps, dir.path)!
        } } }
        let json = String(cString: res); free(res)
        let total = Date().timeIntervalSince(t0)
        return "{\"moments\": \(n), \"yamnet_rows\": \(yam.count / 1024), \"siglip_ms_per_frame\": \(String(format: "%.2f", tSee * 1000 / Double(max(n, 1)))), \"yamnet_ms_per_chunk\": \(String(format: "%.1f", tHear * 1000 / max(1, ceil(Double(rows) / 20)))), \"encode_ms\": \(String(format: "%.1f", Date().timeIntervalSince(e0) * 1000)), \"total_s\": \(String(format: "%.2f", total)), \"rust\": \(json)}"
    }
}
