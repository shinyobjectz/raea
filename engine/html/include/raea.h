/*
 * raea.h — C API of the raea renderer (HTML/CSS/GSAP compositions -> frames).
 * Implemented in src/ffi.rs; link libraea.a (raea.xcframework).
 *
 * Strings: paths and JSON are NUL-terminated UTF-8. `char *` results are owned by the caller and
 * freed with raea_free_string. On failure a call returns NULL or a negative status, and
 * raea_last_error() on the same thread describes it.
 *
 * Paths: a project directory (its index.html is loaded) or an .html file. Relative assets resolve
 * against it; http(s) assets load only in builds with the `network` feature (else from the cache
 * at $HOME/Library/Caches/raea, if present).
 *
 * opts_json (NULL or "" = defaults) mirrors the CLI flags:
 *   {"fps": 30 | "30000/1001", "scale": 1, "width": 1080, "height": 1920,
 *    "start": 0.0, "end": 5.0,                  (seconds; render only part of the timeline)
 *    "variables": {...},                        (getVariables(); also JSON text or "@/path")
 *    "backend": "vello" | "skia",               (skia only in builds with the `skia` feature)
 *    "transparent": false, "network": true,
 *    "workers": 2,                              (render threads; default min(cores/2, 2) on iOS)
 *    "videoFrames": "/path/DIR",                (DIR/<videoId>/frame_%06d.(jpg|png))
 *    "pixelFormat": "rgba" | "bgra"}            (frame byte order, see below)
 * Unknown keys are an error.
 *
 * Frames: 8 bits per channel, premultiplied alpha, sRGB, top row first, opaque unless
 * "transparent". "rgba" = R,G,B,A bytes; "bgra" = B,G,R,A bytes = kCVPixelFormatType_32BGRA.
 * Size in pixels = probe's width x height (CSS size x scale).
 *
 * Audio is never rendered: mix probe's "audio" plan in the host
 * ({id, src, start, end, mediaStart, volume, loop, playbackRate, fadeIn, fadeOut}; seconds).
 *
 * Threading: any call may be made from any thread (each blocks until done; keep long ones off the
 * main thread). Compositions hold a JS runtime that never crosses threads: probe/lint/snapshot run
 * on an internal 64 MB-stack thread; a render handle owns "workers" threads, each with its own
 * document + JS runtime + raster buffers (memory grows per worker). Workers are created by the
 * first raea_render_next_frame and inherit that thread's QoS: call it from a .userInitiated (or
 * higher) thread, or the workers may run on efficiency cores. Use a handle from one thread at a
 * time; never call raea_render_end concurrently with another call on the same handle.
 */
#ifndef RAEA_H
#define RAEA_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Status codes. */
#define RAEA_OK 0
#define RAEA_DONE (-1)                 /* raea_render_next_frame: no more frames */
#define RAEA_ERR (-2)                  /* see raea_last_error() */
#define RAEA_ERR_BUFFER_TOO_SMALL (-3)
#define RAEA_ERR_INVALID_ARGUMENT (-4)
#define RAEA_ERR_STATE (-5)            /* e.g. frame provider set after rendering began */

/* Pixel formats (frame provider input). */
#define RAEA_PIXEL_RGBA8 0u
#define RAEA_PIXEL_BGRA8 1u

/* Frame provider results. */
#define RAEA_FRAME_OK 0                /* frame written */
#define RAEA_FRAME_NEED_BUFFER 1       /* width/height/row_bytes set; call me again with a bigger buffer */
#define RAEA_FRAME_NONE (-1)           /* no frame: the video renders empty this frame */

typedef struct RaeaRender raea_render;

/* One decoded <video> frame, written by the frame provider. */
typedef struct RaeaVideoFrame {
    uint8_t *data;          /* [in]  buffer to write into (NULL when capacity is 0) */
    size_t capacity;        /* [in]  bytes available at data */
    uint32_t width;         /* [out] frame size in pixels */
    uint32_t height;
    size_t row_bytes;       /* [out] bytes between rows; 0 = width * 4 */
    uint32_t pixel_format;  /* [out] RAEA_PIXEL_RGBA8 or RAEA_PIXEL_BGRA8 (alpha ignored: opaque) */
} raea_video_frame;

