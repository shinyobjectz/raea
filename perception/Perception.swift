// Perception: a photo, video or sound read the way an editor reads footage before using it, with what the OS
// already knows how to see and hear (Vision, Sound Analysis; no models shipped). Ported from moonsplice's
// native/apple tool and its fact-sheet design (.robot/docs/factsheet.robot): a video becomes its shots and how
// each one begins (cut, dissolve, fade), each shot's framing, faces, subject, scene labels and look, the words on
// screen over time, and what's heard (speech, music, silence). The vocabulary is closed so labels stay consistent.
//
// Pure Foundation/AVFoundation/Vision (no UIKit): the app's `ugc analyze` (AgentCommands) and the Mac tool the
// evals use (agent/tools/perceive) compile this same file, so both read footage identically.

import AVFoundation
import CoreGraphics
import CoreImage
import CoreML
import Foundation
import ImageIO
import SoundAnalysis
import UniformTypeIdentifiers
import Vision

enum Perception {
    /// Bumped when the facts change shape or meaning: older facts files are made again.
    static let version = 2

    struct Facts: Codable {
        var version = Perception.version
        var file: String
        /// video | photo | sound
        var kind: String
        var width: Int?
        var height: Int?
        var seconds: Double?
        /// Whether a video has a sound track.
        var audio: Bool?
        var shots: [Shot] = []
        /// Words on screen, each with when it's seen (the sampled moments it was read at).
        var text: [Words] = []
        /// What's heard: speech | music | singing | laughter | applause | silence | noise, in spans.
        var sound: [Span] = []
        /// Apple's scene labels for the whole thing, most common first.
        var scene: [String] = []
        /// The creator's performance each second (eye contact, blinks, energy, speaking, reading) and the lip-sync
        /// offset, when a face is in the footage and the face models are in the build (T-332). Derived numbers only.
        var performance: Performance?
    }

    struct Shot: Codable {
        var start: Double
        var end: Double
        /// How it begins: start | cut | dissolve | fade (through black).
        var transition: String
        /// close-up | medium | wide | no people (from the biggest face, or a body without one).
        var framing: String
        var faces: Int
        /// The most attention-grabbing region, [x, y, w, h] in 0...1 from the top left.
        var subject: [Double]?
        /// still | moving (how much the picture changes inside the shot).
        var motion: String
        /// Mean brightness, 0...1.
        var brightness: Double
        /// Apple's aesthetics score of its middle frame, -1...1 (higher looks better; utility shots score low).
        var look: Double?
        var scene: [String]
        /// Its middle frame, a JPEG next to the facts file.
        var still: String?
        /// How its middle frame is laid out: people, captions, screens, panes and splits (docs/previz.feature).
        var layout: Layout? = nil
    }

    struct Words: Codable {
        var text: String
        var start: Double
        var end: Double
        var box: [Double]
    }

    struct Span: Codable {
        var label: String
        var start: Double
        var end: Double
    }

    enum Failure: Error, LocalizedError {
        case unreadable(String)
        var errorDescription: String? { if case .unreadable(let s) = self { return s }; return nil }
    }

    // MARK: entry

    /// Where a file's facts live: <project>/previews/facts/<key>/, the key being its path in the project with "/" as
    /// "-" and no "assets/" in front ("ad.mp4", "takes-take-20261009-1100-video.mov"): every take's video.mov differs.
    static func factsDir(for file: URL, in project: URL) -> URL {
        project.appendingPathComponent("previews/facts/\(key(for: file, in: project))", isDirectory: true)
    }

    /// A file's name for its facts and tracks: its path in the project with "/" as "-" and no "assets/" in front.
    static func key(for file: URL, in project: URL) -> String {
        let root = project.resolvingSymlinksInPath().path + "/"
        let path = file.resolvingSymlinksInPath().path
        var key = path.hasPrefix(root) ? String(path.dropFirst(root.count)) : file.lastPathComponent
        if key.hasPrefix("assets/") { key = String(key.dropFirst("assets/".count)) }
        return key.replacingOccurrences(of: "/", with: "-")
    }

    /// Reads a newly added asset in the background, so the Agent's first look at it is already answered.
    static func warm(_ file: URL, in project: URL) {
        Task.detached(priority: .utility) { _ = try? await analyze(file, into: factsDir(for: file, in: project)) }
    }

    /// Reads `url` and writes facts.json (and a still per shot) into `dir`. Returns the facts.
    static func analyze(_ url: URL, into dir: URL) async throws -> Facts {
        clock = Date()
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let type = UTType(filenameExtension: url.pathExtension.lowercased())
        var facts: Facts
        if type?.conforms(to: .image) == true {
            facts = try photo(url, into: dir)
        } else {
            let asset = AVURLAsset(url: url)
            let videos = (try? await asset.loadTracks(withMediaType: .video)) ?? []
            if let track = videos.first {
                facts = try await video(asset, track: track, name: url.lastPathComponent, into: dir)
            } else {
                let seconds = (try? await asset.load(.duration).seconds) ?? 0
                guard seconds > 0 else { throw Failure.unreadable("\(url.lastPathComponent): not a photo, video or sound") }
                facts = Facts(file: url.lastPathComponent, kind: "sound", seconds: r2(seconds))
            }
        }
        if facts.kind != "photo" { facts.sound = sound(url); lap("sound") }
        let data = try JSONEncoder.sorted.encode(facts)
        try data.write(to: dir.appendingPathComponent("facts.json"))
        if facts.kind == "video", tasteOn, let taste = try? await taste(url, shots: facts.shots) {
            try? JSONEncoder.compact.encode(taste).write(to: dir.appendingPathComponent("taste.json"))
            lap("taste")
        }
        return facts
    }

    /// The biggest face in a video's frame at `t` ([x, y, w, h] in 0...1 from the top left) and the upright picture's
    /// size: what `cut` frames a crop on. Nil when there's no face.
    static func face(_ url: URL, at t: Double) async -> (box: [Double], width: Int, height: Int)? {
        let generator = AVAssetImageGenerator(asset: AVURLAsset(url: url))
        generator.appliesPreferredTrackTransform = true
        generator.maximumSize = CGSize(width: 960, height: 960)
        guard let image = try? await generator.image(at: CMTime(seconds: t, preferredTimescale: 600)).image else { return nil }
        let request = VNDetectFaceRectanglesRequest()
        onCPUInSimulator(request)
        try? VNImageRequestHandler(cgImage: image, options: [:]).perform([request])
        guard let biggest = (request.results ?? []).max(by: { $0.boundingBox.height < $1.boundingBox.height }) else { return nil }
        return (box(biggest.boundingBox), image.width, image.height)
    }

    // MARK: photo

