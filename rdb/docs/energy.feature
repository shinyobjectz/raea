Feature: Energy: what it is, measured from the descriptor table, and shaped as curves
  Status 2026-10-14: the definition, from the user (dictated 2026-10-14). Nothing is built yet; the steps are D5 and
  D6 in ../../the roadmap. Everything here is a scan over columns the ingest and the edit already produce, keyed by
  the source map, so the energy layer is the same shape of computation as the low-level temporal checks (cuts,
  flashes, mid-word cuts): windowed scans in K, scheduled in time order.

  Rule: Energy is a blend of descriptors that rise and fall together, collapsed into one number per moment
    Scenario: The descriptors
      Given a moment of an edit (a frame, or a short window)
      Then its energy descriptors are
        | descriptor             | from                                                                   |
        | motion magnitude       | optical flow, or the sketch's frame-to-frame change (previz)           |
        | cut rate               | cuts per second around the moment (the edit's rows; TransNetV2 on renders) |
        | luminance and contrast | mean luma and its spread (brighter and higher contrast reads as more)   |
        | audio loudness         | short-term loudness (EBU R128)                                          |
        | spectral density       | spectral flux and how full the spectrum is                              |
      And the descriptors are bound into one hypervector per moment (HDC role-filler binding, as previz binds rows),
        not weighted by hand
      And energy is that moment vector projected onto an energy direction: a hypervector made from moments labelled
        high against moments labelled low (the graded runs, ad cards, the founder's picks), so the direction is
        measured, not guessed

  Rule: The energy curve, and its salient nodes
    Scenario: From a line to the turns that matter
      Given energy at every moment of the edit, plotted over its length
      When the curve is smoothed to remove the jitter a viewer doesn't feel
      Then its salient nodes are the smoothed curve's local maxima (payoffs), local minima (breaths) and inflection
        points (where the direction turns); the stretches between are the slopes
      And the edit's strategy is the arrangement of those nodes, which is what an agent adjusts

  Rule: Energy is relative: what is felt is local contrast, not level
    Scenario: The flatline failure
      Given a motion graphics reel that lives at 0.8-1.0 and a calm piece at 0.1-0.3
      Then both need rise and fall within their own band; constant high energy reads as none (the viewer
        habituates)
      And the measure is the curve's local contrast: its variance in a sliding window relative to its own recent
        average
      And a stretch pinned at its level with near-zero local contrast is reported as a flatline, however loud

  Rule: Two curves to shape: intensity (up and down) and aperture (in and out)
    Scenario: Aperture
      Then aperture is the concentration of the frame: one dominant focal point is in, many competing elements is
        out (G1's saliency focus and the focal-point measure in the roadmap's catalogue)
      And a piece breathes on both: tight and intense, then wide and overwhelming, then tight again

  Rule: The music is one energy curve among several, not the master clock
    Scenario: Relating the picture to the music
      Given the music's energy curve from loudness and spectral flux, its beats as salient nodes on it
      Then the measure is the windowed correlation between the visual energy curve and the music's
      And each section has a target for it: locked (cuts land on the beat at the big moments) or deliberately offset
        (cutting against the beat for tension); cutting on every beat is the over-indexing failure
      And sound effects count toward energy too: the depth of layers (how many instruments or sources at once), and
        whether diegetic and non-diegetic sound are mixed on purpose
      And generated sound effects (the user: Fish Audio is available; to be tested, licence and terms checked) must
        fit their moment: a tail that drags past its picture is a dangling tail (the catalogue's check)

  Rule: Every operation is a scan over the descriptor table
    Scenario: The scan family
      Then these are scans over columns, written as K kernels (meaning/src/k.rs on the CPU, kgpu on the GPU)
        | operation                     | scan                                   |
        | smoothing                     | windowed mean or filter over one column |
        | salient nodes                 | turning points of the smoothed column   |
        | local contrast                | windowed variance over its windowed mean |
        | picture-music relation        | windowed correlation over two columns   |
        | flatline                      | a threshold on local contrast, as spans |
      And the same sequential scheduling seam runs these and the temporal checks (flash safety, mid-word cuts,
        dangling tails), so they are built once

  Rule: Built (D5a, an agent session): the scan family as kgpu kernels
    Scenario: The scans, 2026-10-15
      Given kgpu/kernels/energy_{smooth,nodes,contrast,flat,cov}.k on Q16 columns (Q8 inside the squares), with
        native references in raea_kgpu (energy_*_reference) and the CPU interpreter of the same K
      When kgpu's energytest runs them on a synthetic two-sine curve (600 rows) and on a real column (the UGC take's
        CV delta norms, 205 rows at 8 rows/s, from its ingest)
      Then every scan is GPU = native = interpreter, bit for bit, on both
      And on the synthetic curve: 11 peaks, 11 troughs, 61 inflections, 209 flat rows (contrast under 40)
      And on the take: 38 peaks, 38 troughs, 68 inflections, 55 flat rows
      And the picture-music relation is energy_cov.k's per-row (cov, va, vb); the host takes r = cov / sqrt(va vb)
      And next: the descriptor columns from an edit through its source map (motion from the CV deltas or the look's
        Hamming steps, cut rate from the rows, luma and contrast from the sketch, loudness from the mix), the HDC
        moment vector, and the energy direction once labelled high and low moments exist

    Scenario: rdb energy on the UGC example, 2026-10-15
      Given descriptors per frame from rdb itself: motion (the look's Hamming step), luma and contrast (the look grid's
        cells), cuts per second (the rows), loudness (the mix's RMS); each normalised over its own 5th-95th percentile
      And the blend is their mean, marked provisional until labelled moments give a measured direction
      And the curve is smoothed twice over about a second, then the scans run on the GPU; peaks and troughs whose
        swing is under 0.06 are merged (a zigzag filter: only rises and falls a viewer feels)
      When `rdb energy examples/ugc/ugc.sql` runs
      Then payoffs at 4.2, 11.6, 17.0 and 20.0 s, breaths at 9.6, 15.9 and 18.5 s, one flatline 23.0-24.6 s (the end
        card), picture-music r about 0 (the bed is ducked under speech), in 2.2 s

    Scenario: Aperture and the eye across cuts (G1's attention through the source map), 2026-10-15
      Given each frame's focus from previz::attention (1 - the saliency map's normalised entropy at the top clip's
        mapped source moment), normalised, smoothed twice and turned like the energy curve
      When `rdb energy examples/ugc/ugc.sql` runs
      Then aperture tightens ("in") at 1.8, 16.5 and 21.0 s and opens ("out") at 9.2 and 18.2 s (the b-roll), focus
        median 0.126 (raea-23's G1 measured the same), eye jumps at cuts 0.01 within the take and 0.03 into b-roll
      And motion is read from the sources' CV deltas through the source map (motion_from says which)