/*
 * Frame provider: write frame `frame_index` of video `video_id` (probe's videos[].id).
 * frame_index is clip-local at the render fps: 0 = the clip's first output frame (same numbering as
 * the videoFrames directory). media_time = mediaStart + frame_index / fps * playbackRate, in
 * seconds into the source file (for "loop" clips wrap it by the asset duration yourself).
 * If row_bytes * height > capacity, set width/height/row_bytes and return RAEA_FRAME_NEED_BUFFER;
 * the call is repeated with a buffer that fits (sizes are remembered per video). Supply frames at
 * about the size they are displayed (object-fit uses their aspect ratio).
 * Called from render worker threads, up to `workers` at once, in roughly increasing frame order
 * (worker k renders output frames k, k + workers, ...); each frame is requested about once (one
 * held past the clip's end may repeat once per worker). Must be thread-safe.
 */
typedef int32_t (*raea_video_frame_fn)(void *user_data, const char *video_id, uint64_t frame_index,
                                     double media_time, raea_video_frame *frame);

/* Library version string (static). */
const char *raea_version(void);

/* Last error on this thread, or NULL. Valid until the next raea_* call on this thread. */
const char *raea_last_error(void);

/* Free a string returned by this API. NULL is ignored. */
void raea_free_string(char *s);

/*
 * Probe: {width, height, cssWidth, cssHeight, fps, duration, frames, rootComposition, backend,
 * timelineDuration, videos[], audio[], warnings[], unsupported[], jsErrors[]} (the CLI's probe).
 * NULL on error.
 */
char *raea_probe(const char *path, const char *opts_json);

/* Lint: {ok, errors[], warnings[], unsupported[]} (the CLI's lint --json). NULL if the composition
 * cannot be loaded. opts_json may be NULL. */
char *raea_lint(const char *path, const char *opts_json);

/*
 * Render the frame at time t (seconds) into out: width * height * 4 bytes, packed rows.
 * <video> frames come from opts "videoFrames" (else render empty, unless built with ffmpeg); for
 * provider-supplied video use a render handle with "start": t, "end": t + 1/fps.
 * Returns RAEA_OK, RAEA_ERR_BUFFER_TOO_SMALL, RAEA_ERR_INVALID_ARGUMENT or RAEA_ERR.
 */
int32_t raea_snapshot(const char *path, double t, const char *opts_json, uint8_t *out, size_t out_len);

/*
 * Begin a render of frames [start, end) (all by default). Loads and probes the composition;
 * workers start on the first raea_render_next_frame. NULL on error. Free with raea_render_end.
 */
raea_render *raea_render_begin(const char *path, const char *opts_json);

/* Probe JSON for the handle plus firstFrame, endFrame (exclusive), frameCount, workers,
 * bytesPerFrame. Free with raea_free_string. */
char *raea_render_info(const raea_render *r);

/*
 * Supply <video> frames from the host (e.g. AVAssetReader); takes precedence over "videoFrames".
 * Call after raea_render_begin and before the first raea_render_next_frame. cb NULL removes it.
 * user_data is passed back unchanged and must stay valid until raea_render_end.
 * Returns RAEA_OK, RAEA_ERR_STATE or RAEA_ERR_INVALID_ARGUMENT.
 */
int32_t raea_set_video_frame_provider(raea_render *r, raea_video_frame_fn cb, void *user_data);

/*
 * Copy the next frame, in order, into rgba (packed rows; bytesPerFrame bytes).
 * Returns the frame index (>= 0; firstFrame, firstFrame + 1, ...), RAEA_DONE after the last
 * frame, or RAEA_ERR_BUFFER_TOO_SMALL / RAEA_ERR_INVALID_ARGUMENT / RAEA_ERR. After RAEA_ERR the handle
 * only returns RAEA_ERR.
 */
int64_t raea_render_next_frame(raea_render *r, uint8_t *rgba, size_t buf_len);