    static func photo(_ url: URL, into dir: URL) throws -> Facts {
        guard let source = CGImageSourceCreateWithURL(url as CFURL, nil),
              let image = CGImageSourceCreateThumbnailAtIndex(source, 0, [
                  kCGImageSourceCreateThumbnailFromImageAlways: true, kCGImageSourceCreateThumbnailWithTransform: true,
                  kCGImageSourceThumbnailMaxPixelSize: 1600] as CFDictionary)
        else { throw Failure.unreadable("\(url.lastPathComponent): can't read the picture") }
        let props = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any]
        var (w, h) = (props?[kCGImagePropertyPixelWidth] as? Int ?? image.width, props?[kCGImagePropertyPixelHeight] as? Int ?? image.height)
        if ((props?[kCGImagePropertyOrientation] as? Int) ?? 1) >= 5 { swap(&w, &h) }
        let seen = see(image, ocr: true)
        let still = "shot-01.jpg"
        saveJPEG(image, to: dir.appendingPathComponent(still))
        let shot = Shot(start: 0, end: 0, transition: "start", framing: seen.framing, faces: seen.faces, subject: seen.subject,
                        motion: "still", brightness: r2(meanBrightness(image)), look: seen.look, scene: seen.scene, still: still,
                        layout: layout(image))
        var facts = Facts(file: url.lastPathComponent, kind: "photo", width: w, height: h)
        facts.shots = [shot]
        facts.text = seen.words.map { Words(text: $0.0, start: 0, end: 0, box: $0.1) }
        facts.scene = seen.scene
        return facts
    }

    // MARK: video

    /// A small picture of one sampled frame: a grid of RGB values (0...1) and its colour histogram.
    struct Sample {
        var t: Double
        var grid: [Float]
        var hist: [Float]
        var luma: Float
    }

    static let gridW = 48, gridH = 27

    static func video(_ asset: AVURLAsset, track: AVAssetTrack, name: String, into dir: URL) async throws -> Facts {
        let duration = try await asset.load(.duration).seconds
        let size = try await track.load(.naturalSize)
        let transform = try await track.load(.preferredTransform)
        let upright = size.applying(transform)
        let (w, h) = (Int(abs(upright.width).rounded()), Int(abs(upright.height).rounded()))
        // Long footage is sampled less often: the decode is the cost.
        let fps: Double = duration > 600 ? 2 : duration > 180 ? 4 : 10
        let samples = try await sampleFrames(asset, track: track, fps: fps, duration: duration)
        guard !samples.isEmpty else { throw Failure.unreadable("\(name): no frames could be read") }
        lap("decoded \(samples.count) samples")

        let bounds = shotBounds(samples, duration: duration)
        let generator = AVAssetImageGenerator(asset: asset)
        generator.appliesPreferredTrackTransform = true
        generator.maximumSize = CGSize(width: 960, height: 960)
        generator.requestedTimeToleranceBefore = CMTime(seconds: 0.05, preferredTimescale: 600)
        generator.requestedTimeToleranceAfter = CMTime(seconds: 0.05, preferredTimescale: 600)

        var shots: [Shot] = []
        var reads: [(String, Double, [Double])] = []
        var ocrBudget = 30
        var layoutBudget = 40
        for (i, b) in bounds.enumerated() {
            let mid = (b.start + b.end) / 2
            let inside = samples.filter { $0.t >= b.start && $0.t < b.end }
            let change = zip(inside.dropFirst(), inside).map { pixelDistance($0.grid, $1.grid) }
            let motion = (change.isEmpty ? 0 : change.reduce(0, +) / Float(change.count)) < 0.025 ? "still" : "moving"
            let brightness = inside.isEmpty ? 0 : inside.map(\.luma).reduce(0, +) / Float(inside.count)
            guard let frame = try? await generator.image(at: CMTime(seconds: mid, preferredTimescale: 600)).image else {
                shots.append(Shot(start: r2(b.start), end: r2(b.end), transition: b.transition, framing: "no people", faces: 0, subject: nil,
                                  motion: motion, brightness: r2(Double(brightness)), look: nil, scene: [], still: nil))
                continue
            }
            let seen = see(frame, ocr: ocrBudget > 0)
            if ocrBudget > 0 { ocrBudget -= 1; reads += seen.words.map { ($0.0, mid, $0.1) } }
            // Words that come and go inside a long shot: read it every 2 s too.
            var t = b.start + 1.0
            while t < b.end - 0.5 && ocrBudget > 0 && b.end - b.start > 3 {
                if abs(t - mid) > 0.6, let f = try? await generator.image(at: CMTime(seconds: t, preferredTimescale: 600)).image {
                    reads += words(f).map { ($0.0, t, $0.1) }
                    ocrBudget -= 1
                }
                t += 2
            }
            let still = String(format: "shot-%02d.jpg", i + 1)
            saveJPEG(frame, to: dir.appendingPathComponent(still))
            // the layout, with two frames just after the middle so a split has to hold still to count
            var laidOut: Layout? = nil
            if layoutBudget > 0 {
                layoutBudget -= 1
                var next: [CGImage] = []
                for dt in [0.4, 0.8] where mid + dt < b.end - 0.05 {
                    if let f = try? await generator.image(at: CMTime(seconds: mid + dt, preferredTimescale: 600)).image { next.append(f) }
                }
                laidOut = layout(frame, next: next)
            }
            shots.append(Shot(start: r2(b.start), end: r2(b.end), transition: b.transition, framing: seen.framing, faces: seen.faces,
                              subject: seen.subject, motion: motion, brightness: r2(Double(brightness)), look: seen.look,
                              scene: seen.scene, still: still, layout: laidOut))
        }
        lap("looked at \(shots.count) shots")
        var facts = Facts(file: name, kind: "video", width: w, height: h, seconds: r2(duration))
        facts.audio = !((try? await asset.loadTracks(withMediaType: .audio)) ?? []).isEmpty
        facts.shots = shots
        facts.text = spans(reads)
        facts.scene = commonest(shots.flatMap(\.scene), top: 4)
        // A face on camera: how the creator comes across, second by second (fewer frames for long footage).
        if shots.contains(where: { $0.faces > 0 }), duration <= 600 {
            facts.performance = try? await performance(asset.url, fps: duration > 180 ? 8 : 15)
            lap("performance")
        }
        return facts
    }

    /// Frames at `fps`, each shrunk to a 48x27 grid and a 64-bin colour histogram. Small frames straight from the
    /// decoder (the grid needs 48x27 points, not 1080p); full-size ones where the decoder can't scale (the Simulator).
    static func sampleFrames(_ asset: AVURLAsset, track: AVAssetTrack, fps: Double, duration: Double) async throws -> [Sample] {
        if let small = try? sampleFrames(asset, track: track, fps: fps, scaled: true), !small.isEmpty { return small }
        if let full = try? sampleFrames(asset, track: track, fps: fps, scaled: false), !full.isEmpty { return full }
        // Where the reader can't decode at all (the Simulator, some codecs), the still-frame generator still can.
        let generator = AVAssetImageGenerator(asset: asset)
        generator.appliesPreferredTrackTransform = true
        generator.maximumSize = CGSize(width: 192, height: 192)
        let tolerance = CMTime(seconds: 0.5 / fps, preferredTimescale: 600)
        generator.requestedTimeToleranceBefore = tolerance
        generator.requestedTimeToleranceAfter = tolerance
        let times = stride(from: 0.0, to: duration, by: 1 / fps).map { CMTime(seconds: $0, preferredTimescale: 600) }
        var samples: [Sample] = []
        for await result in generator.images(for: times) {
            if let image = try? result.image, let s = sample(image, t: result.requestedTime.seconds) { samples.append(s) }
        }
        guard !samples.isEmpty else { throw Failure.unreadable("the video couldn't be decoded") }
        return samples.sorted { $0.t < $1.t }
    }

    /// A still drawn down to the grid: the same Sample the reader's frames make.
    static func sample(_ image: CGImage, t: Double) -> Sample? {
        let (w, h) = (gridW, gridH)
        var px = [UInt8](repeating: 0, count: w * h * 4)
        guard let ctx = CGContext(data: &px, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4, space: CGColorSpaceCreateDeviceRGB(),
                                  bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue) else { return nil }
        ctx.interpolationQuality = .low
        ctx.draw(image, in: CGRect(x: 0, y: 0, width: w, height: h))
        var grid = [Float](repeating: 0, count: w * h * 3)
        var hist = [Float](repeating: 0, count: 64)
        var luma: Float = 0
        for i in 0..<(w * h) {
            let (r, g, b) = (Float(px[i * 4]) / 255, Float(px[i * 4 + 1]) / 255, Float(px[i * 4 + 2]) / 255)
            grid[i * 3] = r; grid[i * 3 + 1] = g; grid[i * 3 + 2] = b
            hist[min(3, Int(r * 4)) * 16 + min(3, Int(g * 4)) * 4 + min(3, Int(b * 4))] += 1
            luma += 0.2126 * r + 0.7152 * g + 0.0722 * b
        }
        let n = Float(w * h)
        return Sample(t: t, grid: grid, hist: hist.map { $0 / n }, luma: luma / n)
    }

    static func sampleFrames(_ asset: AVURLAsset, track: AVAssetTrack, fps: Double, scaled: Bool) throws -> [Sample] {
        let reader = try AVAssetReader(asset: asset)
        var settings: [String: Any] = [kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA]
        if scaled { settings[kCVPixelBufferWidthKey as String] = 192; settings[kCVPixelBufferHeightKey as String] = 192 }
        let out = AVAssetReaderTrackOutput(track: track, outputSettings: settings)
        out.alwaysCopiesSampleData = false
        reader.add(out)
        guard reader.startReading() else { throw Failure.unreadable(reader.error?.localizedDescription ?? "can't decode the video") }
        var samples: [Sample] = []
        var next = 0.0
        while let buffer = out.copyNextSampleBuffer() {
            let t = CMSampleBufferGetPresentationTimeStamp(buffer).seconds
            guard t + 1e-6 >= next, let px = CMSampleBufferGetImageBuffer(buffer) else { continue }
            next += 1 / fps
            if let s = autoreleasepool(invoking: { sample(px, t: t) }) { samples.append(s) }
        }
        if samples.isEmpty, reader.status == .failed {
            throw Failure.unreadable("can't decode the video (\(reader.error.map { ($0 as NSError).description } ?? "no error"))")
        }
        return samples
    }

    static func sample(_ px: CVPixelBuffer, t: Double) -> Sample? {
        CVPixelBufferLockBaseAddress(px, .readOnly)
        defer { CVPixelBufferUnlockBaseAddress(px, .readOnly) }
        guard let base = CVPixelBufferGetBaseAddress(px) else { return nil }
        let (w, h, row) = (CVPixelBufferGetWidth(px), CVPixelBufferGetHeight(px), CVPixelBufferGetBytesPerRow(px))
        let bytes = base.assumingMemoryBound(to: UInt8.self)
        var grid = [Float](repeating: 0, count: gridW * gridH * 3)
        var hist = [Float](repeating: 0, count: 64)
        var luma: Float = 0
        for gy in 0..<gridH {
            for gx in 0..<gridW {
                let x = (gx * 2 + 1) * w / (gridW * 2), y = (gy * 2 + 1) * h / (gridH * 2)
                let p = bytes + y * row + x * 4
                let (b, g, r) = (Float(p[0]) / 255, Float(p[1]) / 255, Float(p[2]) / 255)
                let i = (gy * gridW + gx) * 3
                grid[i] = r; grid[i + 1] = g; grid[i + 2] = b
                hist[min(3, Int(r * 4)) * 16 + min(3, Int(g * 4)) * 4 + min(3, Int(b * 4))] += 1
                luma += 0.2126 * r + 0.7152 * g + 0.0722 * b
            }
        }
        let n = Float(gridW * gridH)
        return Sample(t: t, grid: grid, hist: hist.map { $0 / n }, luma: luma / n)
    }

    static func pixelDistance(_ a: [Float], _ b: [Float]) -> Float {
        var d: Float = 0
        for i in 0..<a.count { d += abs(a[i] - b[i]) }
        return d / Float(a.count)
    }

    static func histDistance(_ a: [Float], _ b: [Float]) -> Float {
        var d: Float = 0
        for i in 0..<a.count { d += abs(a[i] - b[i]) }
        return d / 2
    }

    struct Bound { var start: Double; var end: Double; var transition: String }

    /// Where the shots are and how each begins. A cut is a jump that stands out from its neighbourhood; a fade goes
    /// through black; a dissolve is a stretch whose middle frame is the blend of its two ends (motion isn't).
    static func shotBounds(_ s: [Sample], duration: Double) -> [Bound] {
        let n = s.count
        guard n > 2 else { return [Bound(start: 0, end: duration, transition: "start")] }
        var change = [Float](repeating: 0, count: n)   // change[i]: from sample i-1 to i
        for i in 1..<n { change[i] = max(histDistance(s[i].hist, s[i - 1].hist), pixelDistance(s[i].grid, s[i - 1].grid) * 1.6) }
        func median(_ v: [Float]) -> Float { let x = v.sorted(); return x.isEmpty ? 0 : x[x.count / 2] }

        // (time, transition, end of the transition) in order.
        var marks: [(Double, String, Double)] = []
        let dark = s.map { $0.luma < 0.06 }
        var step = [Float](repeating: 0, count: n)   // pixel change from sample i-1 to i
        for i in 1..<n { step[i] = pixelDistance(s[i].grid, s[i - 1].grid) }
        let second = max(3, Int((Double(n) / max(duration, 0.1)).rounded()))   // samples in about a second
        var i = 1
        while i < n {
            // Through black: a dark stretch between two pictures, widened to the dimming before and the brightening after.
            if dark[i] && !dark[i - 1] {
                var j = i
                while j < n && dark[j] { j += 1 }
                guard j < n else { break }
                var (p, q) = (i, j)
                while p - 1 > 0 && i - p < second && s[p - 1].luma > s[p].luma + 0.004 { p -= 1 }
                while q + 1 < n && q - j < second && s[q + 1].luma > s[q].luma + 0.004 { q += 1 }
                marks.append((s[p].t, "fade", s[q].t))
                i = q + 1
                continue
            }
            // A dissolve: k samples of steady change adding up to a different picture whose middle is the blend of its
            // two ends (a camera move or someone moving isn't). Tried before cuts: a dissolve between busy pictures
            // jumps a lot from sample to sample.
            var found = false
            for k in stride(from: min(15, n - i), through: 3, by: -1) {
                let (a, b) = (i - 1, i - 1 + k)
                let total = pixelDistance(s[a].grid, s[b].grid)
                guard total >= 0.12, (i...b).allSatisfy({ step[$0] < total * 0.7 }) else { continue }
                let m = (a + b) / 2
                var blend: Float = 0
                for p in 0..<s[m].grid.count { blend += abs(s[m].grid[p] - (s[a].grid[p] + s[b].grid[p]) / 2) }
                blend /= Float(s[m].grid.count)
                if blend <= total * 0.2 {
                    // The widest window that passes also holds steady frames either side: trim them off.
                    var (lo, hi) = (a, b)
                    while lo + 1 < hi && step[lo + 1] < total * 0.06 { lo += 1 }
                    while hi - 1 > lo && step[hi] < total * 0.06 { hi -= 1 }
                    marks.append((s[lo].t, "dissolve", s[hi].t))
                    i = b + 1
                    found = true
                    break
                }
            }
            if found { continue }
            let around = (max(1, i - 5)..<min(n, i + 6)).filter { $0 != i }.map { change[$0] }
            if change[i] >= 0.3 && change[i] >= median(around) * 2.5 + 0.05 { marks.append((s[i].t, "cut", s[i].t)) }
            i += 1
        }
        var bounds: [Bound] = []
        var start = 0.0, how = "start"
        for (t, kind, until) in marks {
            if t - start >= 0.4 { bounds.append(Bound(start: start, end: t, transition: how)) }
            start = until; how = kind
        }
        if duration - start >= 0.2 || bounds.isEmpty { bounds.append(Bound(start: start, end: duration, transition: how)) }
        return bounds
    }

    // MARK: one frame

    struct Seen {
        var framing = "no people"
        var faces = 0
        var subject: [Double]?
        var look: Double?
        var scene: [String] = []
        var words: [(String, [Double])] = []
    }

    static func see(_ image: CGImage, ocr: Bool) -> Seen {
        let handler = VNImageRequestHandler(cgImage: image, options: [:])
        let faces = VNDetectFaceRectanglesRequest()
        let bodies = VNDetectHumanRectanglesRequest()
        let attention = VNGenerateAttentionBasedSaliencyImageRequest()
        let classify = VNClassifyImageRequest()
        let aesthetics = VNCalculateImageAestheticsScoresRequest()
        // One at a time: a request the device can't run (the Simulator lacks some) mustn't take the others with it.
        // In the Simulator, scene labels and the aesthetics score come out wrong on the CPU (a face read as "night
        // sky") and slow, so they're left out there.
        #if targetEnvironment(simulator)
        let requests: [VNRequest] = [faces, bodies, attention]
        #else
        let requests: [VNRequest] = [faces, bodies, attention, classify, aesthetics]
        #endif
        for request in requests { onCPUInSimulator(request); try? handler.perform([request]) }
        var seen = Seen()
        let found = faces.results ?? []
        seen.faces = found.count
        if let biggest = found.map(\.boundingBox.height).max() {
            seen.framing = biggest >= 0.22 ? "close-up" : biggest >= 0.09 ? "medium" : "wide"
        } else if !(bodies.results ?? []).isEmpty {
            seen.framing = "wide"
        }
        if let s = attention.results?.first?.salientObjects?.max(by: { $0.confidence < $1.confidence }) { seen.subject = box(s.boundingBox) }
        seen.look = aesthetics.results?.first.map { r2(Double($0.overallScore)) }
        seen.scene = (classify.results ?? []).filter { $0.confidence >= 0.3 }.prefix(4).map { $0.identifier.replacingOccurrences(of: "_", with: " ") }
        if ocr { seen.words = words(image) }
        return seen
    }

    /// The lines of text in a frame, read accurately, with their boxes.
    static func words(_ image: CGImage) -> [(String, [Double])] {
        let request = VNRecognizeTextRequest()
        request.recognitionLevel = .accurate
        request.usesLanguageCorrection = true
        request.minimumTextHeight = 0.015
        onCPUInSimulator(request)
        try? VNImageRequestHandler(cgImage: image, options: [:]).perform([request])
        return (request.results ?? []).compactMap { o in
            guard let c = o.topCandidates(1).first, c.confidence >= 0.4 else { return nil }
            let text = c.string.trimmingCharacters(in: .whitespacesAndNewlines)
            return text.filter({ $0.isLetter || $0.isNumber }).count >= 2 ? (text, box(o.boundingBox)) : nil
        }
    }

    /// The same words read at nearby moments become one span.
    static func spans(_ reads: [(String, Double, [Double])]) -> [Words] {
        var out: [Words] = []
        for (text, t, b) in reads.sorted(by: { $0.1 < $1.1 }) {
            let key = text.lowercased()
            if let i = out.lastIndex(where: { $0.text.lowercased() == key && t - $0.end <= 2.6 }) {
                out[i].end = r2(max(out[i].end, t))
            } else {
                out.append(Words(text: text, start: r2(t), end: r2(t), box: b))
            }
        }
        return out.sorted { $0.start < $1.start }
    }

    // MARK: sound

    final class Heard: NSObject, SNResultsObserving {
        var rows: [(String, Double, Double)] = []
        func request(_ request: SNRequest, didProduce result: SNResult) {
            guard let r = result as? SNClassificationResult, let top = r.classifications.first, top.confidence >= 0.35 else { return }
            rows.append((top.identifier, r.timeRange.start.seconds, r.timeRange.end.seconds))
        }
    }

    /// What's heard over time, in a small closed set of labels.
    static func sound(_ url: URL) -> [Span] {
        guard let analyzer = try? SNAudioFileAnalyzer(url: url),
              let request = try? SNClassifySoundRequest(classifierIdentifier: .version1) else { return [] }
        request.windowDuration = CMTime(seconds: 1.0, preferredTimescale: 1000)
        request.overlapFactor = 0.5
        let heard = Heard()
        guard (try? analyzer.add(request, withObserver: heard)) != nil else { return [] }
        analyzer.analyze()
        let speech: Set = ["speech", "conversation", "narration_monologue", "whispering", "shout", "yell", "babble"]
        let music: Set = ["music", "electronic_music", "pop_music", "hip_hop_music", "rock_music", "techno", "ambient_music",
                          "piano", "guitar", "drum", "drum_kit", "synthesizer", "orchestra", "bass_guitar", "keyboard_musical"]
        var out: [Span] = []
        for (id, a, b) in heard.rows {
            let label = speech.contains(id) ? "speech" : music.contains(id) || id.hasSuffix("_music") ? "music"
                : id == "singing" || id == "choir" ? "singing" : id == "laughter" ? "laughter" : id == "applause" ? "applause"
                : id == "silence" ? "silence" : "noise"
            if let last = out.last, last.label == label, a - last.end <= 0.6 { out[out.count - 1].end = r2(b) }
            else { out.append(Span(label: label, start: r2(a), end: r2(b))) }
        }
        return out.filter { $0.end - $0.start >= 1.0 }
    }

    // MARK: tracks

    /// A clip's per-frame track (framework/docs/ml-tracks.feature › "Track formats are the contract"): mattes as a PNG sequence beside
    /// track.json, or per-frame samples inside it. Made once per clip; compositions use the files through plain
    /// HTML/SVG, so Chrome, the app's preview and raea render them alike.
    struct Track: Codable {
        var kind: String
        var label: String?
        var source: String
        var model: String
        var fps: Double
        var start: Double
        var frames: Int
        var width: Int
        var height: Int
        var samples: [FaceSample?]?
        /// An object track's prompt.
        var prompt: ObjectPrompt?
        /// The seconds of the clip it was made for, when not the whole clip.
        var range: [Double]?
    }
    struct FaceSample: Codable { var faces: [FaceBox] }

    /// What an `object-<label>` track follows: a box [x, y, w, h] or a point [x, y] on it, normalized to the frame,
    /// on the frame at `at` seconds of the clip. Frames before that one are empty (tracking runs forward).
    struct ObjectPrompt: Codable {
        var box: [Double]?
        var point: [Double]?
        var at: Double
    }
    /// A face: its box, and with the MediaPipe models its 478 points [x, y, z] (z at x's scale, smaller = nearer)
    /// and 52 blendshapes, all normalized to the frame. Without the models (not in this build) it's Vision's box.
    struct FaceBox: Codable {
        var box: [Double]
        var points: [[Double]]?
        var blendshapes: [String: Double]?
    }

    /// The kinds that can be made today: Vision's person mattes, MediaPipe's hair mattes and face mesh, and depth
    /// from Depth Anything 3 Small (~/host/models; hands, pose and object tracks come with their models).
    static let trackKinds = ["person", "hair", "face", "depth"]

    /// Where a clip's track of `kind` lives: <project>/tracks/<key>/<kind>/ (the key as for facts, since every
    /// take's video is called video.mov).
    static func trackDir(for clip: URL, kind: String, in project: URL) -> URL {
        project.appendingPathComponent("tracks/\(key(for: clip, in: project))/\(kind)", isDirectory: true)
    }

    /// Makes the `kind` track of the video `url` in `dir` (replacing what was there) and returns it. `source` is the
    /// clip's path as the project names it. Frames are the upright picture, every 1/fps seconds (the clip's own rate,
    /// at most 30), at half the clip's size rounded to even.
    /// `range` limits it to those seconds of the clip (the part an edit uses: tracking is slow on a phone); the track's
    /// `start` is then the first frame in it. `fullSize` makes mattes at the clip's own size instead of half (the
    /// reference checks compare at full size).
    static func track(_ url: URL, kind: String, source: String, into dir: URL, prompt: ObjectPrompt? = nil,
                      range: ClosedRange<Double>? = nil, fullSize: Bool = false) async throws -> Track {
        let object = kind.hasPrefix("object-") && kind.count > 7
        guard trackKinds.contains(kind) || object else {
            throw Failure.unreadable("a \(kind) track isn't available yet (today: \(trackKinds.joined(separator: ", ")), object-<name>)")
        }
        if object, prompt?.box?.count != 4, prompt?.point?.count != 2 {
            throw Failure.unreadable("an object track needs the object's box [x, y, w, h] or a point [x, y] on it, at a time it's in view")
        }
        let asset = AVURLAsset(url: url)
        guard let video = try await asset.loadTracks(withMediaType: .video).first else { throw Failure.unreadable("\(url.lastPathComponent) has no picture") }
        let (natural, transform, nominal) = try await video.load(.naturalSize, .preferredTransform, .nominalFrameRate)
        let upright = CGRect(origin: .zero, size: natural).applying(transform).standardized.size
        let even = { (x: CGFloat) -> Int in fullSize ? Int(x.rounded()) : max(2, Int((x / 2).rounded()) / 2 * 2) }
        let (width, height) = (even(upright.width), even(upright.height))
        let fps = min(30, nominal > 0 ? Double(nominal) : 30)
        try? FileManager.default.removeItem(at: dir)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)

        let reader = try AVAssetReader(asset: asset)
        let out = AVAssetReaderTrackOutput(track: video, outputSettings: [kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA])
        out.alwaysCopiesSampleData = false
        reader.add(out)
        if let range {
            reader.timeRange = CMTimeRange(start: CMTime(seconds: max(0, range.lowerBound), preferredTimescale: 600),
                                           end: CMTime(seconds: range.upperBound, preferredTimescale: 600))
        }
        guard reader.startReading() else { throw Failure.unreadable(reader.error?.localizedDescription ?? "can't decode the video") }
        let context = modelContext()
        let orient = orientation(transform)
        // Depth: one range for the whole clip (no flicker), from a quick look at ~16 frames first.
        var depth: (model: MLModel, lo: Float, hi: Float)?
        if kind == "depth" {
            let model = try depthModel()
            let (lo, hi) = try await depthRange(asset, model: model, context: context)
            depth = (model, lo, hi)
        }
        let hair = try kind == "hair" ? model("HairSegmenter", folder: "mediapipe-hair-segmenter") : nil
        // Faces: MediaPipe's mesh where the models are in the build, else Vision's boxes.
        let mesh = kind == "face" ? (try? faceModels()) : nil
        var faceCrop: FaceCrop?
        let tracker = try object ? ObjectTracker(models: objectModels()) : nil
        var empty: CVPixelBuffer?
        var frames = 0
        var start: Double?
        var samples: [FaceSample?] = []
        while let buffer = out.copyNextSampleBuffer() {
            let t = CMSampleBufferGetPresentationTimeStamp(buffer).seconds
            let first = start ?? t
            guard t + 1e-6 >= first + Double(frames) / fps, let px = CMSampleBufferGetImageBuffer(buffer) else { continue }
            start = first
            try autoreleasepool {
                let picture = CIImage(cvPixelBuffer: px).oriented(orient)
                let handler = VNImageRequestHandler(ciImage: picture, options: [:])
                if let depth {
                    let inverse = try inverseDepth(picture, model: depth.model, context: context)
                    try writeDepth(inverse, lo: depth.lo, hi: depth.hi, width: width, height: height,
                                   to: dir.appendingPathComponent(String(format: "%06d.png", frames)))
                } else if let tracker, let prompt {
                    // EdgeTAM from the prompted frame on; empty before it.
                    let at = max(0, Int(((prompt.at - first) * fps).rounded()))
                    let file = dir.appendingPathComponent(String(format: "%06d.png", frames))
                    if frames < at {
                        if empty == nil { empty = try blankMatte() }
                        try writeMatte(empty!, width: width, height: height, context: context, to: file)
                    } else {
                        try writeMatte(try tracker.step(picture, prompt: frames == at ? prompt : nil, width: width, height: height, context: context),
                                       width: width, height: height, context: context, to: file)
                    }
                } else if let hair {
                    try writeMatte(try hairMask(picture, model: hair, context: context), width: width, height: height, context: context,
                                   to: dir.appendingPathComponent(String(format: "%06d.png", frames)))
                } else if let mesh {
                    // Follow the face from the last frame's points, as MediaPipe does; look for it again once it's lost.
                    var found = try faceCrop.flatMap { try faceMesh(picture, crop: $0, models: mesh, context: context) }
                    if found == nil, let crop = try detectFace(picture, models: mesh, context: context) {
                        found = try faceMesh(picture, crop: crop, models: mesh, context: context)
                    }
                    faceCrop = found.map { crop(around: $0.points) }
                    samples.append(found.map { FaceSample(faces: [faceBox($0, in: picture.extent.size)]) })
                } else if kind == "person" {
                    let request = VNGeneratePersonSegmentationRequest()
                    request.qualityLevel = .accurate
                    request.outputPixelFormat = kCVPixelFormatType_OneComponent8
                    onCPUInSimulator(request)
                    try handler.perform([request])
                    guard let mask = request.results?.first?.pixelBuffer else { throw Failure.unreadable("no person matte at \(r2(t)) s") }
                    try writeMatte(mask, width: width, height: height, context: context,
                                   to: dir.appendingPathComponent(String(format: "%06d.png", frames)))
                } else {
                    let request = VNDetectFaceRectanglesRequest()
                    onCPUInSimulator(request)
                    try handler.perform([request])
                    let faces = (request.results ?? []).map { FaceBox(box: box($0.boundingBox)) }
                    samples.append(faces.isEmpty ? nil : FaceSample(faces: faces))
                }
            }
            frames += 1
        }
        if frames == 0 { throw Failure.unreadable("can't decode the video (\(reader.error.map { ($0 as NSError).description } ?? "no frames"))") }
        var made = Track(kind: kind, label: object ? String(kind.dropFirst(7)) : nil, source: source,
                         model: object ? "edgetam" : ["person": "apple-vision-person-segmentation-accurate",
                                 "face": mesh == nil ? "apple-vision-face-rectangles" : "mediapipe-face-landmarker-v2",
                                 "hair": "mediapipe-hair-segmenter", "depth": "depth-anything-3-small-504"][kind] ?? kind,
                         fps: fps, start: r2(start ?? 0), frames: frames, width: width, height: height,
                         samples: kind == "face" ? samples : nil)
        made.prompt = object ? prompt : nil
        if let range {
            let seconds = (try? await asset.load(.duration).seconds) ?? range.upperBound
            made.range = [r2(max(0, range.lowerBound)), r2(min(range.upperBound, seconds))]
        }
        // A face mesh is ~10 KB a frame: written compact, not pretty.
        let encoder = JSONEncoder()
        encoder.outputFormatting = mesh == nil ? [.sortedKeys, .prettyPrinted] : [.sortedKeys]
        try encoder.encode(made).write(to: dir.appendingPathComponent("track.json"))
        return made
    }

    // MARK: depth (Depth Anything 3 Small: input `image` 504×504 RGB, output `depth` [1,1,504,504], bigger = farther)

    static let depthSide = 504

    static func depthModel() throws -> MLModel { try model("DepthAnything3Small", folder: "depth-anything-3-small") }

    /// A compiled model: the app's bundled copy, else <name>.mlpackage in ~/host/models/<folder> (or
    /// $UGC_MODELS), compiled once into Caches.
    static func model(_ name: String, folder: String, units: MLComputeUnits = .all) throws -> MLModel {
        let config = MLModelConfiguration()
        #if targetEnvironment(simulator)
        config.computeUnits = .cpuOnly
        #else
        config.computeUnits = units
        #endif
        if let bundled = Bundle.main.url(forResource: name, withExtension: "mlmodelc") {
            return try MLModel(contentsOf: bundled, configuration: config)
        }
        #if os(macOS)
        // The Mac tool (agent/tools/perceive) has no bundle: it uses the repo's models folder.
        let models = ProcessInfo.processInfo.environment["UGC_MODELS"].map { URL(fileURLWithPath: $0) }
            ?? FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("host/models")
        let package = Bundle.main.url(forResource: name, withExtension: "mlpackage")
            ?? models.appendingPathComponent("\(folder)/\(name).mlpackage")
        #else
        // Optional model packs (T-410) download into Application Support/Models/<folder>/, not the bundle.
        let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Models/\(folder)", isDirectory: true)
        let packed = support.appendingPathComponent("\(name).mlmodelc")
        if FileManager.default.fileExists(atPath: packed.path) { return try MLModel(contentsOf: packed, configuration: config) }
        let download = support.appendingPathComponent("\(name).mlpackage")
        guard let package = Bundle.main.url(forResource: name, withExtension: "mlpackage")
            ?? (FileManager.default.fileExists(atPath: download.path) ? download : nil) else {
            throw Failure.unreadable("the \(name) model isn't in this build")
        }
        #endif
        guard FileManager.default.fileExists(atPath: package.path) else {
            throw Failure.unreadable("the \(name) model isn't installed")
        }
        let caches = try FileManager.default.url(for: .cachesDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
        let compiled = caches.appendingPathComponent("\(name).mlmodelc")
        let stamp = { (u: URL) in (try? u.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate) ?? .distantPast }
        if !FileManager.default.fileExists(atPath: compiled.path) || stamp(compiled) < stamp(package) {
            let fresh = try MLModel.compileModel(at: package)
            try? FileManager.default.removeItem(at: compiled)
            try FileManager.default.moveItem(at: fresh, to: compiled)
        }
        return try MLModel(contentsOf: compiled, configuration: config)
    }

    /// 1/depth (bigger = nearer) for the picture's own area, as rows of floats with its size, letterboxed into the
    /// model's square and cropped back.
    static func inverseDepth(_ picture: CIImage, model: MLModel, context: CIContext) throws -> (values: [Float], width: Int, height: Int) {
        let side = depthSide
        let extent = picture.extent
        let scale = min(CGFloat(side) / extent.width, CGFloat(side) / extent.height)
        let (cw, ch) = (max(1, Int((extent.width * scale).rounded())), max(1, Int((extent.height * scale).rounded())))
        let (ox, oy) = ((side - cw) / 2, (side - ch) / 2)
        var px: CVPixelBuffer?
        CVPixelBufferCreate(nil, side, side, kCVPixelFormatType_32BGRA, [kCVPixelBufferCGImageCompatibilityKey: true] as CFDictionary, &px)
        guard let px else { throw Failure.unreadable("no memory for a depth frame") }
        let placed = picture.transformed(by: CGAffineTransform(translationX: -extent.minX, y: -extent.minY))
            .transformed(by: CGAffineTransform(scaleX: scale, y: scale))
            .transformed(by: CGAffineTransform(translationX: CGFloat(ox), y: CGFloat(oy)))
            .composited(over: CIImage(color: .black).cropped(to: CGRect(x: 0, y: 0, width: side, height: side)))
        context.render(placed, to: px)
        let out = try model.prediction(from: MLDictionaryFeatureProvider(dictionary: ["image": MLFeatureValue(pixelBuffer: px)]))
        guard let depth = out.featureValue(for: "depth")?.multiArrayValue else { throw Failure.unreadable("the depth model gave no depth") }
        // CIImage's origin is bottom-left; the model's rows run from the top, padded (504 values in a 512 stride).
        let top = side - oy - ch
        let row = depth.strides.count >= 2 ? depth.strides[depth.strides.count - 2].intValue : side
        // One pixel in from a padded edge: the model smears the black bars into the picture's border.
        let (ix, iy) = (ox > 0 ? 1 : 0, oy > 0 ? 1 : 0)
        let (w, h) = (cw - 2 * ix, ch - 2 * iy)
        var values = [Float](repeating: 0, count: w * h)
        depth.withUnsafeBufferPointer(ofType: Float.self) { d in
            for y in 0..<h {
                for x in 0..<w {
                    let v = d[(top + iy + y) * row + ox + ix + x]
                    values[y * w + x] = v > 1e-6 ? 1 / v : 0
                }
            }
        }
        return (values, w, h)
    }

    /// The clip's 1st and 99th percentile of 1/depth over ~16 frames spread through it.
    static func depthRange(_ asset: AVURLAsset, model: MLModel, context: CIContext) async throws -> (Float, Float) {
        let seconds = try await asset.load(.duration).seconds
        let generator = AVAssetImageGenerator(asset: asset)
        generator.appliesPreferredTrackTransform = true
        generator.maximumSize = CGSize(width: 1024, height: 1024)
        var all: [Float] = []
        for i in 0..<16 {
            let t = seconds * (Double(i) + 0.5) / 16
            guard let image = try? await generator.image(at: CMTime(seconds: t, preferredTimescale: 600)).image else { continue }
            let d = try inverseDepth(CIImage(cgImage: image), model: model, context: context)
            all.append(contentsOf: stride(from: 0, to: d.values.count, by: 7).map { d.values[$0] })
        }
        guard !all.isEmpty else { throw Failure.unreadable("couldn't read frames for depth") }
        all.sort()
        let lo = all[all.count / 100], hi = all[all.count * 99 / 100]
        return (lo, hi > lo ? hi : lo + 1e-6)
    }

    /// A depth frame as the track format has it: 8-bit gray, 255 = nearest, clamped to the clip's range.
    static func writeDepth(_ d: (values: [Float], width: Int, height: Int), lo: Float, hi: Float, width: Int, height: Int, to url: URL) throws {
        let bytes = d.values.map { UInt8(max(0, min(255, (($0 - lo) / (hi - lo) * 255).rounded()))) }
        guard let provider = CGDataProvider(data: Data(bytes) as CFData),
              let small = CGImage(width: d.width, height: d.height, bitsPerComponent: 8, bitsPerPixel: 8, bytesPerRow: d.width,
                                  space: CGColorSpaceCreateDeviceGray(), bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.none.rawValue),
                                  provider: provider, decode: nil, shouldInterpolate: true, intent: .defaultIntent),
              let ctx = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width,
                                  space: CGColorSpaceCreateDeviceGray(), bitmapInfo: CGImageAlphaInfo.none.rawValue) else {
            throw Failure.unreadable("couldn't write \(url.lastPathComponent)")
        }
        ctx.interpolationQuality = .high
        ctx.draw(small, in: CGRect(x: 0, y: 0, width: width, height: height))
        guard let image = ctx.makeImage(), let dest = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil) else {
            throw Failure.unreadable("couldn't write \(url.lastPathComponent)")
        }
        CGImageDestinationAddImage(dest, image, nil)
        guard CGImageDestinationFinalize(dest) else { throw Failure.unreadable("couldn't write \(url.lastPathComponent)") }
    }

    // MARK: hair (MediaPipe hair segmenter: input `image` 512×512 RGB, the whole frame stretched to it as MediaPipe
    // does; output `hair` [1,1,512,512], hair's probability; models/mediapipe-hair-segmenter/mediapipe-hair-segmenter.feature)

    static let hairSide = 512

    /// The picture's hair matte on the model's 512×512 grid, 8-bit (255 = hair), rows from the top. writeMatte
    /// stretches it back to the frame.
    static func hairMask(_ picture: CIImage, model: MLModel, context: CIContext) throws -> CVPixelBuffer {
        let side = hairSide
        let px = try stretched(picture, side: side, context: context)
        let out = try model.prediction(from: MLDictionaryFeatureProvider(dictionary: ["image": MLFeatureValue(pixelBuffer: px)]))
        guard let hair = out.featureValue(for: "hair")?.multiArrayValue else { throw Failure.unreadable("the hair model gave no matte") }
        return try matte(floats(hair), width: side, height: side)
    }

    /// The whole picture stretched to a side×side BGRA buffer, as MediaPipe's segmenters and EdgeTAM take frames.
    /// Lanczos rather than a plain scale: a 1920-high frame shrinks 2–4× and would alias.
    static func stretched(_ picture: CIImage, side: Int, context: CIContext) throws -> CVPixelBuffer {
        let extent = picture.extent
        let origin = picture.transformed(by: CGAffineTransform(translationX: -extent.minX, y: -extent.minY))
        let image = extent.width == CGFloat(side) && extent.height == CGFloat(side) ? origin
            : origin.applyingFilter("CILanczosScaleTransform", parameters: [
                kCIInputScaleKey: CGFloat(side) / extent.height,
                kCIInputAspectRatioKey: (CGFloat(side) / extent.width) / (CGFloat(side) / extent.height)])
        let px = try square(side)
        context.render(image, to: px)
        return px
    }

    /// A matte, 8-bit gray (255 = the subject), from probabilities 0–1 in rows from the top.
    static func matte(_ p: [Float], width: Int, height: Int) throws -> CVPixelBuffer {
        var made: CVPixelBuffer?
        CVPixelBufferCreate(nil, width, height, kCVPixelFormatType_OneComponent8, nil, &made)
        guard let mask = made else { throw Failure.unreadable("no memory for a matte") }
        CVPixelBufferLockBaseAddress(mask, [])
        defer { CVPixelBufferUnlockBaseAddress(mask, []) }
        let row = CVPixelBufferGetBytesPerRow(mask)
        guard let base = CVPixelBufferGetBaseAddress(mask)?.assumingMemoryBound(to: UInt8.self) else { throw Failure.unreadable("no memory for a matte") }
        for y in 0..<height {
            for x in 0..<width { base[y * row + x] = UInt8(max(0, min(255, (p[y * width + x] * 255).rounded()))) }
        }
        return mask
    }

    /// An empty matte (nothing tracked on this frame).
    static func blankMatte() throws -> CVPixelBuffer { try matte([Float](repeating: 0, count: 16 * 16), width: 16, height: 16) }

    // MARK: objects (EdgeTAM: a box or click on one frame, then the object's mask on every later frame;
    // models/edgetam/edgetam.feature, "The Swift side runs SAM 2's tracking loop". Everything is in the 1024×1024 stretched frame.)

    struct ObjectModels { var encoder: MLModel; var decoder: MLModel; var attention: MLModel; var memory: MLModel }

    static func objectModels() throws -> ObjectModels {
        ObjectModels(encoder: try model("EdgeTAMImageEncoder", folder: "edgetam"), decoder: try model("EdgeTAMMaskDecoder", folder: "edgetam"),
                     attention: try model("EdgeTAMMemoryAttention", folder: "edgetam"),
                     // fp32: it can't use the Neural Engine, and is fastest on the GPU.
                     memory: try model("EdgeTAMMemoryEncoder", folder: "edgetam", units: .cpuAndGPU))
    }

    /// One object's EdgeTAM state: the prompted frame's memory and pointer, and the recent frames' (newest last).
    final class ObjectTracker {
        let models: ObjectModels
        private var prompted: (memory: [Float], pointer: [Float])?
        private var recent: [(memory: [Float], pointer: [Float])] = []
        static let side = 1024, memorySlots = 7, pointerSlots = 16, memorySize = 512 * 64, pointerSize = 256

        init(models: ObjectModels) { self.models = models }

        /// The next frame's matte at width×height (255 = the object; empty when EdgeTAM says it's out of view):
        /// from the prompt on the prompted frame, else tracked from memory.
        func step(_ picture: CIImage, prompt: ObjectPrompt?, width: Int, height: Int, context: CIContext) throws -> CVPixelBuffer {
            let side = Double(Self.side)
            let frame = try models.encoder.prediction(from: MLDictionaryFeatureProvider(dictionary: [
                "image": MLFeatureValue(pixelBuffer: try Perception.stretched(picture, side: Self.side, context: context))]))
            guard let vf = frame.featureValue(for: "vision_features"), let h0 = frame.featureValue(for: "high_res_feat_0"),
                  let h1 = frame.featureValue(for: "high_res_feat_1") else { throw Failure.unreadable("EdgeTAM's encoder gave nothing") }
            var points: [(Double, Double, Float)] = [(0, 0, -1)]  // a tracking frame's dummy point
            var pix = vf
            if let prompt {
                if let b = prompt.box { points = [(b[0] * side, b[1] * side, 2), ((b[0] + b[2]) * side, (b[1] + b[3]) * side, 3)] }
                if let p = prompt.point { points = [(p[0] * side, p[1] * side, 1)] }
                recent = []
            } else {
                guard let prompted else { throw Failure.unreadable("EdgeTAM was asked to track before it was shown the object") }
                // The bank: slot 0 the prompted frame, slot s the frame 7 − s back; pointers: slot d the frame d back.
                let memory = try MLMultiArray(shape: [1, NSNumber(value: Self.memorySlots), 512, 64], dataType: .float32)
                let memoryValid = try MLMultiArray(shape: [1, NSNumber(value: Self.memorySlots)], dataType: .float32)
                let pointers = try MLMultiArray(shape: [1, NSNumber(value: Self.pointerSlots), 256], dataType: .float32)
                let pointerValid = try MLMultiArray(shape: [1, NSNumber(value: Self.pointerSlots)], dataType: .float32)
                for a in [memory, memoryValid, pointers, pointerValid] { Self.fill(a, at: 0, with: [Float](repeating: 0, count: a.count)) }
                Self.fill(memory, at: 0, with: prompted.memory)
                Self.fill(memoryValid, at: 0, with: [1])
                Self.fill(pointers, at: 0, with: prompted.pointer)
                Self.fill(pointerValid, at: 0, with: [1])
                for s in 1..<Self.memorySlots where recent.count - (Self.memorySlots - s) >= 0 {
                    Self.fill(memory, at: s * Self.memorySize, with: recent[recent.count - (Self.memorySlots - s)].memory)
                    Self.fill(memoryValid, at: s, with: [1])
                }
                for d in 1..<Self.pointerSlots where recent.count - d >= 0 {
                    Self.fill(pointers, at: d * Self.pointerSize, with: recent[recent.count - d].pointer)
                    Self.fill(pointerValid, at: d, with: [1])
                }
                let attended = try models.attention.prediction(from: MLDictionaryFeatureProvider(dictionary: [
                    "vision_features": vf, "memory": MLFeatureValue(multiArray: memory), "memory_valid": MLFeatureValue(multiArray: memoryValid),
                    "obj_ptrs": MLFeatureValue(multiArray: pointers), "obj_ptr_valid": MLFeatureValue(multiArray: pointerValid)]))
                guard let p = attended.featureValue(for: "pix_feat") else { throw Failure.unreadable("EdgeTAM's memory attention gave nothing") }
                pix = p
            }
            let coords = try MLMultiArray(shape: [1, NSNumber(value: points.count), 2], dataType: .float32)
            let labels = try MLMultiArray(shape: [1, NSNumber(value: points.count)], dataType: .float32)
            Self.fill(coords, at: 0, with: points.flatMap { [Float($0.0), Float($0.1)] })
            Self.fill(labels, at: 0, with: points.map(\.2))
            let decoded = try models.decoder.prediction(from: MLDictionaryFeatureProvider(dictionary: [
                "pix_feat": pix, "high_res_feat_0": h0, "high_res_feat_1": h1,
                "point_coords": MLFeatureValue(multiArray: coords), "point_labels": MLFeatureValue(multiArray: labels),
                "add_no_mem": MLFeatureValue(multiArray: try Self.scalar(prompt == nil ? 0 : 1)),
                // One click, and every tracking frame, picks the best of three masks; a box takes the single mask.
                "multimask": MLFeatureValue(multiArray: try Self.scalar(prompt?.box == nil ? 1 : 0))]))
            guard let logits = decoded.featureValue(for: "mask_logits"), let pointer = decoded.featureValue(for: "obj_ptr")?.multiArrayValue,
                  let score = decoded.featureValue(for: "object_score_logit")?.multiArrayValue else { throw Failure.unreadable("EdgeTAM's decoder gave nothing") }
            let remembered = try models.memory.prediction(from: MLDictionaryFeatureProvider(dictionary: [
                "vision_features": vf, "mask_logits": logits,
                "binarize": MLFeatureValue(multiArray: try Self.scalar(prompt == nil ? 0 : 1))]))
            guard let memory = remembered.featureValue(for: "memory")?.multiArrayValue else { throw Failure.unreadable("EdgeTAM's memory encoder gave nothing") }
            let kept = (memory: Perception.floats(memory), pointer: Perception.floats(pointer))
            if prompt != nil { prompted = kept } else {
                recent.append(kept)
                if recent.count > Self.pointerSlots - 1 { recent.removeFirst(recent.count - (Self.pointerSlots - 1)) }
            }
            guard (Perception.floats(score).first ?? 0) > 0, let l = logits.multiArrayValue else { return try Perception.blankMatte() }
            // The logits resized bilinearly (as the official predictor's F.interpolate, align_corners=False), then
            // the sigmoid: resizing probabilities instead would move the edge.
            let logit = Perception.floats(l), n = 256
            var alpha = [Float](repeating: 0, count: width * height)
            for y in 0..<height {
                let sy = max(0, min(Float(n - 1), (Float(y) + 0.5) * Float(n) / Float(height) - 0.5))
                let (y0, fy) = (Int(sy), sy - Float(Int(sy)))
                let y1 = min(n - 1, y0 + 1)
                for x in 0..<width {
                    let sx = max(0, min(Float(n - 1), (Float(x) + 0.5) * Float(n) / Float(width) - 0.5))
                    let (x0, fx) = (Int(sx), sx - Float(Int(sx)))
                    let x1 = min(n - 1, x0 + 1)
                    let v = (logit[y0 * n + x0] * (1 - fx) + logit[y0 * n + x1] * fx) * (1 - fy)
                        + (logit[y1 * n + x0] * (1 - fx) + logit[y1 * n + x1] * fx) * fy
                    alpha[y * width + x] = 1 / (1 + exp(-v))
                }
            }
            return try Perception.matte(alpha, width: width, height: height)
        }

        private static func scalar(_ v: Float) throws -> MLMultiArray {
            let a = try MLMultiArray(shape: [1], dataType: .float32)
            fill(a, at: 0, with: [v])
            return a
        }

        /// Writes values into a fresh (contiguous) float32 array from a flat offset.
        private static func fill(_ a: MLMultiArray, at offset: Int, with values: [Float]) {
            a.withUnsafeMutableBufferPointer(ofType: Float.self) { p, _ in
                for (i, v) in values.enumerated() { p[offset + i] = v }
            }
        }
    }

    // MARK: face mesh (MediaPipe face landmarker: BlazeFace on the letterboxed frame → a turned 256×256 crop →
    // 478 points → 52 blendshapes; models/mediapipe-face-landmarker/mediapipe-face-landmarker.feature, "How the app should call it")

    struct FaceModels { var detector: MLModel; var landmarks: MLModel; var blendshapes: MLModel }

    static func faceModels() throws -> FaceModels {
        let folder = "mediapipe-face-landmarker"
        return FaceModels(detector: try model("FaceDetector", folder: folder), landmarks: try model("FaceLandmarks", folder: folder),
                          blendshapes: try model("FaceBlendshapes", folder: folder))
    }

    /// The square a face is read from: centre and side in the frame's pixels (y down), turned `rotation` radians
    /// clockwise. Crop pixel (u, v) is centre + R(rotation)·((u, v) − 128)·side/256.
    struct FaceCrop { var cx: Double; var cy: Double; var side: Double; var rotation: Double }

    /// A face read: 478 points [x, y, z] in the frame's pixels (z at x's scale, smaller = nearer) and 52 blendshapes.
    struct FaceMesh { var points: [[Double]]; var blendshapes: [Float] }

    /// BlazeFace's best face, after MediaPipe's weighted non-max suppression, as the crop to read it from; nil: none.
    /// `largest`: the biggest face over the score instead of the surest, for the creator in front when someone stands
    /// behind them (performance, T-332).
    static func detectFace(_ picture: CIImage, models: FaceModels, context: CIContext, largest: Bool = false) throws -> FaceCrop? {
        let side = 128, extent = picture.extent
        let scale = min(Double(side) / extent.width, Double(side) / extent.height)
        let (cw, ch) = (Int((extent.width * scale).rounded()), Int((extent.height * scale).rounded()))
        let (ox, oy) = ((side - cw) / 2, (side - ch) / 2)  // left and top padding
        let px = try square(side)
        let placed = picture.transformed(by: CGAffineTransform(translationX: -extent.minX, y: -extent.minY))
            .transformed(by: CGAffineTransform(scaleX: scale, y: scale))
            .transformed(by: CGAffineTransform(translationX: CGFloat(ox), y: CGFloat(side - oy - ch)))
            .composited(over: CIImage(color: .black).cropped(to: CGRect(x: 0, y: 0, width: side, height: side)))
        context.render(placed, to: px)
        let out = try models.detector.prediction(from: MLDictionaryFeatureProvider(dictionary: ["image": MLFeatureValue(pixelBuffer: px)]))
        guard let s = out.featureValue(for: "scores")?.multiArrayValue, let b = out.featureValue(for: "boxes")?.multiArrayValue,
              let k = out.featureValue(for: "keypoints")?.multiArrayValue else { throw Failure.unreadable("the face detector gave nothing") }
        let (scores, boxes, keys) = (floats(s), floats(b), floats(k))
        let found = scores.indices.filter { scores[$0] >= 0.5 }.sorted { scores[$0] > scores[$1] }
        let area = { (i: Int) in boxes[i * 4 + 2] * boxes[i * 4 + 3] }
        guard let best = largest ? found.max(by: { area($0) < area($1) }) : found.first else { return nil }
        func iou(_ i: Int, _ j: Int) -> Float {
            let (a, c) = (i * 4, j * 4)
            let w = max(0, min(boxes[a] + boxes[a + 2], boxes[c] + boxes[c + 2]) - max(boxes[a], boxes[c]))
            let h = max(0, min(boxes[a + 1] + boxes[a + 3], boxes[c + 1] + boxes[c + 3]) - max(boxes[a + 1], boxes[c + 1]))
            let union = boxes[a + 2] * boxes[a + 3] + boxes[c + 2] * boxes[c + 3] - w * h
            return union > 0 ? w * h / union : 0
        }
        // Boxes overlapping the best are averaged with it by score, not dropped (MediaPipe's WEIGHTED suppression).
        let cluster = found.filter { iou(best, $0) > 0.3 }
        let total = cluster.reduce(Float(0)) { $0 + scores[$1] }
        func mean(_ values: [Float], _ stride: Int, _ at: Int) -> Double {
            Double(cluster.reduce(Float(0)) { $0 + values[$1 * stride + at] * scores[$1] } / total)
        }
        // Back to the frame's pixels: x = (x·128 − padding) / scale.
        func x(_ v: Double) -> Double { (v * Double(side) - Double(ox)) / scale }
        func y(_ v: Double) -> Double { (v * Double(side) - Double(oy)) / scale }
        let (left, top) = (x(mean(boxes, 4, 0)), y(mean(boxes, 4, 1)))
        let (w, h) = (mean(boxes, 4, 2) * Double(side) / scale, mean(boxes, 4, 3) * Double(side) / scale)
        // Keypoints 0 and 1: the subject's right eye, then left.
        let (rx, ry, lx, ly) = (x(mean(keys, 12, 0)), y(mean(keys, 12, 1)), x(mean(keys, 12, 2)), y(mean(keys, 12, 3)))
        return FaceCrop(cx: left + w / 2, cy: top + h / 2, side: 1.5 * max(w, h), rotation: -atan2(-(ly - ry), lx - rx))
    }

    /// The face in `crop`: its points and blendshapes, or nil when the landmark model sees no face there.
    static func faceMesh(_ picture: CIImage, crop: FaceCrop, models: FaceModels, context: CIContext) throws -> FaceMesh? {
        let side = 256, extent = picture.extent, k = crop.side / Double(side)
        // Frame → crop, in Core Image's y-up space: flip to y down, about the crop's centre, turned back, scaled to
        // the crop, flipped up again.
        let toCrop = CGAffineTransform(translationX: -extent.minX, y: -extent.minY)
            .concatenating(CGAffineTransform(a: 1, b: 0, c: 0, d: -1, tx: 0, ty: extent.height))
            .concatenating(CGAffineTransform(translationX: -crop.cx, y: -crop.cy))
            .concatenating(CGAffineTransform(rotationAngle: -crop.rotation))
            .concatenating(CGAffineTransform(scaleX: 1 / k, y: 1 / k))
            .concatenating(CGAffineTransform(translationX: CGFloat(side) / 2, y: CGFloat(side) / 2))
            .concatenating(CGAffineTransform(a: 1, b: 0, c: 0, d: -1, tx: 0, ty: CGFloat(side)))
        let px = try square(side)
        context.render(picture.transformed(by: toCrop)
            .composited(over: CIImage(color: .black).cropped(to: CGRect(x: 0, y: 0, width: side, height: side))), to: px)
        let out = try models.landmarks.prediction(from: MLDictionaryFeatureProvider(dictionary: ["image": MLFeatureValue(pixelBuffer: px)]))
        guard let l = out.featureValue(for: "landmarks")?.multiArrayValue, let presence = out.featureValue(for: "presence")?.multiArrayValue
        else { throw Failure.unreadable("the face landmark model gave nothing") }
        guard (floats(presence).first ?? 0) >= 0.5 else { return nil }
        let raw = floats(l), (c, s) = (cos(crop.rotation), sin(crop.rotation)), half = Double(side) / 2
        let points: [[Double]] = (0..<478).map { i in
            let (u, v) = (Double(raw[i * 3]) - half, Double(raw[i * 3 + 1]) - half)
            return [crop.cx + (c * u - s * v) * k, crop.cy + (s * u + c * v) * k, Double(raw[i * 3 + 2]) * k]
        }
        // Blendshapes read 146 of the points, in the frame's pixels.
        let subset = try MLMultiArray(shape: [1, 146, 2], dataType: .float32)
        for (j, i) in blendshapePoints.enumerated() {
            subset[[0, j, 0] as [NSNumber]] = NSNumber(value: points[i][0])
            subset[[0, j, 1] as [NSNumber]] = NSNumber(value: points[i][1])
        }
        let shapes = try models.blendshapes.prediction(from: MLDictionaryFeatureProvider(dictionary: ["points": MLFeatureValue(multiArray: subset)]))
        guard let scores = shapes.featureValue(for: "blendshapes")?.multiArrayValue else { throw Failure.unreadable("the blendshape model gave nothing") }
        return FaceMesh(points: points, blendshapes: floats(scores))
    }

    /// The next frame's crop from this frame's points, as MediaPipe follows a face: the box around them, turned
    /// from the right eye's outer corner (point 33) to the left's (263), its longer side × 1.5.
    static func crop(around points: [[Double]]) -> FaceCrop {
        let xs = points.map { $0[0] }, ys = points.map { $0[1] }
        let (x0, x1, y0, y1) = (xs.min() ?? 0, xs.max() ?? 0, ys.min() ?? 0, ys.max() ?? 0)
        let (r, l) = (points[33], points[263])
        return FaceCrop(cx: (x0 + x1) / 2, cy: (y0 + y1) / 2, side: 1.5 * max(x1 - x0, y1 - y0),
                        rotation: -atan2(-(l[1] - r[1]), l[0] - r[0]))
    }

    /// A face as the track has it: normalized to the frame (z by its width, as MediaPipe does).
    static func faceBox(_ mesh: FaceMesh, in size: CGSize) -> FaceBox {
        let (w, h) = (Double(size.width), Double(size.height))
        let xs = mesh.points.map { $0[0] }, ys = mesh.points.map { $0[1] }
        let (x0, x1, y0, y1) = (xs.min() ?? 0, xs.max() ?? 0, ys.min() ?? 0, ys.max() ?? 0)
        let r4 = { (x: Double) in (x * 10_000).rounded() / 10_000 }
        var shapes: [String: Double] = [:]
        for (name, score) in zip(blendshapeNames, mesh.blendshapes) where name != "_neutral" { shapes[name] = (Double(score) * 1000).rounded() / 1000 }
        return FaceBox(box: [r4(x0 / w), r4(y0 / h), r4((x1 - x0) / w), r4((y1 - y0) / h)],
                       points: mesh.points.map { [r4($0[0] / w), r4($0[1] / h), r4($0[2] / w)] }, blendshapes: shapes)
    }

    /// The 146 points the blendshape model reads (kLandmarksSubsetIdxs in MediaPipe's face_blendshapes_graph.cc).
    static let blendshapePoints = [
        0, 1, 4, 5, 6, 7, 8, 10, 13, 14, 17, 21, 33, 37, 39, 40, 46, 52, 53, 54, 55, 58, 61, 63, 65, 66, 67, 70, 78, 80, 81, 82,
        84, 87, 88, 91, 93, 95, 103, 105, 107, 109, 127, 132, 133, 136, 144, 145, 146, 148, 149, 150, 152, 153, 154, 155, 157,
        158, 159, 160, 161, 162, 163, 168, 172, 173, 176, 178, 181, 185, 191, 195, 197, 234, 246, 249, 251, 263, 267, 269, 270,
        276, 282, 283, 284, 285, 288, 291, 293, 295, 296, 297, 300, 308, 310, 311, 312, 314, 317, 318, 321, 323, 324, 332, 334,
        336, 338, 356, 361, 362, 365, 373, 374, 375, 377, 378, 379, 380, 381, 382, 384, 385, 386, 387, 388, 389, 390, 397, 398,
        400, 402, 405, 409, 415, 454, 466, 468, 469, 470, 471, 472, 473, 474, 475, 476, 477,
    ]

    /// The blendshape model's 52 outputs, in order (ARKit's names after `_neutral`).
    static let blendshapeNames = """
        _neutral browDownLeft browDownRight browInnerUp browOuterUpLeft browOuterUpRight cheekPuff cheekSquintLeft \
        cheekSquintRight eyeBlinkLeft eyeBlinkRight eyeLookDownLeft eyeLookDownRight eyeLookInLeft eyeLookInRight \
        eyeLookOutLeft eyeLookOutRight eyeLookUpLeft eyeLookUpRight eyeSquintLeft eyeSquintRight eyeWideLeft \
        eyeWideRight jawForward jawLeft jawOpen jawRight mouthClose mouthDimpleLeft mouthDimpleRight mouthFrownLeft \
        mouthFrownRight mouthFunnel mouthLeft mouthLowerDownLeft mouthLowerDownRight mouthPressLeft mouthPressRight \
        mouthPucker mouthRight mouthRollLower mouthRollUpper mouthShrugLower mouthShrugUpper mouthSmileLeft \
        mouthSmileRight mouthStretchLeft mouthStretchRight mouthUpperUpLeft mouthUpperUpRight noseSneerLeft noseSneerRight
        """.split(separator: " ").map(String.init)

    // MARK: taste vectors (SigLIP 2 B/16-256, Apache-2.0, T-410): the same picture space as the research index
    // (turbopuffer ugc-ad-cards-256) and the taste label tables (knowledge/labels/*.table.json). The 8-bit Core ML build
    // (input `image` 256×256 RGB squashed, output `embedding` [1,768]) is an optional pack, not in the bundle. Neural
    // Engine only on the phone: the GPU overflows fp16 (cosine 0.65). The Mac runs it on the CPU (no Neural Engine
    // compiles of big models on the shared Mac) and only with UGC_TASTE=1, so the evals' perceive stays fast.

    struct Taste: Codable {
        var model = "siglip2-base-p16-256"
        /// When each frame was taken (2 a second), and its L2-normalised vector.
        var times: [Double]
        var frames: [[Float]]
        /// Each shot's mean frame vector (normalised), in facts.json's shot order.
        var shots: [[Float]]
        /// The whole clip's mean (normalised): what a take or reference is ranked by.
        var clip: [Float]
        /// What it sounds like (YAMNet), when it has a sound track and the model is installed.
        var sound: SoundVector?
    }

    /// YAMNet (Apache-2.0, models/yamnet/convert.py): AudioSet's 521 sound classes and a 1,024-number sound vector for
    /// every 0.96 s patch (one each 0.48 s). One Core ML model takes 10 s of 16 kHz mono (`waveform` [160000]) and
    /// gives `scores` [20,521] and `embeddings` [20,1024]; the class names are in its metadata. The last window is
    /// padded with silence and its patches past the end dropped. Matches the TensorFlow original at cosine 1.0.
    struct SoundVector: Codable {
        var model = "yamnet-audioset"
        /// The mean patch vector (normalised): the sound side of taste (music, effects, voice, room).
        var vector: [Float]
        /// The ten likeliest classes over the whole clip, mean score each.
        var labels: [SoundLabel]
        /// The likeliest class every 0.48 s, for when things are heard.
        var timeline: [String]
    }

    struct SoundLabel: Codable { var name: String; var score: Double }

    static func soundModel() throws -> MLModel {
        // Small (16 MB) and fp32: the CPU keeps it exact (the log-mel front end is inside).
        try model("YAMNet", folder: "yamnet", units: .cpuOnly)
    }

    static func hear(_ url: URL) async throws -> SoundVector? {
        lap("sound: decoding")
        let pcm = try await pcm16k(AVURLAsset(url: url))
        lap("sound: \(pcm.count) samples")
        guard pcm.count >= 16_000 / 2 else { return nil }
        let model = try soundModel()
        lap("sound: YAMNet loaded")
        let classes = (model.modelDescription.metadata[.creatorDefinedKey] as? [String: String])?["classes"]
            .flatMap { try? JSONDecoder().decode([String].self, from: Data($0.utf8)) } ?? []
        let window = 160_000, patchHop = 7_680, patchLength = 15_360
        var vectors: [[Float]] = [], scores: [[Float]] = []
        for start in stride(from: 0, to: pcm.count, by: window) {
            let input = try MLMultiArray(shape: [NSNumber(value: window)], dataType: .float32)
            let count = min(window, pcm.count - start)
            input.withUnsafeMutableBufferPointer(ofType: Float.self) { buffer, _ in
                for i in 0..<window { buffer[i] = i < count ? pcm[start + i] : 0 }
            }
            let out = try await model.prediction(from: MLDictionaryFeatureProvider(dictionary: ["waveform": input]))
            guard let e = out.featureValue(for: "embeddings")?.multiArrayValue, let sc = out.featureValue(for: "scores")?.multiArrayValue else { continue }
            let (ev, sv) = (floats(e), floats(sc))
            let patches = sv.count / 521
            // a patch counts while most of it is real sound
            let real = max(1, min(patches, (count - patchLength / 2) / patchHop + 1))
            for p in 0..<real {
                vectors.append(Array(ev[p * 1024..<(p + 1) * 1024]))
                scores.append(Array(sv[p * 521..<(p + 1) * 521]))
            }
        }
        lap("sound: \(vectors.count) patches")
        guard !vectors.isEmpty else { return nil }
        let meanScores = mean(scores)
        let name = { (i: Int) in i < classes.count ? classes[i] : "class \(i)" }
        let top = meanScores.indices.sorted { meanScores[$0] > meanScores[$1] }.prefix(10)
        return SoundVector(vector: rounded(normalised(mean(vectors.map(normalised)))),
                     labels: top.map { SoundLabel(name: name($0), score: (Double(meanScores[$0]) * 1000).rounded() / 1000) },
                     timeline: scores.map { s in name(s.indices.max { s[$0] < s[$1] } ?? 0) })
    }

    static var tasteOn: Bool {
        #if os(macOS)
        ProcessInfo.processInfo.environment["UGC_TASTE"] != nil
        #else
        true
        #endif
    }

    static func tasteModel() throws -> MLModel {
        #if os(macOS)
        try model("SigLIP2Image", folder: "siglip2-base-p16-256", units: .cpuOnly)
        #else
        try model("SigLIP2Image", folder: "siglip2-base-p16-256", units: .cpuAndNeuralEngine)
        #endif
    }

    /// Frames at 2 a second through SigLIP; about 1.5 s for a 30 s ad on an iPhone 13 Pro (16 ms a frame).
    static func taste(_ url: URL, shots: [Shot]) async throws -> Taste {
        lap("taste: loading SigLIP")
        let model = try tasteModel()
        lap("taste: SigLIP loaded")
        guard let input = model.modelDescription.inputDescriptionsByName.first?.value, let constraint = input.imageConstraint else {
            throw Failure.unreadable("SigLIP2Image takes no picture")
        }
        let asset = AVURLAsset(url: url)
        let duration = try await asset.load(.duration).seconds
        let generator = AVAssetImageGenerator(asset: asset)
        generator.appliesPreferredTrackTransform = true
        // Frames within one frame (1/30 s) of each time, at most 512 px, squashed to 256×256 here with a smoothing
        // resize (the training side's PIL bilinear). Exact full-size frames cost 83 s for a 26 s ad on an iPhone 13
        // Pro (each decoded from its keyframe) against ~1 s, for no gain: neighbouring frames agree at 0.98 anyway.
        generator.maximumSize = CGSize(width: 512, height: 512)
        generator.requestedTimeToleranceBefore = .zero
        generator.requestedTimeToleranceAfter = .zero
        let wanted = stride(from: 0.25, to: max(duration, 0.26), by: 0.5).map { CMTime(seconds: $0, preferredTimescale: 600) }
        var times: [Double] = [], frames: [[Float]] = []
        for await result in generator.images(for: wanted) {
            guard let frame = try? result.image, let image = squashed(frame, side: 256) else { continue }
            let value = try MLFeatureValue(cgImage: image, constraint: constraint,
                                           options: [.cropAndScale: VNImageCropAndScaleOption.scaleFill.rawValue as Any])
            let out = try await model.prediction(from: MLDictionaryFeatureProvider(dictionary: [input.name: value]))
            guard let a = out.featureValue(for: "embedding")?.multiArrayValue else { continue }
            times.append(r2(result.requestedTime.seconds))
            frames.append(normalised(floats(a)))
        }
        lap("taste: \(frames.count) frames")
        guard !frames.isEmpty else { throw Failure.unreadable("\(url.lastPathComponent): no frames for SigLIP") }
        let perShot = shots.map { shot in
            let inside = zip(times, frames).filter { $0.0 >= shot.start && $0.0 < max(shot.end, shot.start + 0.01) }.map(\.1)
            return inside.isEmpty ? [] : rounded(normalised(mean(inside)))
        }
        let sound = try? await hear(url)
        return Taste(times: times, frames: frames.map(rounded), shots: perShot, clip: rounded(normalised(mean(frames))), sound: sound)
    }

    static func squashed(_ image: CGImage, side: Int) -> CGImage? {
        // In the frame's own colour space: no conversion, the pixels as decoded (ffmpeg's, which the vectors learned from)
        let space = image.colorSpace.flatMap { $0.model == .rgb ? $0 : nil } ?? CGColorSpace(name: CGColorSpace.sRGB)!
        guard let ctx = CGContext(data: nil, width: side, height: side, bitsPerComponent: 8, bytesPerRow: side * 4,
                                  space: space, bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue) else { return nil }
        ctx.interpolationQuality = .high
        ctx.draw(image, in: CGRect(x: 0, y: 0, width: side, height: side))
        return ctx.makeImage()
    }

    static func mean(_ vs: [[Float]]) -> [Float] {
        var m = [Float](repeating: 0, count: vs.first?.count ?? 0)
        for v in vs { for i in m.indices { m[i] += v[i] } }
        return m.map { $0 / Float(max(vs.count, 1)) }
    }

    static func normalised(_ v: [Float]) -> [Float] {
        let n = v.reduce(0) { $0 + $1 * $1 }.squareRoot()
        return n > 0 ? v.map { $0 / n } : v
    }

    /// Five decimals: plenty for cosines, and taste.json stays about 5 KB a frame.
    static func rounded(_ v: [Float]) -> [Float] { v.map { ($0 * 100_000).rounded() / 100_000 } }

    // MARK: model plumbing

    /// Core Image without colour management, for model inputs and mattes. The models learned from decoded pixels as
    /// they are (PIL, ffmpeg); a managed context turns BT.709 video into sRGB, ~5% brighter, and EdgeTAM's masks drift.
    static func modelContext() -> CIContext {
        CIContext(options: [.workingColorSpace: NSNull(), .outputColorSpace: NSNull(), .cacheIntermediates: false])
    }

    /// A side×side BGRA buffer for a model's image input.
    static func square(_ side: Int) throws -> CVPixelBuffer {
        var px: CVPixelBuffer?
        CVPixelBufferCreate(nil, side, side, kCVPixelFormatType_32BGRA, [kCVPixelBufferCGImageCompatibilityKey: true] as CFDictionary, &px)
        guard let px else { throw Failure.unreadable("no memory for a \(side)×\(side) frame") }
        return px
    }

    /// A multi-array's values in logical (row-major) order, read through its strides: Core ML pads rows.
    static func floats(_ a: MLMultiArray) -> [Float] {
        let shape = a.shape.map(\.intValue), strides = a.strides.map(\.intValue)
        let count = shape.reduce(1, *)
        var out = [Float](repeating: 0, count: count)
        func walk<T>(_ p: UnsafeBufferPointer<T>, _ value: (T) -> Float) {
            var index = [Int](repeating: 0, count: shape.count)
            for i in 0..<count {
                var offset = 0
                for d in 0..<shape.count { offset += index[d] * strides[d] }
                out[i] = value(p[offset])
                var d = shape.count - 1
                while d >= 0 {
                    index[d] += 1
                    if index[d] < shape[d] { break }
                    index[d] = 0
                    d -= 1
                }
            }
        }
        switch a.dataType {
        case .double: a.withUnsafeBufferPointer(ofType: Double.self) { walk($0) { Float($0) } }
        #if arch(arm64)
        case .float16: a.withUnsafeBufferPointer(ofType: Float16.self) { walk($0) { Float($0) } }
        #endif
        default: a.withUnsafeBufferPointer(ofType: Float.self) { walk($0) { $0 } }
        }
        return out
    }

    /// A matte as the track format has it: PNG gray + alpha, gray 255, alpha = the matte (255 = the subject).
    static func writeMatte(_ mask: CVPixelBuffer, width: Int, height: Int, context: CIContext, to url: URL) throws {
        // No colour space: a matte is coverage, not sRGB; Core Image would linearize it and shrink every edge.
        let m = CIImage(cvPixelBuffer: mask, options: [.colorSpace: NSNull()])
        let scaled = m.transformed(by: CGAffineTransform(scaleX: CGFloat(width) / m.extent.width, y: CGFloat(height) / m.extent.height))
        var gray = [UInt8](repeating: 0, count: width * height)
        context.render(scaled, toBitmap: &gray, rowBytes: width, bounds: CGRect(x: 0, y: 0, width: width, height: height),
                       format: .L8, colorSpace: nil)
        var pixels = [UInt8](repeating: 255, count: width * height * 2)
        for i in 0..<gray.count { pixels[i * 2 + 1] = gray[i] }
        guard let provider = CGDataProvider(data: Data(pixels) as CFData),
              let image = CGImage(width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 16, bytesPerRow: width * 2,
                                  space: CGColorSpaceCreateDeviceGray(), bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.last.rawValue),
                                  provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent),
              let dest = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil) else {
            throw Failure.unreadable("couldn't write \(url.lastPathComponent)")
        }
        CGImageDestinationAddImage(dest, image, nil)
        guard CGImageDestinationFinalize(dest) else { throw Failure.unreadable("couldn't write \(url.lastPathComponent)") }
    }

    /// The EXIF orientation that makes a track's preferred transform upright.
    static func orientation(_ t: CGAffineTransform) -> CGImagePropertyOrientation {
        switch (t.a, t.b, t.c, t.d) {
        case (0, 1, -1, 0): return .right
        case (0, -1, 1, 0): return .left
        case (-1, 0, 0, -1): return .down
        default: return .up
        }
    }

    // MARK: layout (docs/previz.feature): how a frame is laid out, in the previz layout format (framework/src/previz.rs)
    // Faces and the people around them, captions, screens and slides, face bubbles, and stacked / side-by-side / grid /
    // diagonal splits and framed (letterboxed) video. Splits must hold still over the next frames while the picture
    // changes: a moving camera moves a shelf edge or a horizon with it, a split's seam stays put.

    struct Layout: Codable, Equatable {
        /// 9:16 | 4:5 | 1:1 | 16:9, the nearest to the frame's shape.
        var ratio: String
        var split: LayoutSplit
        var zones: [LayoutZone]
    }

    struct LayoutSplit: Codable, Equatable {
        /// none | stacked | side-by-side | grid | diagonal
        var kind: String
        /// A diagonal's angle from horizontal in degrees, rising to the right.
        var angle: Double?
    }

    struct LayoutBox: Codable, Equatable { var x, y, w, h: Double }

    /// A box in 0...1 from the top left, with what it holds.
    struct LayoutZone: Codable, Equatable {
        /// subject (a person) | text (words laid over the picture) | video (a pane, a screen or a framed video)
        var role: String
        var x, y, w, h: Double
        /// A subject's face.
        var face: LayoutBox?
        /// How many words a text zone holds.
        var words: Int?
        /// face bubble | on screen | person | screen | pane | framed video, or a text zone's first words.
        var label: String?
    }

    struct LRect {
        var x, y, w, h: Double
        var x1: Double { x + w }
        var y1: Double { y + h }
        var area: Double { max(w, 0) * max(h, 0) }
        func union(_ o: LRect) -> LRect { let a = min(x, o.x), b = min(y, o.y); return LRect(x: a, y: b, w: max(x1, o.x1) - a, h: max(y1, o.y1) - b) }
        func inter(_ o: LRect) -> Double { max(0, min(x1, o.x1) - max(x, o.x)) * max(0, min(y1, o.y1) - max(y, o.y)) }
        func contains(_ o: LRect, share: Double = 0.8) -> Bool { o.area > 0 && inter(o) / o.area >= share }
        func clamped() -> LRect { let a = max(0, x), b = max(0, y); return LRect(x: a, y: b, w: min(1, x1) - a, h: min(1, y1) - b) }
        init(x: Double, y: Double, w: Double, h: Double) { (self.x, self.y, self.w, self.h) = (x, y, w, h) }
        /// From Vision's box (origin bottom left).
        init(_ b: CGRect) { (x, y, w, h) = (b.minX, 1 - b.maxY, b.width, b.height) }
        func zone(_ role: String, face: LRect? = nil, words: Int? = nil, label: String? = nil) -> LayoutZone {
            LayoutZone(role: role, x: r3(x), y: r3(y), w: r3(w), h: r3(h), face: face.map { LayoutBox(x: r3($0.x), y: r3($0.y), w: r3($0.w), h: r3($0.h)) },
                       words: words, label: label)
        }
    }

    static func r3(_ x: Double) -> Double { (x * 1000).rounded() / 1000 }

    /// A small grey copy of the frame (240 px wide) for the seam search.
    struct LGrey {
        let w: Int, h: Int, v: [Float]
        subscript(x: Int, y: Int) -> Float { v[min(max(y, 0), h - 1) * w + min(max(x, 0), w - 1)] }
        func at(_ x: Double, _ y: Double) -> Float { self[Int(x.rounded()), Int(y.rounded())] }
        init(_ image: CGImage, width: Int = 240) {
            w = width
            h = max(1, Int(Double(width) * Double(image.height) / Double(image.width)))
            var raw = [UInt8](repeating: 0, count: w * h)
            if let ctx = CGContext(data: &raw, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w,
                                   space: CGColorSpaceCreateDeviceGray(), bitmapInfo: CGImageAlphaInfo.none.rawValue) {
                ctx.interpolationQuality = .medium
                ctx.draw(image, in: CGRect(x: 0, y: 0, width: w, height: h))
            }
            v = raw.map { Float($0) / 255 }
        }
    }

    private typealias Seg = (Double, Double)

    /// Share of a straight line's length that crosses a hard edge (the step across it, 2 px either side).
    /// Stretches under these boxes (grey px) are left out: editors put the caption right on a split's seam.
    private static func coverage(_ g: LGrey, _ p: Seg, _ q: Seg, step: Float = 0.07, skip: [LRect] = []) -> Double {
        let (dx, dy) = (q.0 - p.0, q.1 - p.1), len = (dx * dx + dy * dy).squareRoot()
        guard len > 10 else { return 0 }
        let (nx, ny) = (-dy / len * 2, dx / len * 2)
        let n = Int(len)
        var hit = 0, seen = 0
        for i in 0...n {
            let t = Double(i) / Double(n), x = p.0 + dx * t, y = p.1 + dy * t
            if skip.contains(where: { x >= $0.x - 2 && x <= $0.x1 + 2 && y >= $0.y - 3 && y <= $0.y1 + 3 }) { continue }
            seen += 1
            if abs(g.at(x + nx, y + ny) - g.at(x - nx, y - ny)) > step { hit += 1 }
        }
        // most of the line has to be visible for it to count
        return seen * 2 >= n + 1 ? Double(hit) / Double(seen) : 0
    }

    /// Both sides of a line carry detail: a product's edge against a plain surface isn't a split.
    private static func textured(_ g: LGrey, _ p: Seg, _ q: Seg, min: Float = 0.025) -> Bool {
        let (dx, dy) = (q.0 - p.0, q.1 - p.1), len = (dx * dx + dy * dy).squareRoot()
        let (nx, ny) = (-dy / len, dx / len)
        var side: [Float] = [0, 0], n = 0
        for i in stride(from: 0, through: Int(len), by: 3) {
            let t = Double(i) / len, x = p.0 + dx * t, y = p.1 + dy * t
            for (k, s) in [(0, 1.0), (1, -1.0)] {
                for d in stride(from: 4.0, through: 14.0, by: 2.0) {
                    let (ax, ay) = (x + nx * d * s, y + ny * d * s)
                    side[k] += abs(g.at(ax, ay) - g.at(x + nx * (d + 1) * s + 1, y + ny * (d + 1) * s)) + abs(g.at(ax, ay) - g.at(ax + 1, ay))
                }
            }
            n += 6
        }
        return side.allSatisfy { $0 / Float(Swift.max(n, 1)) > min }
    }

    /// How much the picture carries on across a line: the correlation of the detail along it either side, at a few
    /// distances. A seam between two pictures is near 0; a shelf edge with the same uprights running across is high.
    private static func carriesOver(_ g: LGrey, _ p: Seg, _ q: Seg) -> Double {
        let (dx, dy) = (q.0 - p.0, q.1 - p.1), len = (dx * dx + dy * dy).squareRoot()
        guard len > 10 else { return 1 }
        let (ux, uy) = (dx / len, dy / len), (nx, ny) = (-uy, ux)
        return [4.0, 8.0, 12.0].map { off -> Double in
            var a: [Double] = [], b: [Double] = []
            for i in 1..<(Int(len) - 1) {
                let (x, y) = (p.0 + ux * Double(i), p.1 + uy * Double(i))
                func along(_ o: Double) -> Double { Double(g.at(x + nx * o + ux, y + ny * o + uy) - g.at(x + nx * o - ux, y + ny * o - uy)) }
                a.append(along(off)); b.append(along(-off))
            }
            let n = Double(a.count), ma = a.reduce(0, +) / n, mb = b.reduce(0, +) / n
            var sab = 0.0, saa = 0.0, sbb = 0.0
            for i in a.indices { sab += (a[i] - ma) * (b[i] - mb); saa += (a[i] - ma) * (a[i] - ma); sbb += (b[i] - mb) * (b[i] - mb) }
            return saa > 1e-9 && sbb > 1e-9 ? sab / (saa * sbb).squareRoot() : 0
        }.max() ?? 1
    }

    /// Panes: cut wherever a straight seam crosses the piece edge to edge, then cut each piece the same way (XY-cut).
    /// The panes in grey px, and the first cut's direction.
    private static func panes(_ g: LGrey, skip: [LRect] = []) -> (panes: [(Double, Double, Double, Double)], first: String?) {
        var first: String? = nil
        func cut(_ r: (Double, Double, Double, Double), _ depth: Int) -> [(Double, Double, Double, Double)] {
            let (x0, y0, x1, y1) = r
            let (w, h) = (x1 - x0, y1 - y0)
            guard depth < 3, w > Double(g.w) * 0.2, h > Double(g.h) * 0.12 else { return [r] }
            var best: (Double, String, Double) = (0, "", 0)
            for y in stride(from: y0 + h * 0.12, to: y1 - h * 0.12, by: 1) {
                let c = coverage(g, (x0 + 2, y), (x1 - 3, y), skip: skip)
                if c > best.0 && c >= 0.9 && carriesOver(g, (x0 + 2, y), (x1 - 3, y)) < 0.4 { best = (c, "row", y) }
            }
            for x in stride(from: x0 + w * 0.12, to: x1 - w * 0.12, by: 1) {
                let c = coverage(g, (x, y0 + 2), (x, y1 - 3), skip: skip)
                if c > best.0 && c >= 0.9 && carriesOver(g, (x, y0 + 2), (x, y1 - 3)) < 0.4 { best = (c, "column", x) }
            }
            guard best.0 >= 0.9 else { return [r] }
            if depth == 0 { first = best.1 }
            let at = best.2
            return best.1 == "row" ? cut((x0, y0, x1, at), depth + 1) + cut((x0, at, x1, y1), depth + 1)
                                   : cut((x0, y0, at, y1), depth + 1) + cut((at, y0, x1, y1), depth + 1)
        }
        let all = cut((0, 0, Double(g.w), Double(g.h)), 0)
        return (all, first)
    }

    /// A pane that holds a picture: a real share of the frame (a quarter of it across and down at least) and not flat.
    /// A banner or a stack of cards laid over one picture leaves only thin strips beside it, which don't count.
    private static func isPicture(_ g: LGrey, _ r: (Double, Double, Double, Double)) -> Bool {
        let (w, h) = ((r.2 - r.0) / Double(g.w), (r.3 - r.1) / Double(g.h))
        return w * h >= 0.1 && min(w, h) >= 0.25 && detail(g, r) > 0.006   // a product on white still has some detail
    }

    /// Mean step between neighbouring pixels in a box: near 0 for a black bar or a flat fill.
    private static func detail(_ g: LGrey, _ r: (Double, Double, Double, Double)) -> Float {
        var s: Float = 0, n = 0
        for y in stride(from: Int(r.1) + 1, to: Int(r.3) - 1, by: 2) {
            for x in stride(from: Int(r.0) + 1, to: Int(r.2) - 1, by: 2) { s += abs(g[x + 1, y] - g[x, y]) + abs(g[x, y + 1] - g[x, y]); n += 1 }
        }
        return n > 0 ? s / Float(n) : 0
    }

    /// The best diagonal seam through the middle band (10-60° either way): (angle as found, place 0...1, coverage).
    private static func diagonal(_ g: LGrey) -> (Double, Double, Double)? {
        let (W, H) = (Double(g.w), Double(g.h))
        var best: (Double, Double, Double)? = nil
        for a in stride(from: 10.0, through: 60.0, by: 1.0) {
            for sign in [-1.0, 1.0] {
                let t = tan(a * sign * .pi / 180)
                for y0 in stride(from: H * 0.3, through: H * 0.7, by: 1.0) {
                    var p = (0.0, y0 - W / 2 * t), q = (W - 1, y0 + W / 2 * t)
                    if p.1 < 0 || p.1 > H - 1 { let e = p.1 < 0 ? 0 : H - 1; p = (W / 2 + (e - y0) / t, e) }
                    if q.1 < 0 || q.1 > H - 1 { let e = q.1 < 0 ? 0 : H - 1; q = (W / 2 + (e - y0) / t, e) }
                    let c = coverage(g, p, q)
                    // straight edges in the world (a roof, a shelf, a curb) score as high as real diagonal seams on one frame, and
                    // real diagonal splits are rare (1 in 204 hand-labelled shots, 2026-10-13): only a near-perfect seam counts
                    if c >= 0.95, c > (best?.2 ?? 0), textured(g, p, q), carriesOver(g, p, q) < 0.4 { best = (a * sign, y0 / H, c) }
                }
            }
        }
        return best
    }

    /// A sharp band between two blurred or flat ones with hard, straight edges: a video framed inside the canvas.
    private static func framedBand(_ g: LGrey) -> (Double, Double)? {
        var row = [Double](repeating: 0, count: g.h)
        for y in 0..<(g.h - 1) {
            var s: Float = 0
            for x in 0..<(g.w - 1) { s += abs(g[x + 1, y] - g[x, y]) + abs(g[x, y + 1] - g[x, y]) }
            row[y] = Double(s) / Double(g.w)
        }
        var pre = [0.0]
        for v in row { pre.append(pre.last! + v) }
        func mean(_ a: Int, _ b: Int) -> Double { b > a ? (pre[b] - pre[a]) / Double(b - a) : 0 }
        var best: (Double, Int, Int) = (0, 0, 0)
        let stepRows = max(1, g.h / 100)
        for a in stride(from: Int(Double(g.h) * 0.08), to: Int(Double(g.h) * 0.5), by: stepRows) {
            for b in stride(from: a + Int(Double(g.h) * 0.25), to: Int(Double(g.h) * 0.92), by: stepRows) {
                let outside = (mean(0, a) * Double(a) + mean(b, g.h) * Double(g.h - b)) / Double(a + g.h - b)
                let r = mean(a, b) / (outside + 0.002)
                if r > best.0 { best = (r, a, b) }
            }
        }
        guard best.0 > 3 else { return nil }
        func edge(_ y: Int, _ down: Bool) -> Double { let a = mean(max(0, y - 6), y), b = mean(y, min(g.h, y + 6)); return down ? b - a : a - b }
        let span = Int(Double(g.h) * 0.12)
        let top = (max(1, best.1 - span)..<min(g.h - 1, best.1 + span)).max(by: { edge($0, true) < edge($1, true) }) ?? best.1
        let bottom = (max(1, best.2 - span)..<min(g.h - 1, best.2 + span)).max(by: { edge($0, false) < edge($1, false) }) ?? best.2
        let W = Double(g.w)
        func hard(_ y: Int) -> Bool { (y - 1...y + 1).contains { coverage(g, (2, Double($0)), (W - 3, Double($0)), step: 0.04) >= 0.6 } }
        guard hard(top), hard(bottom) else { return nil }
        return (Double(top) / Double(g.h), Double(bottom) / Double(g.h))
    }

    /// The split in one grey frame as comparable numbers, for checking it holds still.
    private static func splitKey(_ g: LGrey, skip: [LRect] = []) -> (String, [Double]) {
        let content = panes(g, skip: skip).panes.filter { isPicture(g, $0) }
        if content.count > 1 { return ("panes", content.flatMap { [$0.0, $0.1, $0.2, $0.3] }.sorted()) }
        if let d = diagonal(g) { return ("diagonal", [d.0, d.1 * Double(g.h)]) }
        return ("none", [])
    }

    /// Whether the frame's split holds still in the next frames while the picture changes. Without next frames, or
    /// when the picture doesn't change, the one frame decides.
    private static func splitHolds(_ g: LGrey, _ next: [LGrey], skip: [LRect] = []) -> Bool {
        let k = splitKey(g, skip: skip)
        guard k.0 != "none" else { return true }
        for n in next {
            let change = zip(g.v, n.v).reduce(Float(0)) { $0 + abs($1.0 - $1.1) } / Float(max(g.v.count, 1))
            if change < 0.01 { continue }
            let m = splitKey(n, skip: skip)
            guard m.0 == k.0, m.1.count == k.1.count, zip(m.1, k.1).allSatisfy({ abs($0 - $1) <= 1.5 }) else { return false }
        }
        return true
    }

    /// Text lines into blocks: stacked close together, overlapping across, of a similar size.
    private static func textBlocks(_ lines: [(LRect, String)]) -> [[(LRect, String)]] {
        var parent = Array(lines.indices)
        func find(_ i: Int) -> Int { var i = i; while parent[i] != i { i = parent[i] }; return i }
        for i in lines.indices {
            for j in lines.indices where j > i {
                let (a, b) = (lines[i].0, lines[j].0)
                let gap = max(a.y, b.y) - min(a.y1, b.y1)
                let across = max(0, min(a.x1, b.x1) - max(a.x, b.x)) / max(min(a.w, b.w), 1e-6)
                let size = max(a.h, b.h) / max(min(a.h, b.h), 1e-6)
                if gap < 0.8 * min(a.h, b.h) && across > 0.3 && size < 1.6 { parent[find(j)] = find(i) }
            }
        }
        return Dictionary(grouping: lines.indices, by: find).values.map { $0.map { lines[$0] } }
    }

    /// How a frame is laid out. `next`: frames a moment later in the same shot (for splits; may be empty).
    /// What the layout is read from: Vision's answers about the frame and small grey copies of it and the next frames.
    /// Split from `layout(from:)` so raea's Rust port can be checked against the same inputs (ugc-perceive layout-inputs).
    struct LayoutInputs {
        var W: Double, H: Double
        var screenshot: Bool
        var faces: [LRect]
        var lines: [(LRect, String)]
        var rects: [LRect]
        var mask: [Float]?, mw: Int, mh: Int
        var g: LGrey
        var next: [LGrey]
    }

    static func layout(_ image: CGImage, next: [CGImage] = []) -> Layout { layout(from: layoutInputs(image, next: next)) }

    static func layoutInputs(_ image: CGImage, next: [CGImage] = []) -> LayoutInputs {
        let (W, H) = (Double(image.width), Double(image.height))

        let handler = VNImageRequestHandler(cgImage: image, options: [:])
        let faceRequest = VNDetectFaceRectanglesRequest()
        let textRequest = VNRecognizeTextRequest()
        textRequest.recognitionLevel = .fast
        textRequest.usesLanguageCorrection = false
        let personRequest = VNGeneratePersonSegmentationRequest()
        personRequest.qualityLevel = .balanced
        let rectRequest = VNDetectRectanglesRequest()
        rectRequest.maximumObservations = 8
        rectRequest.minimumSize = 0.08
        rectRequest.minimumAspectRatio = 0.2
        rectRequest.quadratureTolerance = 20   // screens shown at an angle are quads, not rectangles
        let classify = VNClassifyImageRequest()
        #if targetEnvironment(simulator)
        let requests: [VNRequest] = [faceRequest, textRequest, personRequest, rectRequest]   // labels come out wrong on the Simulator's CPU
        #else
        let requests: [VNRequest] = [faceRequest, textRequest, personRequest, rectRequest, classify]
        #endif
        for r in requests { onCPUInSimulator(r); try? handler.perform([r]) }
        let screenshot = (classify.results ?? []).contains { $0.identifier == "screenshot" && $0.confidence >= 0.3 }
        let faces = (faceRequest.results ?? []).map { LRect($0.boundingBox) }.filter { $0.area > 0.0015 }
        // printed on things in the scene (tilted or in perspective) isn't laid over the frame
        let lines: [(LRect, String)] = (textRequest.results ?? []).compactMap { o in
            guard let s = o.topCandidates(1).first, s.confidence > 0.3 else { return nil }
            let tilt = abs(atan2(Double(o.topRight.y - o.topLeft.y) * H, Double(o.topRight.x - o.topLeft.x) * W)) * 180 / .pi
            return tilt < 2.5 ? (LRect(o.boundingBox), s.string) : nil
        }
        let rects = (rectRequest.results ?? []).map { LRect($0.boundingBox) }
        var mask: [Float]? = nil, mw = 0, mh = 0
        if let px = personRequest.results?.first?.pixelBuffer {
            CVPixelBufferLockBaseAddress(px, .readOnly)
            mw = CVPixelBufferGetWidth(px); mh = CVPixelBufferGetHeight(px)
            let row = CVPixelBufferGetBytesPerRow(px)
            if let base = CVPixelBufferGetBaseAddress(px)?.assumingMemoryBound(to: UInt8.self) {
                mask = (0..<(mw * mh)).map { Float(base[($0 / mw) * row + $0 % mw]) / 255 }
            }
            CVPixelBufferUnlockBaseAddress(px, .readOnly)
        }
        return LayoutInputs(W: W, H: H, screenshot: screenshot, faces: faces, lines: lines, rects: rects, mask: mask, mw: mw, mh: mh,
                            g: LGrey(image), next: next.map { LGrey($0) })
    }

    /// How a frame is laid out, from what Vision saw in it (layoutInputs).
    static func layout(from i: LayoutInputs) -> Layout {
        let (W, H, screenshot, faces, lines, rects, mask, mw, mh, g) = (i.W, i.H, i.screenshot, i.faces, i.lines, i.rects, i.mask, i.mw, i.mh, i.g)
        let ratio = [("9:16", 9.0 / 16), ("4:5", 0.8), ("1:1", 1.0), ("16:9", 16.0 / 9)].min(by: { abs($0.1 - W / H) < abs($1.1 - W / H) })!.0

        var zones: [LayoutZone] = []
        var split = LayoutSplit(kind: "none", angle: nil)

        // panes and framed video, only from splits that hold still
        // text boxes in grey px, left out of the seam search
        let textPx = lines.map { LRect(x: $0.0.x * Double(g.w), y: $0.0.y * Double(g.h), w: $0.0.w * Double(g.w), h: $0.0.h * Double(g.h)) }
        let still = splitHolds(g, i.next, skip: textPx)
        var framed: LRect? = nil
        var paneZones: [LRect] = []
        if still {
            let (all, first) = panes(g, skip: textPx)
            let content = all.filter { isPicture(g, $0) }
                .map { LRect(x: $0.0 / Double(g.w), y: $0.1 / Double(g.h), w: ($0.2 - $0.0) / Double(g.w), h: ($0.3 - $0.1) / Double(g.h)) }
            // a framed picture: everything around it is flat (bars, a plain or gradient margin)
            let around = all.filter { !isPicture(g, $0) }
            if all.count > 1 && content.count == 1 && around.allSatisfy({ detail(g, $0) <= 0.006 }) {
                framed = content[0]
            } else if content.count > 1 {
                split.kind = content.count == 2 ? (first == "row" ? "stacked" : "side-by-side") : "grid"
                paneZones = content
            }
            if split.kind == "none", framed == nil, let d = diagonal(g) {
                split = LayoutSplit(kind: "diagonal", angle: r3(-d.0))   // rising to the right; image y runs down
            }
        }
        if framed == nil, split.kind == "none", let band = framedBand(g) { framed = LRect(x: 0, y: band.0, w: 1, h: band.1 - band.0) }

        // screens and slides: dense small text, or a big rectangle holding several lines; the window, not one card in it
        let small = lines.filter { $0.0.h < 0.022 }
        var screen: LRect? = nil
        if small.count >= 6 {
            let box = small.map(\.0).reduce(small[0].0) { $0.union($1) }
            let holder = rects.filter { r in Double(small.filter { r.contains($0.0) }.count) >= 0.6 * Double(small.count) }.max(by: { $0.area < $1.area })
            screen = (holder ?? LRect(x: box.x - 0.03, y: box.y - 0.03, w: box.w + 0.06, h: box.h + 0.06)).clamped()
        } else if let slide = rects.filter({ r in r.area > 0.2 && r.area < 0.97 && lines.filter { r.contains($0.0) }.count >= 4 }).max(by: { $0.area < $1.area }) {
            screen = slide
        } else if screenshot && paneZones.isEmpty {
            // a recording whose text is too small to read: the biggest rectangle (a window, a tilted laptop, a phone), else the picture
            screen = rects.filter { $0.area > 0.12 }.max(by: { $0.area < $1.area }) ?? framed ?? LRect(x: 0, y: 0, w: 1, h: 1)
        }
        if let sc = screen {
            zones.append(sc.zone("video", label: "screen"))
            // a slide's or a window's own frames aren't split panes
            if paneZones.filter({ sc.inter($0) > 0.5 * $0.area }).count >= paneZones.count - 1 { paneZones = []; split = LayoutSplit(kind: "none", angle: nil) }
        }
        for p in paneZones { zones.append(p.zone("video", label: "pane")) }
        if let f = framed, !(screen.map { $0.inter(f) > 0.7 * min($0.area, f.area) } ?? false) { zones.append(f.zone("video", label: "framed video")) }

        // people: each face, and the person's box from the mask around it
        var people: [LRect] = []
        for f in faces {
            var body = LRect(x: f.x - 1.2 * f.w, y: f.y - 0.6 * f.h, w: 3.4 * f.w, h: 4.6 * f.h).clamped()
            if let m = mask {
                var (x0, y0, x1, y1) = (1.0, 1.0, 0.0, 0.0)
                for i in 0..<(mw * mh) where m[i] > 0.5 {
                    let x = (Double(i % mw) + 0.5) / Double(mw), y = (Double(i / mw) + 0.5) / Double(mh)
                    if x > body.x && x < body.x1 && y > body.y && y < body.y1 { x0 = min(x0, x); y0 = min(y0, y); x1 = max(x1, x); y1 = max(y1, y) }
                }
                if x1 > x0 { body = LRect(x: x0, y: y0, w: x1 - x0, h: y1 - y0).union(f).clamped() }
            }
            // a presenter's bubble sits at the edge of a screen or framed picture, or outside it; a face well inside is content
            let (fx, fy) = (f.x + f.w / 2, f.y + f.h / 2)
            let holder = screen ?? (framed.map { $0.area < 0.95 ? $0 : nil } ?? nil)
            let onScreen = holder.map { r in fx > r.x + 0.15 * r.w && fx < r.x1 - 0.15 * r.w && fy > r.y + 0.15 * r.h && fy < r.y1 - 0.15 * r.h } ?? false
            // an inset of the creator over other footage (the green-screen corner, a picture-in-picture box): small, head and
            // shoulders only (a small person in a wide shot is about seven faces tall), cut by the frame's edge or edged by seams
            let headAndShoulders = body.h < 3.6 * f.h && body.area < 0.15
            let cut = body.y1 > 0.97 || body.x < 0.03 || body.x1 > 0.97
            let gx = Double(g.w), gy = Double(g.h)
            let seams = [((body.x * gx, body.y * gy), (body.x1 * gx, body.y * gy)), ((body.x * gx, body.y * gy), (body.x * gx, body.y1 * gy)),
                         ((body.x1 * gx, body.y * gy), (body.x1 * gx, body.y1 * gy))].filter { coverage(g, $0.0, $0.1) >= 0.7 }.count
            let inset = headAndShoulders && !onScreen && (cut && (fx < 0.35 || fx > 0.65) || seams >= 2)
            let bubble = (body.area < 0.12 && holder != nil && !onScreen) || inset
            zones.append(body.zone("subject", face: f, label: bubble ? "face bubble" : onScreen ? "on screen" : nil))
            people.append(body)
        }
        // people the face finder missed (turned away, cut by the frame): parts of the person mask with no face in them
        if let m = mask {
            let gw = 48, gh = max(1, Int(48.0 * H / W))
            var on = [Bool](repeating: false, count: gw * gh), seen = on
            for i in 0..<(mw * mh) where m[i] > 0.5 { on[min(gh - 1, (i / mw) * gh / mh) * gw + min(gw - 1, (i % mw) * gw / mw)] = true }
            for start in 0..<(gw * gh) where on[start] && !seen[start] {
                var stack = [start], cells: [Int] = []
                seen[start] = true
                while let c = stack.popLast() {
                    cells.append(c)
                    let (x, y) = (c % gw, c / gw)
                    for (nx, ny) in [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)] where nx >= 0 && ny >= 0 && nx < gw && ny < gh {
                        let k = ny * gw + nx
                        if on[k] && !seen[k] { seen[k] = true; stack.append(k) }
                    }
                }
                guard Double(cells.count) / Double(gw * gh) > 0.01 else { continue }
                let xs = cells.map { $0 % gw }, ys = cells.map { $0 / gw }
                let r = LRect(x: Double(xs.min()!) / Double(gw), y: Double(ys.min()!) / Double(gh),
                              w: Double(xs.max()! - xs.min()! + 1) / Double(gw), h: Double(ys.max()! - ys.min()! + 1) / Double(gh))
                if people.contains(where: { $0.inter(r) > 0.3 * r.area }) { continue }
                zones.append(r.zone("subject", label: "person"))
            }
        }

        // captions and titles: text blocks outside the screen
        for b in textBlocks(lines.filter { l in !(screen?.contains(l.0) ?? false) }) {
            let box = b.map(\.0).reduce(b[0].0) { $0.union($1) }
            guard box.h > 0.012 else { continue }
            let words = b.reduce(0) { $0 + $1.1.split(separator: " ").count }
            let text = b.sorted { $0.0.y < $1.0.y }.map(\.1).joined(separator: " ")
            zones.append(box.zone("text", words: words, label: String(text.prefix(40))))
        }
        return Layout(ratio: ratio, split: split, zones: zones)
    }

    // MARK: framing coach (docs/previz.feature › Lighting check from the live camera; Previz is boxes): the live camera's
    // face, where the eyes sit, and the light on the face, a few times a second. Apple Vision's face box and person mask
    // only (no model ships for it). Only what Post can't fix becomes a warning.

    struct Coach: Codable, Equatable, Sendable {
        /// The frame's width over its height, to place the boxes on a preview that fills its view.
        var aspect: Double
        /// The biggest face, 0...1 from the top left of the frame.
        var face: LayoutBox?
        /// Where the eyes sit, 0...1 from the top.
        var eyeLine: Double?
        /// even front light | soft front light | light to one side | overhead | window behind (backlit) | mixed |
        /// low light | overexposed
        var light: String?
        /// One line for the creator, only for what Post can't fix.
        var warning: String?
        /// The camera's horizontal field of view in degrees at its zoom, and what that is in words: ultra wide (over 90°),
        /// wide (60-90°, the selfie and main lenses), normal (40-60°, about 2×), tele (under 40°).
        var fieldOfView: Double?
        var lens: String?
        var faceLevel: Double?
        var backlitStops: Double?
        var keyFillStops: Double?
        var mixed: Double?
    }

    /// The eye line a talking head is framed to by default: the upper third, with room for words underneath.
    static let eyeBand: ClosedRange<Double> = 0.28...0.42

    /// `pixels`: an upright BGRA frame, mirrored like the preview. `iso`: the camera's ISO at that moment (0 if unknown).
    /// What the coach reads from Vision: the biggest face and the person mask. Split from `coach(from:)` so raea's
    /// Rust port can be checked against the same inputs (ugc-perceive coach-inputs).
    struct CoachInputs {
        var W: Int, H: Int
        var face: LRect?
        /// The person mask's bytes, row by row (empty when there's none).
        var mask: [UInt8], mw: Int, mh: Int
    }

    static func coachInputs(_ pixels: CVPixelBuffer) -> CoachInputs {
        let (W, H) = (CVPixelBufferGetWidth(pixels), CVPixelBufferGetHeight(pixels))
        let handler = VNImageRequestHandler(cvPixelBuffer: pixels, options: [:])
        let faces = VNDetectFaceRectanglesRequest()
        let person = VNGeneratePersonSegmentationRequest()
        person.qualityLevel = .fast
        for r in [faces, person] as [VNRequest] { onCPUInSimulator(r); try? handler.perform([r]) }
        var out = CoachInputs(W: W, H: H, face: nil, mask: [], mw: 0, mh: 0)
        guard let biggest = (faces.results ?? []).max(by: { $0.boundingBox.height < $1.boundingBox.height }) else { return out }
        out.face = LRect(biggest.boundingBox)
        if let m = person.results?.first?.pixelBuffer {
            CVPixelBufferLockBaseAddress(m, .readOnly)
            let (mw, mh, row) = (CVPixelBufferGetWidth(m), CVPixelBufferGetHeight(m), CVPixelBufferGetBytesPerRow(m))
            if let base = CVPixelBufferGetBaseAddress(m)?.assumingMemoryBound(to: UInt8.self) {
                out.mask = (0..<mh).flatMap { y in (0..<mw).map { base[y * row + $0] } }
                (out.mw, out.mh) = (mw, mh)
            }
            CVPixelBufferUnlockBaseAddress(m, .readOnly)
        }
        return out
    }

    static func coach(_ pixels: CVPixelBuffer, iso: Float = 0, fieldOfView: Float = 0) -> Coach {
        let inputs = coachInputs(pixels)
        CVPixelBufferLockBaseAddress(pixels, .readOnly)
        defer { CVPixelBufferUnlockBaseAddress(pixels, .readOnly) }
        let row = CVPixelBufferGetBytesPerRow(pixels)
        let base = CVPixelBufferGetBaseAddress(pixels)?.assumingMemoryBound(to: UInt8.self)
        return coach(from: inputs, iso: iso, fieldOfView: fieldOfView, pixel: base.map { b in { x, y in let p = b + y * row + x * 4; return (p[2], p[1], p[0]) } })
    }

    /// The coach from what Vision saw; `pixel(x, y)` reads the upright frame's (r, g, b) bytes, nil when it can't be read.
    static func coach(from i: CoachInputs, iso: Float = 0, fieldOfView: Float = 0, pixel: ((Int, Int) -> (UInt8, UInt8, UInt8))?) -> Coach {
        let (W, H) = (i.W, i.H)
        var out = Coach(aspect: Double(W) / Double(max(H, 1)))
        if fieldOfView > 0 {
            let fov = Double(fieldOfView)
            out.fieldOfView = r3(fov)
            out.lens = fov > 90 ? "ultra wide" : fov > 60 ? "wide" : fov > 40 ? "normal" : "tele"
        }
        guard let f = i.face else { return out }
        out.face = LayoutBox(x: r3(f.x), y: r3(f.y), w: r3(f.w), h: r3(f.h))
        out.eyeLine = r3(f.y + 0.45 * f.h)

        // the person mask, read where it's asked for
        var maskAt: (Double, Double) -> Bool = { _, _ in true }
        if !i.mask.isEmpty {
            let (data, mw, mh) = (i.mask, i.mw, i.mh)
            maskAt = { x, y in data[min(mh - 1, Int(y * Double(mh))) * mw + min(mw - 1, Int(x * Double(mw)))] > 127 }
        }

        // linear light at a point (sRGB in)
        guard let pixel else { return out }
        func lin(_ v: UInt8) -> Double { let c = Double(v) / 255; return c <= 0.04045 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4) }
        func rgb(_ x: Double, _ y: Double) -> (Double, Double, Double) {
            let p = pixel(min(W - 1, max(0, Int(x * Double(W)))), min(H - 1, max(0, Int(y * Double(H)))))
            return (lin(p.0), lin(p.1), lin(p.2))
        }
        func luma(_ c: (Double, Double, Double)) -> Double { 0.2126 * c.0 + 0.7152 * c.1 + 0.0722 * c.2 }
        func warmth(_ c: (Double, Double, Double)) -> Double { (c.0 - c.2) / max(c.0 + c.1 + c.2, 1e-4) }

        // the skin of the face: the middle of the face box, inside the person
        var left: [(Double, Double, Double)] = [], right: [(Double, Double, Double)] = []
        var upper: [Double] = [], lower: [Double] = []
        var clipped = 0
        let steps = 24
        for j in 0..<steps {
            for i in 0..<steps {
                let x = f.x + f.w * (0.2 + 0.6 * (Double(i) + 0.5) / Double(steps))
                let y = f.y + f.h * (0.25 + 0.6 * (Double(j) + 0.5) / Double(steps))
                guard x > 0, x < 1, y > 0, y < 1, maskAt(x, y) else { continue }
                let c = rgb(x, y)
                if min(c.0, c.1, c.2) > 0.95 { clipped += 1 }
                if i < steps / 2 { left.append(c) } else { right.append(c) }
                if j < steps / 2 { upper.append(luma(c)) } else { lower.append(luma(c)) }
            }
        }
        let skin = left + right
        guard skin.count > 40 else { return out }
        func mean(_ v: [(Double, Double, Double)]) -> (Double, Double, Double) {
            let n = Double(max(v.count, 1))
            return (v.reduce(0) { $0 + $1.0 } / n, v.reduce(0) { $0 + $1.1 } / n, v.reduce(0) { $0 + $1.2 } / n)
        }
        func stops(_ a: Double, _ b: Double) -> Double { log2(max(a, 1e-4) / max(b, 1e-4)) }
        let face = luma(mean(skin))
        let clipShare = Double(clipped) / Double(skin.count)
        let keyFill = abs(stops(luma(mean(left)), luma(mean(right))))
        let topDown = stops(upper.reduce(0, +) / Double(max(upper.count, 1)), lower.reduce(0, +) / Double(max(lower.count, 1)))
        let mixed = abs(warmth(mean(left)) - warmth(mean(right)))

        // the background just around the person
        var around: [Double] = []
        for j in 0..<steps {
            for i in 0..<steps {
                let x = f.x - 1.5 * f.w + 4 * f.w * (Double(i) + 0.5) / Double(steps)
                let y = f.y - 0.8 * f.h + 3 * f.h * (Double(j) + 0.5) / Double(steps)
                guard x > 0, x < 1, y > 0, y < 1, !maskAt(x, y) else { continue }
                around.append(luma(rgb(x, y)))
            }
        }
        let backlit: Double? = around.count > 30 ? stops(around.reduce(0, +) / Double(around.count), face) : nil

        // first match wins (the thresholds were checked on Kelly's finished ads and on made-bad copies, T-401)
        let light: String
        if clipShare > 0.05 { light = "overexposed" }
        else if face < 0.03 || iso >= 1600 { light = "low light" }
        else if let b = backlit, b > 1.0 { light = "window behind (backlit)" }
        else if mixed > 0.15 { light = "mixed" }
        else if keyFill > 1.3 { light = "light to one side" }
        else if topDown > 1.0 { light = "overhead" }
        else { light = keyFill < 0.5 ? "even front light" : "soft front light" }
        out.light = light
        out.warning = [
            "overexposed": "Your face is blown out: turn a little away from the light, or soften it",
            "low light": "Too dark to clean up later: add light or move closer to a window",
            "window behind (backlit)": "Backlit: turn around to face the window",
            "mixed": "Two colours of light on your face: turn off the lamp or the overhead light",
        ][light]
        // a face close to a wide lens bulges (nose big, ears gone), and no edit can undo it: step back or zoom in
        // (the frame here is upright, so its width is the narrow side; the lens's angle is across the long side)
        if out.warning == nil, let fov = out.fieldOfView, fov > 60, f.w > 0.55 {
            out.warning = "Too close for this lens: step back, or zoom in to about 2×"
        }
        out.faceLevel = r3(face)
        out.backlitStops = backlit.map(r3)
        out.keyFillStops = r3(keyFill)
        out.mixed = r3(mixed)
        return out
    }

    // MARK: helpers

    /// The Simulator has no Neural Engine or GPU path for Vision's models: every stage goes to the CPU there.
    static func onCPUInSimulator(_ request: VNRequest) {
        #if targetEnvironment(simulator)
        guard let stages = try? request.supportedComputeStageDevices else { return }
        for (stage, devices) in stages {
            if let cpu = devices.first(where: { if case .cpu = $0 { return true } else { return false } }) { request.setComputeDevice(cpu, for: stage) }
        }
        #endif
    }

    nonisolated(unsafe) static var clock = Date()
    /// With PERCEPTION_TIMES set, how long each stage took, to stderr.
    static func lap(_ what: String) {
        guard ProcessInfo.processInfo.environment["PERCEPTION_TIMES"] != nil else { return }
        FileHandle.standardError.write(String(format: "%6.2fs %@\n", Date().timeIntervalSince(clock), what).data(using: .utf8)!)
    }

    static func r2(_ x: Double) -> Double { (x * 100).rounded() / 100 }

    /// Vision's box (origin bottom-left) as [x, y, w, h] from the top left.
    static func box(_ b: CGRect) -> [Double] { [r2(b.minX), r2(1 - b.maxY), r2(b.width), r2(b.height)] }

    static func commonest(_ labels: [String], top: Int) -> [String] {
        var count: [String: Int] = [:]
        for l in labels { count[l, default: 0] += 1 }
        return count.sorted { $0.value != $1.value ? $0.value > $1.value : $0.key < $1.key }.prefix(top).map(\.key)
    }

    static func meanBrightness(_ image: CGImage) -> Double {
        let (w, h) = (16, 16)
        var px = [UInt8](repeating: 0, count: w * h * 4)
        guard let ctx = CGContext(data: &px, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4,
                                  space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue) else { return 0.5 }
        ctx.draw(image, in: CGRect(x: 0, y: 0, width: w, height: h))
        var sum = 0.0
        for i in stride(from: 0, to: px.count, by: 4) { sum += 0.2126 * Double(px[i]) + 0.7152 * Double(px[i + 1]) + 0.0722 * Double(px[i + 2]) }
        return sum / Double(w * h) / 255
    }

    static func saveJPEG(_ image: CGImage, to url: URL) {
        guard let dest = CGImageDestinationCreateWithURL(url as CFURL, UTType.jpeg.identifier as CFString, 1, nil) else { return }
        CGImageDestinationAddImage(dest, image, [kCGImageDestinationLossyCompressionQuality: 0.8,
                                                 kCGImageDestinationImageMaxPixelSize: 540] as CFDictionary)
        CGImageDestinationFinalize(dest)
    }
}

