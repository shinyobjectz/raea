Feature: The media plane bake-off (G7), on the real UGC edit
  Four backends behind one MediaBackend seam, measured on the UGC edit built from Buddy's eval take (talking head cut
  on sentences, word-chunked captions, two b-roll inserts, a music bed ducked under speech; 738 frames, 24.6 s,
  1080x1920 at 30 fps). M4 Mac, 2026-10-13. The raw numbers are in bench/ and ../rdb/bench/.

  Background:
    Given the edit references its sources by content hash (the media table), with an edit mezzanine for any source
      the portable decoders can't read (B-frames: broll-screen.mov)
    And its second b-roll, broll-rot.mov, is a segment of Buddy's take stored landscape with a -90 degree display
      matrix (2026-10-14: it replaced a clip that turned out to be from Kelly's drive; every number here is from the
      corrected edit, re-run by ../rdb/bench/step0.sh)
    And every render runs guarded (bench/guarded.sh in rdb: memory floor, lane eval, decoder and render ceilings)

  Scenario: Decoding is exact in any order and matches ffmpeg
    # any-order: 24 seeks forward vs shuffled, identical frames; SSIM against ffmpeg's frame at the same time
    # (untagged HD read as BT.709); decode fps over 300 sequential frames (bench/decode-sources.txt)
    Then the decoders on the edit's sources measure
      | backend | source                | any order | SSIM mean | SSIM min | decode fps |
      | ffmpeg  | take-h264.mp4         | 24/24     | 1.0000    | 1.0000   | 95.6       |
      | apple   | take-h264.mp4         | 24/24     | 0.9984    | 0.9983   | 98.5       |
      | soft    | take-h264.mp4         | 24/24     | 0.9985    | 0.9984   | 37.3       |
      | av1     | take-av1.mp4          | 24/24     | 0.9985    | 0.9983   | 61.3       |
      | ffmpeg  | broll-screen.mov      | 24/24     | 1.0000    | 1.0000   | 92.2       |
      | apple   | broll-screen.mov      | 24/24     | 0.9922    | 0.9913   | 94.9       |
      | soft    | broll-screen.mezz.mp4 | 24/24     | 0.9995    | 0.9994   | 67.4       |
      | av1     | broll-screen.av1.mp4  | 24/24     | 0.9997    | 0.9996   | 60.2       |
      | ffmpeg  | broll-rot.mov         | 24/24     | 1.0000    | 1.0000   | 95.1       |
      | apple   | broll-rot.mov         | 24/24     | 0.9983    | 0.9981   | 100.2      |
      | soft    | broll-rot.mov         | 24/24     | 0.9985    | 0.9983   | 35.1       |
      | av1     | broll-rot.av1.mp4     | 24/24     | 0.9985    | 0.9983   | 63.0       |
    And every row is >= 0.99
    # found and fixed on the way: soft used openh264's BT.601 conversion, soft and av1 ignored the rotation matrix,
    # ffmpeg's reference read untagged HD as BT.601, and the Apple decoder leaked VideoToolbox sessions (23 GB)

  Scenario: The edit's rendered frames are byte-identical across runs and orders
    # 738 frame hashes per run (rdb render -o X.txt), ../rdb/examples/out/ref/
    Then with the apple decoders, two runs and one shuffled run give the same 738 hashes
    And with the soft decoders, two runs and one shuffled run give the same 738 hashes
    And the soft decoder's frames on Linux (aarch64, in a container) hash the same as on the Mac (a64d0a5c26cd9af3)
    And the reference hashes for the corrected edit are ../rdb/examples/out/ref/hash-{apple,soft}-{a,b,shuffled}.txt

  Scenario: Encoding the whole edit
    # full render including encode, 8 Mbps, SSIM against the uncompressed rendered frames (every 30th),
    # ../rdb/examples/out/encode-table.jsonl
    Then the encoders measure
      | decode + encode         | render incl. encode | encode ms/frame | SSIM   | size    | audio              |
      | apple + VideoToolbox    | 70.7 fps            | 0.75            | 0.9790 | 25.1 MB | AAC (AudioToolbox) |
      | ffmpeg + libx264        | 23.9 fps            | 5.00            | 0.9709 | 25.4 MB | AAC (ffmpeg)       |
      | soft + openh264         | 20.2 fps            | 14.12           | 0.9690 | 24.8 MB | Opus               |
      | soft + rav1e (speed 10) | 1.7 fps             | 547.27          | 0.9764 | 24.9 MB | Opus               |
    And the Apple path renders the edit at 2.4x real time; the portable path at 0.7x
    # (step 0 path, RGBA readback; G8's GPU frame path renders the same edit at 188 fps incl. encode: ../rdb/docs/g8.feature)

  Scenario: The outputs play
    # ffprobe -count_frames; ffmpeg decode errors; AVFoundation (QuickTime's engine) playable, then every video
    # frame and audio sample decoded through AVAssetReader, headless (../rdb/bench/playback.jsonl)
    Then every output has 738 video frames and 24.6 s of audio, no ffmpeg decode errors
    And AVFoundation plays all four, decoding 738 frames and all audio samples
    # found and fixed: muxide read AV1's decoder_model_info flag without timing info, writing a wrong av1C (tier,
    # level, monochrome) that VideoToolbox refused (vendor/muxide)

  Scenario: Audio as columns
    Then the column mix matches ffmpeg's amix to -117.3 dB residual on the same lanes (soft decode, resample, cut, gain)
    And Symphonia's AAC decode matches ffmpeg's to -134 dB and AudioToolbox's to -136 dB, sample-aligned (gapless)
    And the edit's master lands at -14.2 LUFS (ffmpeg's ebur128 agrees within 0.1 LU), peaks at -1.5 dBFS
    And the brief's audio scenarios hold, and fail when the ducking rule is removed

  Scenario: Portability
    Then cargo check passes for aarch64-apple-darwin, aarch64-apple-ios, x86_64-unknown-linux-gnu (zig cc, nasm)
      and aarch64-linux-android (NDK) with soft, soft+av1, and soft+av1+apple (apple compiles away off Apple)
    And the soft backend runs end to end in a Linux container (rust:1-slim, aarch64): decode, any-order seeks,
      SSIM vs ffmpeg 0.9985, a rotated source, encode with Opus, Symphonia vs ffmpeg -134 dB, mix vs amix -117 dB
      (bench/linux-container.txt)
    And again on x86_64 (2026-10-14, rust:1-slim under Docker Engine in WSL2, headless): the same decoded frames as the
      Mac and aarch64 (hash a64d0a5c26cd9af3), 24/24 any-order seeks with SSIM vs ffmpeg 0.9985 (rotated source
      0.9985), an H.264 + Opus encode with no decode errors, Symphonia vs ffmpeg loudness within 1e-6 LU, mix vs amix
      -117 dB (bench/linux-container-x86.txt)

  Scenario: Licences
    Then the per-backend record is docs/licences.feature: no GPL in any shipping path; Apple codecs are licensed with
      the OS; AV1 and Opus are royalty-free; a source-built openh264 is not covered by Cisco's H.264 licence

  Scenario: The recommendation
    Then Mac renders on the apple backend: VideoToolbox decode and encode, AAC, muxide (63 fps for the edit)
    And iPhone uses the same apple backend (it compiles for aarch64-apple-ios; not yet run on a device)
    And Linux servers decode with soft (H.264 mezzanines) or rav1d (AV1), and encode with openh264 under an AVC
      licence, or AV1 when there's time (rav1e at speed 10 is 1.9 fps; VA-API/NVDEC are the next backends to add)
    And the winning path's render of the UGC edit is ../rdb/examples/out/ugc-apple.mp4