/*
 * Same, with rows row_bytes apart (e.g. a locked CVPixelBuffer's base address and bytesPerRow;
 * use "pixelFormat": "bgra" for kCVPixelFormatType_32BGRA). Needs
 * row_bytes * (height - 1) + width * 4 bytes. row_bytes 0 = packed.
 */
int64_t raea_render_next_frame_strided(raea_render *r, uint8_t *dst, size_t buf_len, size_t row_bytes);

/* Random access (scrubbing, edit previews): the frame at t seconds, rows row_bytes apart
 * (0 = packed) into dst. A separate worker keeps the composition loaded, so after the first call
 * (which loads it) each frame costs one seek + paint. Blocks: call off the main thread and render
 * only the latest playhead time. Independent of raea_render_next_frame on the same handle; set the
 * frame provider before the first call. RAEA_OK, RAEA_ERR_BUFFER_TOO_SMALL, RAEA_ERR_INVALID_ARGUMENT
 * or RAEA_ERR. */
int32_t raea_render_frame_at(raea_render *r, double t, uint8_t *dst, size_t buf_len, size_t row_bytes);

/* Text-layout report at t seconds as JSON (the CLI's `snapshot --boxes` sidecar):
 * {"t","width","height","safeZone","boxes":[{"id","class","tag","text","x","y","w","h",
 * "centerOffset":{"x","y"},"opacity","clipped":[edges],"overflowClipped","inSafeZone":[edges],
 * "overlaps":[indices]}]}, boxes in composition px with transforms applied. safe_zone is
 * "top,right,bottom,left" in composition px, or NULL for 220,140,420,0 at 1080x1920 (none
 * otherwise). Renders t on the raea_render_frame_at worker; blocks. Free with raea_free_string;
 * NULL on error (raea_last_error). */
char *raea_render_boxes_at(raea_render *r, double t, const char *safe_zone);

/* Stop the workers (each finishes its current frame) and free the handle. NULL is ignored. */
void raea_render_end(raea_render *r);

/*
 * html nodes: one HTML page rendered as a layer by an embedding engine (e.g. moonsplice's html
 * node kind). A node is used by the thread that opened it; nodes on different threads are
 * independent. Call from a thread with a large stack (8 MB or more; layout recurses).
 *
 * opts_json (all optional): {"width":1920,"height":1080,"scale":1,"profile":"web"|"html",
 * "bake":true,"fonts":[{"family":"..","path":"/abs/font.ttf"}],"network":false}
 *   profile web (default): fonts resolve as the page writes them; the composition format: the composition format's
 *   deterministic fonts. bake: run the page's classic scripts once at open (false strips them).
 *   network: allow http(s) at open (local files and data: URLs always load).
 * Every resource loads in raea_node_open; afterwards the network is sealed and a late fetch is an
 * error listed in raea_node_info's lateFetches.
 */
typedef struct RaeaNode raea_node;

/* Open a node from markup; base_url is a file:// URL of the page or its folder, or NULL/"".
 * NULL on error (raea_last_error). Free with raea_node_close. */
raea_node *raea_node_open(const char *html, const char *base_url, const char *opts_json);

/* The frame at t seconds: premultiplied RGBA8, width*scale x height*scale, transparent where the
 * page paints nothing, `stride` bytes per row (0 = width*scale*4). The same bytes for any order of
 * t. RAEA_OK, RAEA_ERR_BUFFER_TOO_SMALL, RAEA_ERR_INVALID_ARGUMENT or RAEA_ERR. */
int32_t raea_node_render(raea_node *n, double t, uint8_t *rgba, size_t buf_len, size_t stride);

/* Text boxes at t as JSON (as raea_render_boxes_at). Free with raea_free_string; NULL on error. */
char *raea_node_boxes(raea_node *n, double t);

/* Facts as JSON: width, height, scale, duration (0 if static), profile, fonts, fetched [{url,bytes}],
 * failed [{url,error}], lateFetches, warnings, jsErrors, inputsHash (FNV-1a of markup, options and
 * resources). Free with raea_free_string. */
char *raea_node_info(raea_node *n);

/* Free a node (NULL is ignored), from the thread that opened it. */
void raea_node_close(raea_node *n);

#ifdef __cplusplus
}
#endif

#endif /* RAEA_H */