extension JSONEncoder {
    static var compact: JSONEncoder {
        let e = JSONEncoder()
        e.outputFormatting = [.sortedKeys]
        return e
    }

    static var sorted: JSONEncoder {
        let e = JSONEncoder()
        e.outputFormatting = [.sortedKeys, .prettyPrinted]
        return e
    }
}


// MARK: hands (MediaPipe hand landmarker and gesture recognizer: the palm detector on the letterboxed frame → a turned
// 224×224 crop → 21 points → the gesture embedder and classifier; models/mediapipe-hand-landmarker and
// models/mediapipe-gesture-recognizer, their features' "How the app should call it"). Read in memory, never stored.

extension Perception {
    struct HandModels { var detector: MLModel; var landmarks: MLModel; var embedder: MLModel; var classifier: MLModel }

    static func handModels() throws -> HandModels {
        HandModels(detector: try model("HandDetector", folder: "mediapipe-hand-landmarker"),
                   landmarks: try model("HandLandmarks", folder: "mediapipe-hand-landmarker"),
                   embedder: try model("GestureEmbedder", folder: "mediapipe-gesture-recognizer"),
                   classifier: try model("GestureClassifier", folder: "mediapipe-gesture-recognizer"))
    }

    /// The classifier's 8 outputs, in order.
    static let gestureNames = ["None", "Closed_Fist", "Open_Palm", "Pointing_Up", "Thumb_Down", "Thumb_Up", "Victory", "ILoveYou"]

