Feature: html nodes for embedding engines
  An html node is one HTML page rendered as a layer by another engine: moonsplice's `html` node kind
  raea_node_info / raea_node_close in include/raea.h, over src/node.rs. Engines link
  libraea.dylib (scripts/build-dylib.sh), built in raea's own target directory; they pin a
  raea commit and never edit raea in place.

  Background:
    Given scripts/build-dylib.sh writes target/dylib/libraea.dylib and raea.h
    And tools/capi/build-node-smoke.sh builds the C smoke test against it

  Scenario: The node's promises
    Then every resource (CSS, fonts, images, scripts) loads in raea_node_open, and the network is sealed after it
    And a fetch attempted later fails and is listed in raea_node_info's lateFetches, never loaded late
    And raea_node_render(t) is the same bytes for any order of t
    And frames are premultiplied RGBA8, width*scale x height*scale, transparent where the page paints nothing
    And a node is used by the thread that opened it; nodes on different threads are independent
    And `cargo test --release --lib node` and target/raea_node_smoke check each of these

  Scenario: Options
    Then opts_json takes width, height, scale, profile ("web" by default, or "html"), bake (run classic
      scripts once at open; false strips them), fonts ([{family, path}], declared before layout) and network
      (http(s) at open, off by default)
    And an unknown key is an error that names the known ones

  Scenario: Profiles
    Then profile "html" applies the composition format's deterministic fonts (Inter in front of system stacks), as its
      Chrome renders do, and is the default for compositions (`raea --profile`, RenderOptions.profile)
    And profile "web" reads the page as a browser would: font stacks resolve as written; it is the node default

  Scenario: Seek determinism (gate R2)
    Given bench/seek-determinism.py renders every parity case's frames sorted, shuffled, reversed and one process
      per frame, and compares decoded RGBA md5s
    Then all 59 cases are identical in every order (2026-10-13)
    And two causes were fixed to get there:
      | case             | cause                                                                                         |
      | canvas2d/drawing | canvas text used fallback faces only on the first frame rendered; now only at t = 0            |
      | effects/tracks   | an off-grid exact-time decode shifted the video decoder's frame grid for the next frame        |
