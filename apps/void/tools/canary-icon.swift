import Foundation
import CoreImage
import AppKit
// tint.swift IN OUT: each PNG of an iconset recoloured canary yellow (luminance kept, alpha kept)
let args = CommandLine.arguments
let inDir = URL(fileURLWithPath: args[1]), outDir = URL(fileURLWithPath: args[2])
try? FileManager.default.createDirectory(at: outDir, withIntermediateDirectories: true)
let ctx = CIContext()
for f in try FileManager.default.contentsOfDirectory(atPath: inDir.path) where f.hasSuffix(".png") {
    let img = CIImage(contentsOf: inDir.appendingPathComponent(f))!
    let mono = img.applyingFilter("CIFalseColor", parameters: ["inputColor0": CIColor(red: 1.0, green: 0.80, blue: 0.10), "inputColor1": CIColor(red: 0.10, green: 0.08, blue: 0.04)])
    let rep = ctx.pngRepresentation(of: mono, format: .RGBA8, colorSpace: CGColorSpace(name: CGColorSpace.sRGB)!)!
    try rep.write(to: outDir.appendingPathComponent(f))
}