    /// A hand read: 21 points [x, y, z] in the frame's pixels and its gesture (nil under 0.5, or None).
    struct HandRead { var points: [[Double]]; var gesture: String?; var crop: [Double] = [] }

    /// A square of the frame, turned `rotation` radians clockwise about (cx, cy) in pixels (y down), as `size`×`size`:
    /// crop pixel (u, v) is centre + R(rotation)·((u, v) − size/2)·side/size. The face mesh's crop, for any size.
    static func turnedCrop(_ picture: CIImage, cx: Double, cy: Double, side: Double, rotation: Double, size: Int, context: CIContext) throws -> CVPixelBuffer {
        let extent = picture.extent, k = side / Double(size)
        let toCrop = CGAffineTransform(translationX: -extent.minX, y: -extent.minY)
            .concatenating(CGAffineTransform(a: 1, b: 0, c: 0, d: -1, tx: 0, ty: extent.height))
            .concatenating(CGAffineTransform(translationX: -cx, y: -cy))
            .concatenating(CGAffineTransform(rotationAngle: -rotation))
            .concatenating(CGAffineTransform(scaleX: 1 / k, y: 1 / k))
            .concatenating(CGAffineTransform(translationX: CGFloat(size) / 2, y: CGFloat(size) / 2))
            .concatenating(CGAffineTransform(a: 1, b: 0, c: 0, d: -1, tx: 0, ty: CGFloat(size)))
        let px = try square(size)
        context.render(picture.transformed(by: toCrop)
            .composited(over: CIImage(color: .black).cropped(to: CGRect(x: 0, y: 0, width: size, height: size))), to: px)
        return px
    }

