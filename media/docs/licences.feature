Feature: What the media plane may ship, per backend
  raea-media's backends, what each links, under which licence, and which codec patents apply. Checked against
  `cargo metadata` (no GPL or AGPL crate anywhere in the dependency graph). This is an engineering record, not legal advice: a release that ships H.264 or AAC encoders in
  our own binaries goes past legal review first.

  Background:
    Given muxing is muxide (MIT OR Apache-2.0) and demuxing is re_mp4 (MIT) on every backend
    And the audio columns, mixing, resampling and loudness are our own code (raea-media, unpublished)

  Scenario: ffmpeg is the baseline, never the shipping path
    Given the "ffmpeg" backend runs the ffmpeg and ffprobe command-line tools as separate processes
    Then nothing from ffmpeg is linked into our binaries
    And the Homebrew ffmpeg build used for the bench is GPL (it includes libx264), so it stays a bench and test tool
    And no shipping build calls it

  Scenario: soft, portable software (macOS, iOS, Linux, Android)
    Given H.264 decode and encode are openh264 (BSD-2-Clause), built from source by openh264-sys2 (BSD-2-Clause)
    And audio decode is Symphonia (MPL-2.0: file-level copyleft; we ship its files unmodified)
    And Opus encode is libopus through opusic-sys (BSD-3-Clause) and opus (MIT OR Apache-2.0)
    Then the code licences allow closed-source shipping
    But H.264 is patented (the Via LA AVC pool): Cisco pays the royalties only for its prebuilt openh264 binary,
      downloaded at run time and unmodified; an openh264 we build from source is not covered
    And so shipping our own H.264 encoder or decoder needs either Cisco's binary at run time or our own AVC licence
      (the pool has free tiers, such as internet video free to end users and small unit counts, but that is legal's
      call)
    And AAC decode (Symphonia) falls under the Via LA AAC pool for the same reason
    And Opus is royalty-free (its contributors license their patents royalty-free)

  Scenario: apple, platform hardware (macOS, iOS)
    Given decode is AVFoundation (VideoToolbox) and encode is a VTCompressionSession (VideoToolbox) with AAC from
      AudioToolbox, muxed by muxide (MIT OR Apache-2.0); AVAssetWriter remains behind RAEA_APPLE_ENC=writer
    And the bindings are objc2 (MIT) and the objc2 framework crates, objc2-video-toolbox among them (Zlib OR
      Apache-2.0 OR MIT)
    Then the codecs are Apple's, licensed with the OS: H.264, HEVC and AAC carry no royalty for us
    And this is the recommended shipping path on Mac and iPhone

  Scenario: av1, royalty-free (every platform)
    Given decode is rav1d (BSD-2-Clause, the Rust port of dav1d), built from the v1.1.0 tag with its assembly
    And encode is rav1e (BSD-2-Clause)
    And audio is Opus (royalty-free)
    Then the code licences allow closed-source shipping
    And AV1 is licensed royalty-free by AOMedia's patent licence
    But a third-party pool (Sisvel) claims AV1 patents; no major platform pays it, and it is noted here, not settled

  Scenario: later backends slot in behind the same trait
    Given Android's MediaCodec and Linux's VA-API or NVDEC are not implemented yet
    Then on Android the device's codecs are licensed with the device, as on Apple
    And on Linux servers VA-API and NVDEC drivers are covered by the hardware vendor; their user-space libraries
      (libva MIT, NVIDIA's SDK headers MIT) would be checked again when they land

  Scenario: the recommendation that follows from the licences
    Then Mac and iPhone ship the apple backend (no codec royalty, hardware speed)
    And Linux servers render with av1 (royalty-free) or with soft H.264 under an AVC licence or Cisco's binary
    And nothing GPL is in any shipping path
