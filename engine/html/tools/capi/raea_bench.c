// raea_bench: drives the C API (include/raea.h) the way the iOS app would, and measures it.
//
//   raea_bench probe    <path> [opts_json]
//   raea_bench lint     <path> [opts_json]
//   raea_bench snapshot <path> <t> [opts_json] [out.rgba]
//   raea_bench render   <path> [opts_json] [--provider FRAMES_DIR] [--out out.rgba] [--strided] [--hash]
//
// render prints load time, time to first frame, fps, with --hash an FNV-1a hash of all frames
// (or --out them and cmp with `raea render --format rgba`), and peak memory: phys_footprint (what iOS's jetsam
// limits) and max RSS. --provider decodes FRAMES_DIR/<videoId>/frame_%06d.jpg with ImageIO into
// padded BGRA rows, exercising the frame provider like an AVAssetReader host would.
// --strided renders into a buffer with padded rows (64-byte aligned, as CVPixelBuffer pools do).
//
// Build: tools/capi/build.sh (macOS) or tools/capi/build.sh --ios-sim / --ios (iOS executables, run
// the simulator one with `xcrun simctl spawn <device> target/raea_bench-ios-sim ...`)

#include "raea.h"

#include <CoreGraphics/CoreGraphics.h>
#include <ImageIO/ImageIO.h>
#include <mach/mach.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/resource.h>
#include <time.h>

static double now(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return ts.tv_sec + ts.tv_nsec / 1e9;
}

static void memory(double *footprint_mb, double *peak_footprint_mb, double *maxrss_mb) {
    task_vm_info_data_t info;
    mach_msg_type_number_t count = TASK_VM_INFO_COUNT;
    if (task_info(mach_task_self(), TASK_VM_INFO, (task_info_t)&info, &count) == KERN_SUCCESS) {
        *footprint_mb = info.phys_footprint / 1048576.0;
        *peak_footprint_mb = info.ledger_phys_footprint_peak / 1048576.0;
    }
    struct rusage ru;
    getrusage(RUSAGE_SELF, &ru);
    *maxrss_mb = ru.ru_maxrss / 1048576.0; // bytes on macOS
}

static double cpu_seconds(void) {
    struct rusage ru;
    getrusage(RUSAGE_SELF, &ru);
    return ru.ru_utime.tv_sec + ru.ru_utime.tv_usec / 1e6 + ru.ru_stime.tv_sec + ru.ru_stime.tv_usec / 1e6;
}

static void die(const char *what) {
    const char *e = raea_last_error();
    fprintf(stderr, "%s failed: %s\n", what, e ? e : "(no message)");
    exit(1);
}

// FNV-1a over 64-bit words (n is a multiple of 8 here: rows are width * 4 bytes, width even)
static uint64_t fnv(uint64_t h, const uint8_t *p, size_t n) {
    size_t i = 0;
    for (; i + 8 <= n; i += 8) {
        uint64_t w;
        memcpy(&w, p + i, 8);
        h ^= w;
        h *= 0x100000001b3ULL;
    }
    for (; i < n; i++) {
        h ^= p[i];
        h *= 0x100000001b3ULL;
    }
    return h;
}

// --- frame provider: ImageIO-decoded JPEGs, delivered as BGRA with padded rows -------------

static const char *g_frames_dir;
static atomic_long g_provider_calls, g_provider_retries;
static double g_provider_seconds; // approximate (racy adds are fine for a benchmark)

static int32_t provide(void *user_data, const char *video_id, uint64_t frame_index, double media_time,
                       raea_video_frame *frame) {
    (void)user_data;
    (void)media_time;
    double t0 = now();
    char path[4096];
    snprintf(path, sizeof path, "%s/%s/frame_%06llu.jpg", g_frames_dir, video_id, (unsigned long long)frame_index);
    CFURLRef url = CFURLCreateFromFileSystemRepresentation(NULL, (const UInt8 *)path, strlen(path), false);
    CGImageSourceRef src = url ? CGImageSourceCreateWithURL(url, NULL) : NULL;
    if (url) CFRelease(url);
    if (!src) return RAEA_FRAME_NONE;
    CGImageRef img = CGImageSourceCreateImageAtIndex(src, 0, NULL);
    CFRelease(src);
    if (!img) return RAEA_FRAME_NONE;
    size_t w = CGImageGetWidth(img), h = CGImageGetHeight(img);
    size_t row = (w * 4 + 63) & ~(size_t)63;
    frame->width = (uint32_t)w;
    frame->height = (uint32_t)h;
    frame->row_bytes = row;
    frame->pixel_format = RAEA_PIXEL_BGRA8;
    atomic_fetch_add(&g_provider_calls, 1);
    if (frame->capacity < row * h) {
        atomic_fetch_add(&g_provider_retries, 1);
        CGImageRelease(img);
        return RAEA_FRAME_NEED_BUFFER;
    }
    CGColorSpaceRef cs = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
    CGContextRef ctx = CGBitmapContextCreate(frame->data, w, h, 8, row, cs,
                                             kCGImageAlphaPremultipliedFirst | kCGBitmapByteOrder32Little);
    CGColorSpaceRelease(cs);
    CGContextDrawImage(ctx, CGRectMake(0, 0, w, h), img);
    CGContextRelease(ctx);
    CGImageRelease(img);
    g_provider_seconds += now() - t0;
    return RAEA_FRAME_OK;
}

// --- tiny JSON field reader (numbers only) ----------------------------------------------------