    /// Up to `limit` hands in the frame, with their gestures.
    static func hands(_ picture: CIImage, models: HandModels, context: CIContext, limit: Int = 2) throws -> [HandRead] {
        let side = 192, extent = picture.extent
        let scale = min(Double(side) / extent.width, Double(side) / extent.height)
        let (cw, ch) = (Int((extent.width * scale).rounded()), Int((extent.height * scale).rounded()))
        let (ox, oy) = ((side - cw) / 2, (side - ch) / 2)
        let px = try square(side)
        let placed = picture.transformed(by: CGAffineTransform(translationX: -extent.minX, y: -extent.minY))
            .transformed(by: CGAffineTransform(scaleX: scale, y: scale))
            .transformed(by: CGAffineTransform(translationX: CGFloat(ox), y: CGFloat(side - oy - ch)))
            .composited(over: CIImage(color: .black).cropped(to: CGRect(x: 0, y: 0, width: side, height: side)))
        context.render(placed, to: px)
        let out = try models.detector.prediction(from: MLDictionaryFeatureProvider(dictionary: ["image": MLFeatureValue(pixelBuffer: px)]))
        guard let s = out.featureValue(for: "scores")?.multiArrayValue, let b = out.featureValue(for: "boxes")?.multiArrayValue,
              let k = out.featureValue(for: "keypoints")?.multiArrayValue else { throw Failure.unreadable("the palm detector gave nothing") }
        let (scores, boxes, keys) = (floats(s), floats(b), floats(k))
        // Palms over 0.5, best first, without the ones overlapping a better one by IoU 0.3 or more.
        var kept: [Int] = []
        for i in scores.indices.filter({ scores[$0] >= 0.5 }).sorted(by: { scores[$0] > scores[$1] }) where kept.count < limit {
            func iou(_ a: Int, _ c: Int) -> Float {
                let (p, q) = (a * 4, c * 4)
                let w = max(0, min(boxes[p] + boxes[p + 2], boxes[q] + boxes[q + 2]) - max(boxes[p], boxes[q]))
                let h = max(0, min(boxes[p + 1] + boxes[p + 3], boxes[q + 1] + boxes[q + 3]) - max(boxes[p + 1], boxes[q + 1]))
                let union = boxes[p + 2] * boxes[p + 3] + boxes[q + 2] * boxes[q + 3] - w * h
                return union > 0 ? w * h / union : 0
            }
            if kept.allSatisfy({ iou(i, $0) < 0.3 }) { kept.append(i) }
        }
        func x(_ v: Float) -> Double { (Double(v) * Double(side) - Double(ox)) / scale }
        func y(_ v: Float) -> Double { (Double(v) * Double(side) - Double(oy)) / scale }
        var reads: [HandRead] = []
        for i in kept {
            let (left, top) = (x(boxes[i * 4]), y(boxes[i * 4 + 1]))
            let (w, h) = (Double(boxes[i * 4 + 2]) * Double(side) / scale, Double(boxes[i * 4 + 3]) * Double(side) / scale)
            // Keypoint 0 is the wrist, 2 the middle finger's knuckle; the hand points up the crop.
            let (x0, y0, x2, y2) = (x(keys[i * 14]), y(keys[i * 14 + 1]), x(keys[i * 14 + 4]), y(keys[i * 14 + 5]))
            let rot = Double.pi / 2 - atan2(-(y2 - y0), x2 - x0)
            let (c, sn) = (cos(rot), sin(rot))
            let (cx, cy) = (left + w / 2 + 0.5 * h * sn, top + h / 2 - 0.5 * h * c)
            let sideCrop = 2.6 * max(w, h)
            let crop = try turnedCrop(picture, cx: cx, cy: cy, side: sideCrop, rotation: rot, size: 224, context: context)
            let l = try models.landmarks.prediction(from: MLDictionaryFeatureProvider(dictionary: ["image": MLFeatureValue(pixelBuffer: crop)]))
            guard let lm = l.featureValue(for: "landmarks")?.multiArrayValue, let presence = l.featureValue(for: "presence")?.multiArrayValue,
                  (floats(presence).first ?? 0) >= 0.5 else { continue }
            let raw = floats(lm), kk = sideCrop / 224
            let points: [[Double]] = (0..<21).map { j in
                let (u, v) = (Double(raw[j * 3]) - 112, Double(raw[j * 3 + 1]) - 112)
                return [cx + (c * u - sn * v) * kk, cy + (sn * u + c * v) * kk, Double(raw[j * 3 + 2]) * kk]
            }
            let hand = try MLMultiArray(shape: [1, 21, 3], dataType: .float32)
            for (j, p) in points.enumerated() { for d in 0..<3 { hand[[0, j, d] as [NSNumber]] = NSNumber(value: p[d]) } }
            let e = try models.embedder.prediction(from: MLDictionaryFeatureProvider(dictionary: ["hand": MLFeatureValue(multiArray: hand)]))
            var gesture: String?
            if let emb = e.featureValue(for: "embedding")?.multiArrayValue {
                let g = try models.classifier.prediction(from: MLDictionaryFeatureProvider(dictionary: ["embedding": MLFeatureValue(multiArray: emb)]))
                if let sc = g.featureValue(for: "scores")?.multiArrayValue {
                    let v = floats(sc)
                    if let best = v.indices.max(by: { v[$0] < v[$1] }), v[best] >= 0.5, best > 0 { gesture = gestureNames[best] }
                }
            }
            reads.append(HandRead(points: points, gesture: gesture, crop: [cx, cy, sideCrop, rot * 180 / .pi]))
        }
        return reads
    }
}

// MARK: performance (T-332; docs/perception-measures.feature › The creator's performance, › Face data stays on the device)
//
// The creator's performance second by second, from the face mesh above and the clip's sound: eye contact, blinks,
// energy, speaking with the lip-sync offset, and reading. Only these derived numbers leave `performance`: the 478
// points and 52 expression scores each frame are read, turned into a few angles and levels, and dropped.

extension Perception {
    struct Performance: Codable, Equatable {
        struct Second: Codable, Equatable {
            /// The second's start, from the clip's start.
            var t: Int
            /// Share of the second's frames with a face (the numbers below are over those frames).
            var face: Double
            /// Where the eyes are, from the second's median gaze: "lens" (into the camera), "near" (the screen just beside
            /// the lens) or "away" (off camera, reading, at the product or someone else). Toward the camera is lens or near.
            var eye_contact: String?
            /// Share of the second's face frames inside the lens cone (the advisory number under "lens").
            var lens_share: Double?
            /// Eyes closed at some moment of the second (both eyeBlink scores over 0.5).
            var blink: Bool?
            /// How much the face moves, against the clip's own median second (1 = typical, 2 = twice as lively).
            var energy: Double?
            /// Share of the second with a voice and a moving mouth.
            var speaking: Double?
            /// The eyes sweeping side to side off the lens, as when reading a script off camera.
            var reading: Bool?
            /// A hand gesture held in the second (thumbs up, pointing, open palm, holding something up), when the hand
            /// models are in the build.
            var gesture: String?
        }
        var model = "mediapipe-face-landmarker-v2"
        /// Frames read a second.
        var fps: Double
        /// The sound against the mouth, in ms: positive when the sound comes late. Nil without enough speech to tell.
        var lip_sync_ms: Double?
        /// How clearly the mouth and the voice line up (the correlation's peak, 0-1); under about 0.3 the offset is a guess.
        var lip_sync_confidence: Double?
        var seconds: [Second]
    }

    /// One frame's read of the face, already reduced: never the points or the expression scores themselves.
    struct Look {
        var t: Double
        /// Where the eyes point, in degrees from the lens: yaw positive to the subject's left, pitch positive up.
        var yaw: Double
        var pitch: Double
        /// The eyes alone, against the head (from the eyeLook scores, -1…1): positive to the subject's left.
        var eyes: Double
        var blink: Bool
        /// The lips' gap (inner upper lip 13 to lower lip 14) over the eyes' width (outer corners 33 to 263): about 0
        /// closed, 0.3+ wide open. jawOpen alone missed speech made with little jaw movement.
        var mouth: Double
        /// How far the expression moved since the last frame (sum of the 52 scores' changes, blinks left out).
        var motion: Double
        /// Tuning parts: the head's turn and tilt, the eyeLook up score, the irises in the eye openings, and the nose
        /// tip against the eyes (sideways, over the eyes' width; down, over eyes-to-chin).
        var headYaw = 0.0, headPitch = 0.0, up = 0.0, irisX = 0.0, irisY = 0.0, noseX = 0.0, noseY = 0.0
    }

    /// How far the eyeLook scores turn the eyes (degrees at a score of 1).
    static let eyeYawDegrees = 25.0, eyePitchDegrees = 20.0
    /// Eye contact in three levels, from the second's median gaze around the centre where the lens reads: inside the wide
    /// cone is toward the camera, inside the tight one the lens itself. Fitted to Gemini's three-level labels on 20 raw
    /// takes, a take held out at a time (knowledge/drive/bin/perf_fit3.py, tickets/T-332/proof/fit3.json): toward against
    /// away agrees on 77.0% of seconds; lens against near 73.9% (advisory: a few degrees apart).
    static let gazeYawOffset = -6.0, gazePitchOffset = -6.0
    static let towardYawCone = 16.0, towardPitchCone = 20.0
    static let lensYawCone = 13.0, lensPitchCone = 13.0
    /// Reading's own cone (fitted with it: 98.7% of seconds agree, held out; perf_tune.py): mostly outside it.
    static let readingYawOffset = -6.0, readingPitchOffset = -6.0, readingYawCone = 13.0, readingPitchCone = 13.0
    /// The jaw opens a little before the sound starts, even in sync: this much is taken off the measured lag.
    static let lipSyncBiasMs = 0.0
    /// For tuning the thresholds against teacher labels (ugc-perceive performance --looks): each frame's reduced read
    /// and the sound's levels, before they're summarized. Never the points or the expression scores.
    nonisolated(unsafe) static var tuning: (([Look], [Float]) -> Void)?
    /// What `performance` read, when a tool asks (ugc-perceive performance --inputs): every Look, the frame times, the
    /// loudness, the gestures, and the first frames' meshes with their Looks: the reference for raea's Rust port.
    nonisolated(unsafe) static var performanceInputsLog: [String: Any]? = nil

    static func lookFields(_ l: Look) -> [String: Any] {
        ["t": l.t, "yaw": l.yaw, "pitch": l.pitch, "eyes": l.eyes, "blink": l.blink, "mouth": l.mouth, "motion": l.motion,
         "headYaw": l.headYaw, "headPitch": l.headPitch, "up": l.up, "irisX": l.irisX, "irisY": l.irisY, "noseX": l.noseX, "noseY": l.noseY]
    }

    private static func index(_ name: String) -> Int { blendshapeNames.firstIndex(of: name) ?? 0 }

    /// One frame's face, reduced to a Look. `last` is the previous frame's 52 scores, for the motion.
    static func look(_ mesh: FaceMesh, t: Double, last: [Float]?) -> Look {
        let p = mesh.points, s = mesh.blendshapes
        func v(_ name: String) -> Double { Double(s[index(name)]) }
        // The head's turn from the mesh's depth: the cheeks (234 image-left, 454 image-right) and the forehead (10)
        // against the chin (152). z is in x's pixel units, smaller = nearer.
        let yawHead = atan2(p[454][2] - p[234][2], p[454][0] - p[234][0]) * 180 / .pi
        let pitchHead = atan2(p[152][2] - p[10][2], p[152][1] - p[10][1]) * 180 / .pi
        // The eyes against the head: looking to the subject's left turns the left eye out and the right eye in.
        let eyes = (v("eyeLookOutLeft") + v("eyeLookInRight") - v("eyeLookInLeft") - v("eyeLookOutRight")) / 2
        let up = (v("eyeLookUpLeft") + v("eyeLookUpRight") - v("eyeLookDownLeft") - v("eyeLookDownRight")) / 2
        let blinkScores = Set(["eyeBlinkLeft", "eyeBlinkRight"].map(index))
        let motion = last.map { l in s.indices.reduce(0.0) { blinkScores.contains($1) ? $0 : $0 + abs(Double(s[$1] - l[$1])) } } ?? 0
        func gap(_ a: Int, _ b: Int) -> Double { hypot(p[a][0] - p[b][0], p[a][1] - p[b][1]) }
        let eyesWidth = max(1, gap(33, 263))
        func along(_ iris: Int, from a: Int, to b: Int) -> Double {
            let (dx, dy) = (p[b][0] - p[a][0], p[b][1] - p[a][1])
            return ((p[iris][0] - p[a][0]) * dx + (p[iris][1] - p[a][1]) * dy) / max(1, dx * dx + dy * dy) - 0.5
        }
        func between(_ iris: Int, upper: Int, lower: Int) -> Double { (p[iris][1] - p[upper][1]) / max(0.5, p[lower][1] - p[upper][1]) - 0.5 }
        var l = Look(t: t, yaw: yawHead + eyes * eyeYawDegrees, pitch: -pitchHead + up * eyePitchDegrees, eyes: eyes,
                     blink: v("eyeBlinkLeft") > 0.5 && v("eyeBlinkRight") > 0.5, mouth: gap(13, 14) / eyesWidth, motion: motion)
        let (ex, ey) = ((p[33][0] + p[133][0] + p[362][0] + p[263][0]) / 4, (p[33][1] + p[133][1] + p[362][1] + p[263][1]) / 4)
        (l.headYaw, l.headPitch, l.up) = (yawHead, -pitchHead, up)
        (l.irisX, l.irisY) = ((along(468, from: 33, to: 133) + along(473, from: 362, to: 263)) / 2,
                              (between(468, upper: 159, lower: 145) + between(473, upper: 386, lower: 374)) / 2)
        (l.noseX, l.noseY) = ((p[1][0] - ex) / eyesWidth, (p[1][1] - ey) / max(1, p[152][1] - ey))
        return l
    }