static double json_num(const char *json, const char *key) {
    char pat[128];
    snprintf(pat, sizeof pat, "\"%s\":", key);
    const char *p = strstr(json, pat);
    return p ? atof(p + strlen(pat)) : -1;
}

int main(int argc, char **argv) {
    if (argc < 3) {
        fprintf(stderr, "usage: raea_bench probe|lint|snapshot|render <path> ...\n");
        return 2;
    }
    const char *cmd = argv[1], *path = argv[2];
    fprintf(stderr, "raea %s\n", raea_version());

    if (!strcmp(cmd, "probe") || !strcmp(cmd, "lint")) {
        const char *opts = argc > 3 ? argv[3] : NULL;
        double t0 = now();
        char *json = !strcmp(cmd, "probe") ? raea_probe(path, opts) : raea_lint(path, opts);
        if (!json) die(cmd);
        printf("%s\n", json);
        fprintf(stderr, "%s took %.0f ms\n", cmd, (now() - t0) * 1e3);
        raea_free_string(json);
        return 0;
    }

    if (!strcmp(cmd, "snapshot")) {
        double t = argc > 3 ? atof(argv[3]) : 0;
        const char *opts = argc > 4 ? argv[4] : NULL;
        char *info = raea_probe(path, opts);
        if (!info) die("probe");
        size_t w = (size_t)json_num(info, "width"), h = (size_t)json_num(info, "height");
        raea_free_string(info);
        size_t len = w * h * 4;
        uint8_t *buf = malloc(len);
        double t0 = now();
        if (raea_snapshot(path, t, opts, buf, len) != RAEA_OK) die("snapshot");
        fprintf(stderr, "snapshot %zux%zu at %.3fs took %.0f ms, hash %016llx\n", w, h, t, (now() - t0) * 1e3,
                (unsigned long long)fnv(0xcbf29ce484222325ULL, buf, len));
        if (argc > 5) {
            FILE *f = fopen(argv[5], "wb");
            fwrite(buf, 1, len, f);
            fclose(f);
        }
        free(buf);
        return 0;
    }

    if (strcmp(cmd, "render")) {
        fprintf(stderr, "unknown command %s\n", cmd);
        return 2;
    }
    const char *opts = NULL, *out_path = NULL;
    int strided = 0, hashing = 0;
    for (int i = 3; i < argc; i++) {
        if (!strcmp(argv[i], "--provider") && i + 1 < argc) g_frames_dir = argv[++i];
        else if (!strcmp(argv[i], "--out") && i + 1 < argc) out_path = argv[++i];
        else if (!strcmp(argv[i], "--strided")) strided = 1;
        else if (!strcmp(argv[i], "--hash")) hashing = 1;
        else opts = argv[i];
    }
    double fp0, peak0, rss0;
    memory(&fp0, &peak0, &rss0);
    double t0 = now();
    raea_render *r = raea_render_begin(path, opts);
    if (!r) die("raea_render_begin");
    double t_begin = now() - t0;
    char *info = raea_render_info(r);
    size_t w = (size_t)json_num(info, "width"), h = (size_t)json_num(info, "height");
    long frames = (long)json_num(info, "frameCount");
    int workers = (int)json_num(info, "workers");
    raea_free_string(info);
    if (g_frames_dir && raea_set_video_frame_provider(r, provide, NULL) != RAEA_OK) die("raea_set_video_frame_provider");

    size_t row = strided ? ((w * 4 + 63) & ~(size_t)63) + 64 : w * 4;
    size_t len = row * h;
    uint8_t *buf = malloc(len);
    FILE *out = out_path ? fopen(out_path, "wb") : NULL;
    uint64_t hash = 0xcbf29ce484222325ULL;
    double t_first = 0, t1 = now(), cpu1 = cpu_seconds();
    long n = 0;
    for (;;) {
        int64_t idx = strided ? raea_render_next_frame_strided(r, buf, len, row) : raea_render_next_frame(r, buf, len);
        if (idx == RAEA_DONE) break;
        if (idx < 0) die("raea_render_next_frame");
        if (n == 0) t_first = now() - t1;
        if (hashing || out)
            for (size_t y = 0; y < h; y++) {
                if (hashing) hash = fnv(hash, buf + y * row, w * 4);
                if (out) fwrite(buf + y * row, 1, w * 4, out);
            }
        n++;
    }
    double t_render = now() - t1, cpu_render = cpu_seconds() - cpu1;
    double fp, peak, rss;
    memory(&fp, &peak, &rss);
    raea_render_end(r);
    if (out) fclose(out);
    free(buf);
    printf("{\"size\":\"%zux%zu\",\"frames\":%ld,\"expected\":%ld,\"workers\":%d,\"beginMs\":%.0f,\"firstFrameMs\":%.0f,"
           "\"renderS\":%.2f,\"fps\":%.2f,\"cpuMsPerFrame\":%.1f,\"peakFootprintMB\":%.0f,\"footprintAtEndMB\":%.0f,\"maxRssMB\":%.0f,"
           "\"providerCalls\":%ld,\"providerRetries\":%ld,\"providerMsPerCall\":%.1f,\"hash\":\"%016llx\"}\n",
           w, h, n, frames, workers, t_begin * 1e3, t_first * 1e3, t_render, n / t_render, n ? cpu_render * 1e3 / n : 0.0, peak, fp, rss,
           (long)g_provider_calls, (long)g_provider_retries,
           g_provider_calls ? g_provider_seconds * 1e3 / g_provider_calls : 0.0, (unsigned long long)hash);
    return 0;
}