    /// The clip's performance: `fps` frames a second through the face mesh, the sound's loudness every 10 ms.
    static func performance(_ url: URL, fps wanted: Double = 15, models given: FaceModels? = nil) async throws -> Performance {
        let models = try given ?? faceModels()
        // Hands when their models are in the build: gestures, and their movement counts toward energy.
        let handModels = try? handModels()
        let asset = AVURLAsset(url: url)
        guard let video = try await asset.loadTracks(withMediaType: .video).first else { throw Failure.unreadable("\(url.lastPathComponent) has no picture") }
        let transform = try await video.load(.preferredTransform)
        let nominal = try await video.load(.nominalFrameRate)
        let duration = try await asset.load(.duration)
        let fps = min(wanted, nominal > 0 ? Double(nominal) : 30)
        let reader = try AVAssetReader(asset: asset)
        let out = AVAssetReaderTrackOutput(track: video, outputSettings: [kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA])
        out.alwaysCopiesSampleData = false
        reader.add(out)
        guard reader.startReading() else { throw Failure.unreadable(reader.error?.localizedDescription ?? "can't decode the video") }
        let context = modelContext(), orient = orientation(transform)
        var looks: [Look] = [], frameTimes: [Double] = [], crop: FaceCrop?, last: [Float]?, frames = 0, start: Double?
        var gestures: [(t: Double, gesture: String)] = [], lastHands: [HandRead] = []
        while let buffer = out.copyNextSampleBuffer() {
            let t = CMSampleBufferGetPresentationTimeStamp(buffer).seconds
            let first = start ?? t
            guard t + 1e-6 >= first + Double(frames) / fps, let px = CMSampleBufferGetImageBuffer(buffer) else { continue }
            start = first
            frames += 1
            try autoreleasepool {
                // A smaller picture is plenty for a face that fills the frame, and keeps the crop's sampling cheap.
                var picture = CIImage(cvPixelBuffer: px).oriented(orient)
                let long = max(picture.extent.width, picture.extent.height)
                if long > 1280 { picture = picture.transformed(by: CGAffineTransform(scaleX: 1280 / long, y: 1280 / long)) }
                var found = try crop.flatMap { try faceMesh(picture, crop: $0, models: models, context: context) }
                if found == nil, let c = try detectFace(picture, models: models, context: context, largest: true) {
                    found = try faceMesh(picture, crop: c, models: models, context: context)
                }
                crop = found.map { Perception.crop(around: $0.points) }
                frameTimes.append(t - first)
                // Hands every third frame: the gesture held, and how far the hands moved (against their own size).
                var handMotion = 0.0
                if let handModels, frames % 3 == 1 {
                    let read = (try? hands(picture, models: handModels, context: context)) ?? []
                    for h in read { if let g = h.gesture { gestures.append((t - first, g)) } }
                    for (a, b) in zip(lastHands, read) {
                        let size = max(1, hypot(b.points[9][0] - b.points[0][0], b.points[9][1] - b.points[0][1]))
                        handMotion += zip(a.points, b.points).reduce(0.0) { $0 + hypot($1.0[0] - $1.1[0], $1.0[1] - $1.1[1]) } / 21 / size / 3
                    }
                    lastHands = read
                }
                if let mesh = found {
                    var l = look(mesh, t: t - first, last: last)
                    if performanceInputsLog != nil, looks.count < 40 {
                        var meshes = performanceInputsLog?["meshes"] as? [[String: Any]] ?? []
                        meshes.append(["points": mesh.points, "blendshapes": mesh.blendshapes.map(Double.init), "last": last.map { $0.map(Double.init) } as Any,
                                       "t": t - first, "look": lookFields(l)])
                        performanceInputsLog?["meshes"] = meshes
                    }
                    l.motion += 2 * handMotion
                    looks.append(l)
                    last = mesh.blendshapes
                } else {
                    last = nil
                }
            }
        }
        let loudness = (try? await voiceLevels(asset)) ?? []
        tuning?(looks, loudness)
        if performanceInputsLog != nil {
            performanceInputsLog?["looks"] = looks.map(lookFields)
            performanceInputsLog?["frameTimes"] = frameTimes
            performanceInputsLog?["loudness"] = loudness.map(Double.init)
            performanceInputsLog?["fps"] = fps
            performanceInputsLog?["duration"] = duration.seconds
            performanceInputsLog?["gestures"] = gestures.map { ["t": $0.t, "gesture": $0.gesture] }
            performanceInputsLog?["hands"] = handModels != nil
        }
        var p = summarize(looks, frameTimes: frameTimes, fps: fps, duration: duration.seconds, loudness: loudness)
        // A gesture counts in a second when it's read in at least two of that second's hand reads.
        for i in p.seconds.indices {
            let held = gestures.filter { Int($0.t) == p.seconds[i].t }.map(\.gesture)
            if let top = Dictionary(grouping: held, by: { $0 }).max(by: { $0.value.count < $1.value.count }), top.value.count >= 2 {
                p.seconds[i].gesture = top.key
            }
        }
        if handModels != nil { p.model += "+mediapipe-hands" }
        return p
    }

    /// The sound's level every 10 ms (RMS of 16 kHz mono), from the clip's start; empty without a sound track.
    static func voiceLevels(_ asset: AVURLAsset) async throws -> [Float] {
        let all = try await pcm16k(asset)
        return stride(from: 0, to: all.count, by: 160).map { i in
            let w = all[i..<min(all.count, i + 160)]
            return sqrt(w.reduce(0) { $0 + $1 * $1 } / Float(max(1, w.count)))
        }
    }

    /// The sound as 16 kHz mono floats from the clip's start; empty without a sound track.
    static func pcm16k(_ asset: AVURLAsset) async throws -> [Float] {
        guard let audio = try await asset.loadTracks(withMediaType: .audio).first else { return [] }
        let reader = try AVAssetReader(asset: asset)
        let out = AVAssetReaderTrackOutput(track: audio, outputSettings: [
            AVFormatIDKey: kAudioFormatLinearPCM, AVSampleRateKey: 16_000, AVNumberOfChannelsKey: 1,
            AVLinearPCMBitDepthKey: 32, AVLinearPCMIsFloatKey: true, AVLinearPCMIsNonInterleaved: false, AVLinearPCMIsBigEndianKey: false,
        ])
        reader.add(out)
        guard reader.startReading() else { return [] }
        var samples: [Float] = [], firstAt: Double?
        while let buffer = out.copyNextSampleBuffer() {
            if firstAt == nil { firstAt = CMSampleBufferGetPresentationTimeStamp(buffer).seconds }
            guard let block = CMSampleBufferGetDataBuffer(buffer) else { continue }
            let n = CMBlockBufferGetDataLength(block) / 4
            var chunk = [Float](repeating: 0, count: n)
            chunk.withUnsafeMutableBytes { _ = CMBlockBufferCopyDataBytes(block, atOffset: 0, dataLength: n * 4, destination: $0.baseAddress!) }
            samples += chunk
        }
        // Sound that starts late in the file starts late here too.
        let lead = [Float](repeating: 0, count: max(0, Int(((firstAt ?? 0) * 16_000).rounded())))
        return lead + samples
    }

    /// Seconds from the frames' Looks and the sound. Pure: the tests drive it with made-up Looks.
    static func summarize(_ looks: [Look], frameTimes: [Double], fps: Double, duration: Double, loudness: [Float]) -> Performance {
        let secondsCount = max(1, Int(ceil(duration - 1e-6)))
        // Voice: louder than the quiet part of the clip by 10 dB, and not silence.
        let sortedLevels = loudness.sorted()
        let floor = sortedLevels.isEmpty ? 0 : sortedLevels[sortedLevels.count / 5]
        let voiced = loudness.map { $0 > max(floor * 3.16, 0.003) }
        func voiceAt(_ t: Double) -> Bool {
            let i = Int((t * 100).rounded())
            return (max(0, i - 5)...max(0, i + 5)).contains { $0 < voiced.count && voiced[$0] }
        }
        // The mouth moving: the jaw's change against the frame before, over a small threshold.
        var mouthMoving: [Double: Bool] = [:]
        for (a, b) in zip(looks, looks.dropFirst()) where b.t - a.t < 2.5 / fps { mouthMoving[b.t] = abs(b.mouth - a.mouth) > 0.015 }
        let perSecondMotion: [Double] = (0..<secondsCount).map { s in
            let m = looks.filter { Int($0.t) == s && $0.motion > 0 }.map(\.motion)
            return m.isEmpty ? 0 : m.reduce(0, +) / Double(m.count)
        }
        let typical = median(perSecondMotion.filter { $0 > 0 })
        var seconds: [Performance.Second] = []
        for s in 0..<secondsCount {
            let inSecond = looks.filter { Int($0.t) == s }
            let framesInSecond = frameTimes.filter { Int($0) == s }.count
            var second = Performance.Second(t: s, face: framesInSecond == 0 ? 0 : r2(Double(inSecond.count) / Double(framesInSecond)))
            if !inSecond.isEmpty {
                let (yaws, pitches) = (inSecond.map(\.yaw).sorted(), inSecond.map(\.pitch).sorted())
                let (dy, dp) = (abs(yaws[yaws.count / 2] - gazeYawOffset), abs(pitches[pitches.count / 2] - gazePitchOffset))
                second.eye_contact = dy <= lensYawCone && dp <= lensPitchCone ? "lens"
                    : dy <= towardYawCone && dp <= towardPitchCone ? "near" : "away"
                let inLens = inSecond.filter { abs($0.yaw - gazeYawOffset) <= lensYawCone && abs($0.pitch - gazePitchOffset) <= lensPitchCone }
                second.lens_share = r2(Double(inLens.count) / Double(inSecond.count))
                let atCamera = inSecond.filter { abs($0.yaw - readingYawOffset) <= readingYawCone && abs($0.pitch - readingPitchOffset) <= readingPitchCone }
                second.blink = inSecond.contains(where: \.blink)
                if typical > 0, perSecondMotion[s] > 0 { second.energy = r2(perSecondMotion[s] / typical) }
                if !loudness.isEmpty {
                    let talking = inSecond.filter { voiceAt($0.t) && (mouthMoving[$0.t] ?? false) }.count
                    second.speaking = r2(Double(talking) / Double(inSecond.count))
                }
                // Reading: mostly off the lens, the eyes sweeping across, turning back at least once.
                let eyes = inSecond.map(\.eyes)
                let turns = zip(zip(eyes, eyes.dropFirst()), eyes.dropFirst(2)).filter { pair, c in
                    let (a, b) = pair
                    return (b - a) * (c - b) < 0 && abs(b - a) > 0.03
                }.count
                let range = (eyes.max() ?? 0) - (eyes.min() ?? 0)
                // Fitted with eye contact (98.7% of seconds agree with Gemini, held out): rarely at the lens, the eyes
                // ranging widely and turning back three or more times.
                second.reading = Double(atCamera.count) / Double(inSecond.count) < 0.2 && range > 0.5 && turns >= 3
            }
            seconds.append(second)
        }
        let (lag, confidence) = lipSync(looks, loudness: loudness, fps: fps)
        return Performance(fps: fps, lip_sync_ms: lag.map { ($0 - lipSyncBiasMs).rounded() }, lip_sync_confidence: confidence.map(r2),
                           seconds: seconds)
    }

    /// The lag (ms) at which the sound's loudness best follows the jaw's opening, from -500 to +500 ms, with the
    /// correlation there. Both series are taken to 100 a second and their slow trends removed.
    static func lipSync(_ looks: [Look], loudness: [Float], fps: Double) -> (Double?, Double?) {
        guard looks.count >= Int(fps * 3), loudness.count >= 300 else { return (nil, nil) }
        let end = min(Double(loudness.count) / 100, (looks.last?.t ?? 0))
        let n = Int(end * 100)
        guard n > 300 else { return (nil, nil) }
        // The jaw at 100 a second, by linear interpolation between frames (gaps over 0.3 s left as 0).
        var jaw = [Double](repeating: .nan, count: n)
        for (a, b) in zip(looks, looks.dropFirst()) where b.t - a.t < 0.3 {
            let (i0, i1) = (Int((a.t * 100).rounded()), min(n - 1, Int((b.t * 100).rounded())))
            guard i0 < n, i1 > i0 else { continue }
            for i in i0...i1 { jaw[i] = a.mouth + (b.mouth - a.mouth) * Double(i - i0) / Double(i1 - i0) }
        }
        let level = (0..<n).map { log(Double(loudness[$0]) + 1e-4) }
        func detrend(_ x: [Double]) -> [Double] {
            // Minus a 1-second moving average, so only the syllable-scale wiggles are compared.
            var out = x
            for i in x.indices {
                let w = x[max(0, i - 50)..<min(x.count, i + 51)].filter { !$0.isNaN }
                out[i] = x[i].isNaN || w.isEmpty ? .nan : x[i] - w.reduce(0, +) / Double(w.count)
            }
            return out
        }
        let (j, l) = (detrend(jaw), detrend(level))
        var best: (lag: Int, r: Double)?
        for lag in -50...50 {
            var sxy = 0.0, sxx = 0.0, syy = 0.0, count = 0
            for i in 0..<n {
                let k = i + lag
                guard k >= 0, k < n, !j[i].isNaN, !l[k].isNaN else { continue }
                sxy += j[i] * l[k]; sxx += j[i] * j[i]; syy += l[k] * l[k]; count += 1
            }
            guard count > 200, sxx > 0, syy > 0 else { continue }
            let r = sxy / sqrt(sxx * syy)
            if best == nil || r > best!.r { best = (lag, r) }
        }
        guard let best, best.r > 0 else { return (nil, nil) }
        return (Double(best.lag * 10), best.r)
    }

    static func median(_ x: [Double]) -> Double {
        let s = x.sorted()
        return s.isEmpty ? 0 : s[s.count / 2]
    }
}

// MARK: product candidates (T-333; docs/perception-measures.feature › Finding the product without a product detector)
//
// No model knows every product, so the objects in a clip's keyframes are cut out (Vision's foreground instances, the
// person left out) and described by what can be checked against the business: the crop itself (for SigLIP 2 against
// the product shots, the logo and the brief's words), the words printed on it, and whether a hand holds it. Scoring and
// tracking come after (the product finder); this is the cutting out.

extension Perception {
    struct Candidate: Codable, Equatable {
        /// The keyframe's time, s.
        var t: Double
        /// The object's box, as shares of the frame: x and y from the top-left.
        var box: [Double]
        /// Share of the frame the object covers (its mask, not its box).
        var area: Double
        /// Words Vision reads on it (packaging, a label), most confident first.
        var text: [String]
        /// A hand's points fall inside its box (MediaPipe hands, when in the build).
        var held: Bool
        /// The crop beside the candidates' file (a little room around the box), for embedding.
        var crop: String
    }

    /// The hook and five more moments spread over the clip, as measure takes keyframes.
    static func keyframeTimes(_ duration: Double) -> [Double] {
        var times: [Double] = [min(0.5, duration / 2)]
        for i in 1..<6 { times.append(duration * Double(i) / 6) }
        return times
    }

    /// Cuts the objects out of a clip's keyframes into `dir` (crops and candidates.json). The person is left out:
    /// instances that cover the person segmentation's mask by half or more.
    static func productCandidates(_ url: URL, into dir: URL) async throws -> [Candidate] {
        let asset = AVURLAsset(url: url)
        let duration = try await asset.load(.duration).seconds
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let generator = AVAssetImageGenerator(asset: asset)
        generator.appliesPreferredTrackTransform = true
        generator.maximumSize = CGSize(width: 960, height: 960)
        generator.requestedTimeToleranceBefore = CMTime(seconds: 0.05, preferredTimescale: 600)
        generator.requestedTimeToleranceAfter = CMTime(seconds: 0.05, preferredTimescale: 600)
        let handModels = try? handModels()
        let objectModels = try? objectModels()
        let context = modelContext()
        var out: [Candidate] = []
        for (k, t) in keyframeTimes(duration).enumerated() {
            guard let frame = try? await generator.image(at: CMTime(seconds: t, preferredTimescale: 600)).image else { continue }
            let (w, h) = (Double(frame.width), Double(frame.height))
            let objects = VNGenerateForegroundInstanceMaskRequest()
            let person = VNGeneratePersonSegmentationRequest()
            person.qualityLevel = .balanced
            person.outputPixelFormat = kCVPixelFormatType_OneComponent8
            onCPUInSimulator(person)
            let handler = VNImageRequestHandler(cgImage: frame, options: [:])
            try? handler.perform([objects, person])
            let personMask = person.results?.first?.pixelBuffer
            let hands = handModels.flatMap { try? Perception.hands(CIImage(cgImage: frame), models: $0, context: context) } ?? []
            // Vision's objects; when it finds none apart from the person (it often cuts out the person and what they
            // hold as one), EdgeTAM clicked on a grid.
            var boxes: [(CGRect, Double, Double)] = []
            if let found = objects.results?.first {
                for instance in found.allInstances {
                    guard let mask = try? found.generateScaledMaskForImage(forInstances: IndexSet(integer: instance), from: handler),
                          let b = maskBox(mask, person: personMask) else { continue }
                    boxes.append(b)
                }
            }
            // The grid always: a held product is usually inside the person's mask, and Vision often cuts it out with them.
            if let objectModels {
                // A held product sits between the thumb and the fingers: clicks between the thumb and index tips, and at
                // the palm's centre, for each hand.
                var holds: [(Double, Double)] = []
                for hand in hands {
                    let q = hand.points
                    let grip: (Double, Double) = ((q[4][0] + q[8][0]) / 2 / Double(w), (q[4][1] + q[8][1]) / 2 / Double(h))
                    let palm: (Double, Double) = ((q[0][0] + q[9][0]) / 2 / Double(w), (q[0][1] + q[9][1]) / 2 / Double(h))
                    for c in [grip, palm] where c.0 > 0 && c.0 < 1 && c.1 > 0 && c.1 < 1 { holds.append(c) }
                }
                boxes += (try? gridObjects(CIImage(cgImage: frame), width: Int(w), height: Int(h), models: objectModels,
                                           person: personMask, extra: holds, context: context)) ?? []
            }
            // The creator is the cut-out holding a face's centre; a product in their hands overlaps their mask and stays.
            let faces = VNDetectFaceRectanglesRequest()
            try? handler.perform([faces])
            let faceCentres = (faces.results ?? []).map { CGPoint(x: $0.boundingBox.midX * w, y: (1 - $0.boundingBox.midY) * h) }
            for (instance, (box, covered, _)) in boxes.enumerated() {
                if covered <= 0.002 || covered > 0.6 { continue }  // specks, the background
                if faceCentres.contains(where: { box.contains($0) }) && covered > 0.08 { continue }  // the creator
                let (bx, by, bw, bh) = (box.minX / w, box.minY / h, box.width / w, box.height / h)
                // A hand counts when three or more of its points fall inside the box.
                let held = hands.contains { hand in hand.points.filter { box.contains(CGPoint(x: $0[0], y: $0[1])) }.count >= 3 }
                let pad = 0.1 * max(box.width, box.height)
                let room = box.insetBy(dx: -pad, dy: -pad).intersection(CGRect(x: 0, y: 0, width: w, height: h)).integral
                guard let cut = frame.cropping(to: room) else { continue }
                let name = String(format: "cand-%d-%02d.jpg", k, instance)
                saveJPEG(cut, to: dir.appendingPathComponent(name))
                out.append(Candidate(t: r2(t), box: [bx, by, bw, bh].map { ($0 * 1000).rounded() / 1000 }, area: (covered * 1000).rounded() / 1000,
                                     text: words(cut).map(\.0).prefix(6).map { String($0) }, held: held, crop: name))
            }
        }
        let enc = JSONEncoder()
        enc.outputFormatting = [.prettyPrinted, .sortedKeys]
        try enc.encode(out).write(to: dir.appendingPathComponent("candidates.json"))
        return out
    }

    /// EdgeTAM clicked on a 4×6 grid of one frame (the encoder once, the decoder per click): each object's box, the
    /// share of the frame it covers and the share of it on the person, with repeats (IoU 0.6+) dropped.
    static func gridObjects(_ picture: CIImage, width: Int, height: Int, models: ObjectModels, person: CVPixelBuffer?,
                            extra: [(Double, Double)] = [], context: CIContext) throws -> [(CGRect, Double, Double)] {
        let side = Double(ObjectTracker.side)
        let frame = try models.encoder.prediction(from: MLDictionaryFeatureProvider(dictionary: [
            "image": MLFeatureValue(pixelBuffer: try stretched(picture, side: ObjectTracker.side, context: context))]))
        guard let vf = frame.featureValue(for: "vision_features"), let h0 = frame.featureValue(for: "high_res_feat_0"),
              let h1 = frame.featureValue(for: "high_res_feat_1") else { throw Failure.unreadable("EdgeTAM's encoder gave nothing") }
        var out: [(CGRect, Double, Double)] = []
        // The grid's clicks, then any extra ones (shares of the frame): where a hand holds something.
        var clicks: [(Double, Double)] = []
        for row in 0..<6 { for col in 0..<4 { clicks.append(((Double(col) + 0.5) / 4, (Double(row) + 0.5) / 6)) } }
        clicks += extra
        for (nx, ny) in clicks {
            do {
                let (px, py) = (nx * side, ny * side)
                let coords = try MLMultiArray(shape: [1, 1, 2], dataType: .float32)
                coords[0] = NSNumber(value: Float(px)); coords[1] = NSNumber(value: Float(py))
                let labels = try MLMultiArray(shape: [1, 1], dataType: .float32)
                labels[0] = 1
                let one = try MLMultiArray(shape: [1], dataType: .float32)
                one[0] = 1
                let decoded = try models.decoder.prediction(from: MLDictionaryFeatureProvider(dictionary: [
                    "pix_feat": vf, "high_res_feat_0": h0, "high_res_feat_1": h1,
                    "point_coords": MLFeatureValue(multiArray: coords), "point_labels": MLFeatureValue(multiArray: labels),
                    "add_no_mem": MLFeatureValue(multiArray: one), "multimask": MLFeatureValue(multiArray: one)]))
                guard let l = decoded.featureValue(for: "mask_logits")?.multiArrayValue,
                      let score = decoded.featureValue(for: "object_score_logit")?.multiArrayValue, (floats(score).first ?? 0) > 0 else { continue }
                // The 256×256 logits over the stretched frame: the object's cells, back in the frame's pixels.
                let logit = floats(l), n = 256
                var (x0, y0, x1, y1, count, onPerson) = (n, n, -1, -1, 0, 0)
                var pbase: UnsafeMutableRawPointer?, prow = 0, (pw, ph) = (0, 0)
                if let person { CVPixelBufferLockBaseAddress(person, .readOnly); pbase = CVPixelBufferGetBaseAddress(person)
                    prow = CVPixelBufferGetBytesPerRow(person); (pw, ph) = (CVPixelBufferGetWidth(person), CVPixelBufferGetHeight(person)) }
                for y in 0..<n { for x in 0..<n where logit[y * n + x] > 0 {
                    count += 1
                    (x0, y0, x1, y1) = (min(x0, x), min(y0, y), max(x1, x), max(y1, y))
                    if let pbase, pw > 0, ph > 0, pbase.load(fromByteOffset: (y * ph / n) * prow + x * pw / n, as: UInt8.self) > 127 { onPerson += 1 }
                } }
                if let person { CVPixelBufferUnlockBaseAddress(person, .readOnly) }
                guard count > 0 else { continue }
                let (sx, sy) = (Double(width) / Double(n), Double(height) / Double(n))
                let box = CGRect(x: Double(x0) * sx, y: Double(y0) * sy, width: Double(x1 - x0 + 1) * sx, height: Double(y1 - y0 + 1) * sy)
                let iou = { (a: CGRect, b: CGRect) -> Double in
                    let i = a.intersection(b); return i.isNull ? 0 : Double(i.width * i.height) / Double(a.width * a.height + b.width * b.height - i.width * i.height) }
                if out.contains(where: { iou($0.0, box) >= 0.6 }) { continue }
                out.append((box, Double(count) / Double(n * n), Double(onPerson) / Double(count)))
            }
        }
        return out
    }

    /// An instance mask's box in the frame's pixels (y from the top), the share of the frame it covers, and the share
    /// of it on the person's mask.
    static func maskBox(_ mask: CVPixelBuffer, person: CVPixelBuffer?) -> (CGRect, Double, Double)? {
        CVPixelBufferLockBaseAddress(mask, .readOnly)
        defer { CVPixelBufferUnlockBaseAddress(mask, .readOnly) }
        let (mw, mh) = (CVPixelBufferGetWidth(mask), CVPixelBufferGetHeight(mask))
        guard let base = CVPixelBufferGetBaseAddress(mask) else { return nil }
        let row = CVPixelBufferGetBytesPerRow(mask)
        let isFloat = CVPixelBufferGetPixelFormatType(mask) == kCVPixelFormatType_OneComponent32Float
        var personBase: UnsafeMutableRawPointer?, personRow = 0, (pw, ph) = (0, 0)
        if let person {
            CVPixelBufferLockBaseAddress(person, .readOnly)
            personBase = CVPixelBufferGetBaseAddress(person)
            personRow = CVPixelBufferGetBytesPerRow(person)
            (pw, ph) = (CVPixelBufferGetWidth(person), CVPixelBufferGetHeight(person))
        }
        defer { if let person { CVPixelBufferUnlockBaseAddress(person, .readOnly) } }
        var (x0, y0, x1, y1, count, onPerson) = (mw, mh, -1, -1, 0, 0)
        for y in stride(from: 0, to: mh, by: 2) {
            for x in stride(from: 0, to: mw, by: 2) {
                let v: Float = isFloat ? base.load(fromByteOffset: y * row + x * 4, as: Float.self) : Float(base.load(fromByteOffset: y * row + x, as: UInt8.self)) / 255
                guard v > 0.5 else { continue }
                count += 1
                (x0, y0, x1, y1) = (min(x0, x), min(y0, y), max(x1, x), max(y1, y))
                if let personBase, pw > 0, ph > 0 {
                    let (px, py) = (x * pw / mw, y * ph / mh)
                    if personBase.load(fromByteOffset: py * personRow + px, as: UInt8.self) > 127 { onPerson += 1 }
                }
            }
        }
        guard count > 0 else { return nil }
        let covered = Double(count * 4) / Double(mw * mh)
        return (CGRect(x: x0, y: y0, width: x1 - x0 + 2, height: y1 - y0 + 2), covered, Double(onPerson) / Double(count))
    }
}

// MARK: camera and space (T-334; docs/perception-measures.feature › Camera and space; models/depth-anything-3-small,
// the rule for DepthAnything3SmallCamera.mlpackage: 4 frames of one shot → each frame's camera and lens)

extension Perception {
    struct ShotCamera: Codable, Equatable {
        /// still | handheld | pan | tilt | push-in | pull-out | moving
        var move: String
        /// The jitter left after the window's smooth turn is taken out, degrees.
        var shake: Double
        /// zoom (the lens changed) | dolly (the camera moved along its view) | nil
        var zoom_or_dolly: String?
        /// The lens as a 35 mm-equivalent focal length, and wide (under 20 mm), standard or tele (over 40 mm).
        var focal_mm: Double
        var lens: String
        /// Which camera: "front" (a selfie) when the biggest face covers 8% of the frame or more, else "back". Focal
        /// length can't tell them apart (both about 26 mm); fitted to Gemini's labels on 46 library clips, 91.3% held out
        /// (knowledge/drive/bin/side_teacher.py). New takes also record it in take.json.
        var side: String?
        /// The person's depth against the background's (background ÷ subject): about 1 flat, 1.5+ layered. Nil without a person.
        var separation: Double?
        /// Turn over the shot, degrees a second, about the vertical (pan) and horizontal (tilt) axes.
        var pan_dps: Double
        var tilt_dps: Double
        /// The camera centre's move: push is its forward share (1 straight in, −1 straight out), travel how far it went
        /// against the scene (0 still, 1 a lot), each averaged over the shot's windows.
        var push: Double = 0
        var travel: Double = 0
        /// The place: Apple's image feature print of the shot's first frame with the person blacked out (768 numbers,
        /// rounded); shots in the same room are near each other (cosine), so a location change can be counted.
        var room: [Float]?
    }

    static func cameraModel() throws -> MLModel { try model("DepthAnything3SmallCamera", folder: "depth-anything-3-small", units: .cpuAndGPU) }

    /// A frame letterboxed into 504×504 like inverseDepth, and the letterbox's scale.
    static func depthSquare(_ picture: CIImage, context: CIContext) throws -> (CVPixelBuffer, CGFloat) {
        let side = depthSide, extent = picture.extent
        let scale = min(CGFloat(side) / extent.width, CGFloat(side) / extent.height)
        let (cw, ch) = (max(1, Int((extent.width * scale).rounded())), max(1, Int((extent.height * scale).rounded())))
        let (ox, oy) = ((side - cw) / 2, (side - ch) / 2)
        var px: CVPixelBuffer?
        CVPixelBufferCreate(nil, side, side, kCVPixelFormatType_32BGRA, [kCVPixelBufferCGImageCompatibilityKey: true] as CFDictionary, &px)
        guard let px else { throw Failure.unreadable("no memory for a camera frame") }
        let placed = picture.transformed(by: CGAffineTransform(translationX: -extent.minX, y: -extent.minY))
            .transformed(by: CGAffineTransform(scaleX: scale, y: scale))
            .transformed(by: CGAffineTransform(translationX: CGFloat(ox), y: CGFloat(oy)))
            .composited(over: CIImage(color: .black).cropped(to: CGRect(x: 0, y: 0, width: side, height: side)))
        context.render(placed, to: px)
        return (px, scale)
    }

    /// One window: each frame's rotation (3×3, row-major), camera centre and fx, and frame0's depth with its person mask.
    struct CameraWindow { var rotations: [[Double]]; var centres: [[Double]]; var fx: [Double]; var separation: Double? }

    static func cameraWindow(_ frames: [CGImage], model: MLModel, context: CIContext) throws -> CameraWindow {
        var inputs: [String: MLFeatureValue] = [:]
        for (i, f) in frames.prefix(4).enumerated() { inputs["frame\(i)"] = MLFeatureValue(pixelBuffer: try depthSquare(CIImage(cgImage: f), context: context).0) }
        let out = try model.prediction(from: MLDictionaryFeatureProvider(dictionary: inputs))
        guard let e = out.featureValue(for: "extrinsics")?.multiArrayValue, let k = out.featureValue(for: "intrinsics")?.multiArrayValue
        else { throw Failure.unreadable("the camera model gave nothing") }
        let ex = floats(e), ki = floats(k)
        var rotations: [[Double]] = [], centres: [[Double]] = [], fx: [Double] = []
        for v in 0..<4 {
            let m = (0..<12).map { Double(ex[v * 12 + $0]) }  // [R t] row-major, 3×4
            let r: [Double] = [m[0], m[1], m[2], m[4], m[5], m[6], m[8], m[9], m[10]]
            let t: [Double] = [m[3], m[7], m[11]]
            // C = −Rᵀ t
            var c: [Double] = [0, 0, 0]
            for j in 0..<3 {
                let a: Double = r[j] * t[0]
                let b: Double = r[3 + j] * t[1]
                let d: Double = r[6 + j] * t[2]
                c[j] = -(a + b + d)
            }
            centres.append(c)
            rotations.append(r)
            fx.append(Double(ki[v * 9]))
        }
        // Separation from frame0: the depth under the person against the rest (bigger depth = farther).
        var separation: Double?
        if let depth = out.featureValue(for: "depth")?.multiArrayValue {
            let person = VNGeneratePersonSegmentationRequest()
            person.qualityLevel = .balanced
            person.outputPixelFormat = kCVPixelFormatType_OneComponent8
            onCPUInSimulator(person)
            let (square, _) = try depthSquare(CIImage(cgImage: frames[0]), context: context)
            try? VNImageRequestHandler(cvPixelBuffer: square, options: [:]).perform([person])
            if let mask = person.results?.first?.pixelBuffer {
                CVPixelBufferLockBaseAddress(mask, .readOnly)
                defer { CVPixelBufferUnlockBaseAddress(mask, .readOnly) }
                let (mw, mh, mrow) = (CVPixelBufferGetWidth(mask), CVPixelBufferGetHeight(mask), CVPixelBufferGetBytesPerRow(mask))
                let row = depth.strides.count >= 2 ? depth.strides[depth.strides.count - 2].intValue : depthSide
                var fg: [Float] = [], bg: [Float] = []
                if let base = CVPixelBufferGetBaseAddress(mask) {
                    depth.withUnsafeBufferPointer(ofType: Float.self) { d in
                        for y in stride(from: 0, to: depthSide, by: 4) {
                            for x in stride(from: 0, to: depthSide, by: 4) {
                                let v = d[y * row + x]
                                guard v > 1e-6 else { continue }
                                let m = base.load(fromByteOffset: (y * mh / depthSide) * mrow + x * mw / depthSide, as: UInt8.self)
                                if m > 127 { fg.append(v) } else { bg.append(v) }
                            }
                        }
                    }
                }
                if fg.count > 50, bg.count > 50 {
                    let med = { (a: [Float]) -> Double in let s = a.sorted(); return Double(s[s.count / 2]) }
                    // The ratio of the two medians, the larger over the smaller: on a selfie take with the creator well in
                    // front, this package's depth read the person's region as the bigger (2026-10-12), so the direction
                    // isn't relied on; about 1 is flat, 1.5+ layered.
                    let (a, b) = (med(bg), med(fg))
                    separation = r2(max(a, b) / max(1e-6, min(a, b)))
                }
            }
        }
        return CameraWindow(rotations: rotations, centres: centres, fx: fx, separation: separation)
    }

    /// R_a · R_bᵀ for row-major 3×3s.
    static func relative(_ a: [Double], _ b: [Double]) -> [Double] {
        var out = [Double](repeating: 0, count: 9)
        for i in 0..<3 {
            for j in 0..<3 {
                var v = 0.0
                for k in 0..<3 { v += a[i * 3 + k] * b[j * 3 + k] }
                out[i * 3 + j] = v
            }
        }
        return out
    }

    static func angle(_ r: [Double]) -> Double { acos(max(-1, min(1, (r[0] + r[4] + r[8] - 1) / 2))) * 180 / .pi }


    /// "front" when the biggest face covers 8% of the frame or more (a selfie), else "back".
    static func side(of frame: CGImage) -> String {
        let faces = VNDetectFaceRectanglesRequest()
        onCPUInSimulator(faces)
        try? VNImageRequestHandler(cgImage: frame, options: [:]).perform([faces])
        let biggest = (faces.results ?? []).map { Double($0.boundingBox.width * $0.boundingBox.height) }.max() ?? 0
        return biggest >= 0.08 ? "front" : "back"
    }

    /// Every shot's windows and first-frame facts as `camera` read them, when a tool asks (ugc-perceive camera-inputs).
    nonisolated(unsafe) static var cameraInputsLog: [[String: Any]]? = nil

    /// Where `ugc-perceive camera --room-frames <dir>` wants each shot's person-masked frame (room-0.jpg, room-1.jpg…).
    nonisolated(unsafe) static var roomFrames: URL?
    nonisolated(unsafe) static var roomFrameCount = 0

    /// The shot's place as a vector: the frame's feature print with the person masked out.
    static func roomVector(_ frame: CGImage, context: CIContext) -> [Float]? {
        var picture = CIImage(cgImage: frame)
        let person = VNGeneratePersonSegmentationRequest()
        person.qualityLevel = .balanced
        person.outputPixelFormat = kCVPixelFormatType_OneComponent8
        onCPUInSimulator(person)
        try? VNImageRequestHandler(cgImage: frame, options: [:]).perform([person])
        if let mask = person.results?.first?.pixelBuffer {
            let m = CIImage(cvPixelBuffer: mask)
            let scaled = m.transformed(by: CGAffineTransform(scaleX: picture.extent.width / m.extent.width, y: picture.extent.height / m.extent.height))
            picture = CIImage(color: .black).cropped(to: picture.extent).applyingFilter("CIBlendWithMask", parameters: [
                kCIInputBackgroundImageKey: picture, kCIInputMaskImageKey: scaled])
        }
        guard let masked = context.createCGImage(picture, from: picture.extent) else { return nil }
        // For SigLIP 2 on the Mac (ugc-embed), which tells rooms apart far better than the feature print (room_test.py:
        // 36/36 takes nearest their own folder against 20/36): the masked frame, when a tool asks for it.
        if let dir = roomFrames { saveJPEG(masked, to: dir.appendingPathComponent("room-\(roomFrameCount).jpg")); roomFrameCount += 1 }
        let print = VNGenerateImageFeaturePrintRequest()
        onCPUInSimulator(print)
        try? VNImageRequestHandler(cgImage: masked, options: [:]).perform([print])
        guard let fp = print.results?.first as? VNFeaturePrintObservation, fp.elementType == .float else { return nil }
        var v = [Float](repeating: 0, count: fp.elementCount)
        fp.data.withUnsafeBytes { raw in for i in 0..<fp.elementCount { v[i] = (raw.load(fromByteOffset: i * 4, as: Float.self) * 1000).rounded() / 1000 } }
        return v
    }

    /// Each shot's camera: frames every 0.5 s, in windows of 4 that chain through their shared frame (up to 4 windows).
    static func camera(_ url: URL, shots: [Shot]) async throws -> [ShotCamera?] {
        let model = try cameraModel()
        let asset = AVURLAsset(url: url)
        let generator = AVAssetImageGenerator(asset: asset)
        generator.appliesPreferredTrackTransform = true
        generator.maximumSize = CGSize(width: 720, height: 720)
        generator.requestedTimeToleranceBefore = CMTime(seconds: 0.03, preferredTimescale: 600)
        generator.requestedTimeToleranceAfter = CMTime(seconds: 0.03, preferredTimescale: 600)
        let context = modelContext()
        var out: [ShotCamera?] = []
        for shot in shots {
            let length = shot.end - shot.start
            guard length >= 1.5 else { out.append(nil); continue }
            var times: [Double] = []
            var t = shot.start + 0.05
            while t <= shot.end - 0.05 && times.count < 13 { times.append(t); t += 0.5 }
            var images: [CGImage] = []
            for t in times { if let f = try? await generator.image(at: CMTime(seconds: t, preferredTimescale: 600)).image { images.append(f) } }
            guard images.count >= 4, let first = images.first else { out.append(nil); continue }
            var windows: [CameraWindow] = []
            var i = 0
            while i + 3 < images.count {
                windows.append(try cameraWindow(Array(images[i...(i + 3)]), model: model, context: context))
                i += 3
            }
            let (sd, room) = (side(of: first), roomVector(first, context: context))
            cameraInputsLog?.append(["images": images.count, "width": first.width, "height": first.height, "side": sd, "room": room.map { $0.map(Double.init) } as Any,
                                     "windows": windows.map { ["rotations": $0.rotations, "centres": $0.centres, "fx": $0.fx, "separation": $0.separation as Any] }])
            out.append(cameraSummary(windows, images: images.count, width: first.width, height: first.height, side: sd, room: room))
        }
        return out
    }

    /// A shot's camera from its windows (the model's rotations, centres and fx) and what Vision said of its first frame.
    /// Split from `camera` so raea's Rust port can be checked against the same windows (ugc-perceive camera-inputs).
    static func cameraSummary(_ windows: [CameraWindow], images: Int, width: Int, height: Int, side: String, room: [Float]?) -> ShotCamera {
            var yaw = 0.0, pitch = 0.0, turn = 0.0, jitter: [Double] = [], forward: [Double] = [], fxRatio: [Double] = [], fx0: Double?, sep: Double?
            var travels: [Double] = []
            for w in windows {
                if sep == nil { sep = w.separation }
                if fx0 == nil { fx0 = w.fx[0] }
                let rel = relative(w.rotations[3], w.rotations[0])
                turn += angle(rel)
                // About y (pan) and x (tilt), small-angle, in frame0's axes (x right, y down).
                yaw += atan2(rel[2], rel[8]) * 180 / .pi
                pitch += atan2(-rel[5], rel[8]) * 180 / .pi
                // The middle frames against a steady turn: the jitter.
                for k in 1...2 {
                    let mid = angle(relative(w.rotations[k], w.rotations[0]))
                    jitter.append(abs(mid - angle(rel) * Double(k) / 3))
                }
                // The camera centre's move in frame0's axes: z > 0 is toward the subject.
                let d = (0..<3).map { w.centres[3][$0] - w.centres[0][$0] }
                let r0 = w.rotations[0]
                var local: [Double] = [0, 0, 0]
                for j in 0..<3 { for k in 0..<3 { local[j] += r0[j * 3 + k] * d[k] } }
                let norm = max(1e-9, sqrt(local.reduce(0) { $0 + $1 * $1 }))
                forward.append(local[2] / norm * min(1, norm * 10))
                travels.append(min(1, norm * 10))
                fxRatio.append(w.fx[3] / max(1e-6, w.fx[0]))
            }
            let seconds = Double(images - 1) * 0.5
            let shake = r2(jitter.isEmpty ? 0 : sqrt(jitter.reduce(0) { $0 + $1 * $1 } / Double(jitter.count)))
            let zoom = fxRatio.contains { abs($0 - 1) > 0.04 }
            let push = forward.reduce(0, +) / Double(max(1, forward.count))
            let travel = travels.reduce(0, +) / Double(max(1, travels.count))
            let (pan, tilt) = (yaw / seconds, pitch / seconds)
            let move: String
            // Thresholds fitted to Gemini's labels on 50 library shots (knowledge/drive/bin/camera_fit.py; tickets/T-334/proof):
            // a pan or tilt turns 2°/s or more; a push or pull moves 0.3+ with a forward share of 0.2+; still is under 0.8° of shake
            // (66% held out against the 75% bar: walking shots and slow push-ins are the misses).
            _ = turn
            if abs(pan) >= 2.0 && abs(pan) >= abs(tilt) { move = "pan" }
            else if abs(tilt) >= 2.0 { move = "tilt" }
            else if !zoom && travel >= 0.3 && abs(push) >= 0.2 { move = push > 0 ? "push-in" : "pull-out" }  // fitted: camera_refit.py
            else if shake < 0.8 { move = "still" }
            else { move = "handheld" }
            let scale = min(504 / CGFloat(width), 504 / CGFloat(height))
            let fSrc = (fx0 ?? 400) / Double(scale)
            let mm = 43.27 * fSrc / hypot(Double(width), Double(height))
            return ShotCamera(move: move, shake: shake, zoom_or_dolly: zoom ? "zoom" : abs(push) > 0.5 ? "dolly" : nil,
                              focal_mm: (mm * 10).rounded() / 10, lens: mm < 20 ? "wide" : mm > 40 ? "tele" : "standard",
                              side: side, separation: sep, pan_dps: r2(pan), tilt_dps: r2(tilt), push: r2(push), travel: r2(travel),
                              room: room)
    }
}

// MARK: the product through the clip (T-333; docs/perception-measures.feature › Product numbers each second): EdgeTAM
// follows the finder's box from its keyframe, forward and (on the frames reversed) backward, at 5 frames a second.

extension Perception {
    struct ProductSecond: Codable, Equatable {
        var t: Int
        /// Share of the second's frames with the product in view.
        var on_screen: Double
        /// Its mask's share of the frame, and how far its centre is from the middle (0 middle, 1 a corner), when in view.
        var area: Double?
        var centered: Double?
        /// A hand holds it (MediaPipe hand points inside its box), when the hand models are in the build.
        var held: Bool?
    }

    /// The product followed through the clip from `box` (shares of the frame, x and y from the top-left) at `at` s.
    static func productTrack(_ url: URL, box: [Double], at: Double, fps: Double = 5) async throws -> [ProductSecond] {
        let asset = AVURLAsset(url: url)
        let duration = try await asset.load(.duration).seconds
        let generator = AVAssetImageGenerator(asset: asset)
        generator.appliesPreferredTrackTransform = true
        generator.maximumSize = CGSize(width: 720, height: 720)
        generator.requestedTimeToleranceBefore = CMTime(seconds: 0.04, preferredTimescale: 600)
        generator.requestedTimeToleranceAfter = CMTime(seconds: 0.04, preferredTimescale: 600)
        let models = try objectModels()
        let handModels = try? handModels()
        let context = modelContext()
        let step = 1 / fps
        var forward: [Double] = [], backward: [Double] = []
        var t = at
        while t < duration - 0.02 { forward.append(t); t += step }
        t = at - step
        while t >= 0 { backward.append(t); t -= step }
        var reads: [(t: Double, share: Double, cx: Double, cy: Double, bbox: CGRect, held: Bool?)] = []
        for (times, _) in [(forward, true), (backward, false)] where !times.isEmpty {
            let tracker = ObjectTracker(models: models)
            var first = true
            // Going backward starts again at the prompted frame, so the tracker is shown the product there first.
            let run = times.first == at ? times : [at] + times
            for tt in run {
                guard let frame = try? await generator.image(at: CMTime(seconds: tt, preferredTimescale: 600)).image else { continue }
                let picture = CIImage(cgImage: frame)
                let (w, h) = (64, max(16, Int(64 * Double(frame.height) / Double(frame.width))))
                let prompt = first ? ObjectPrompt(box: box, point: nil, at: at) : nil
                let matte = try tracker.step(picture, prompt: prompt, width: w, height: h, context: context)
                first = false
                if tt == at && times.first != at { continue }  // the backward pass's restart, already read forward
                CVPixelBufferLockBaseAddress(matte, .readOnly)
                defer { CVPixelBufferUnlockBaseAddress(matte, .readOnly) }
                let (mw, mh, row) = (CVPixelBufferGetWidth(matte), CVPixelBufferGetHeight(matte), CVPixelBufferGetBytesPerRow(matte))
                guard let base = CVPixelBufferGetBaseAddress(matte) else { continue }
                var (n, sx, sy, x0, y0, x1, y1) = (0, 0, 0, mw, mh, -1, -1)
                for y in 0..<mh { for x in 0..<mw where base.load(fromByteOffset: y * row + x, as: UInt8.self) > 127 {
                    n += 1; sx += x; sy += y; (x0, y0, x1, y1) = (min(x0, x), min(y0, y), max(x1, x), max(y1, y))
                } }
                let share = Double(n) / Double(mw * mh)
                let (cx, cy) = n > 0 ? (Double(sx) / Double(n) / Double(mw), Double(sy) / Double(n) / Double(mh)) : (0.5, 0.5)
                let bbox = n > 0 ? CGRect(x: Double(x0) / Double(mw) * Double(frame.width), y: Double(y0) / Double(mh) * Double(frame.height),
                                          width: Double(x1 - x0 + 1) / Double(mw) * Double(frame.width), height: Double(y1 - y0 + 1) / Double(mh) * Double(frame.height)) : .null
                var held: Bool?
                if let handModels, n > 0, Int(tt * fps) % Int(fps) == 0 {
                    let hands = (try? Perception.hands(picture, models: handModels, context: context)) ?? []
                    held = hands.contains { hand in hand.points.filter { bbox.contains(CGPoint(x: $0[0], y: $0[1])) }.count >= 3 }
                }
                reads.append((tt, share, cx, cy, bbox, held))
            }
        }
        return (0..<max(1, Int(ceil(duration)))).map { s in
            let here = reads.filter { Int($0.t) == s }
            let seen = here.filter { $0.share > 0.002 }
            var out = ProductSecond(t: s, on_screen: here.isEmpty ? 0 : r2(Double(seen.count) / Double(here.count)))
            if !seen.isEmpty {
                out.area = r2(seen.map(\.share).reduce(0, +) / Double(seen.count))
                out.centered = r2(seen.map { hypot($0.cx - 0.5, $0.cy - 0.5) / hypot(0.5, 0.5) }.reduce(0, +) / Double(seen.count))
                let h = seen.compactMap(\.held)
                if !h.isEmpty { out.held = h.contains(true) }
            }
            return out
        }
    }

    /// Share of the transcript's mentions of the product (any of `words`) said while it's on screen.
    static func namedWhileVisible(_ seconds: [ProductSecond], mentions: [Double]) -> Double? {
        guard !mentions.isEmpty else { return nil }
        let on = Set(seconds.filter { $0.on_screen >= 0.5 }.map(\.t))
        return r2(Double(mentions.filter { on.contains(Int($0)) }.count) / Double(mentions.count))
    }
}
