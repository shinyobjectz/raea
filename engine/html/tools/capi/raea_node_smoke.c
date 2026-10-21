// raea_node_smoke: the html node API (include/raea.h, raea_node_*) as an embedding engine uses it,
// linked against libraea.dylib. Exits non-zero on any broken promise.
//
//   scripts/build-dylib.sh && tools/capi/build-node-smoke.sh && target/raea_node_smoke
//
// Checks: frames are the same bytes in any order of t, premultiplied, transparent where unpainted;
// stride and buffer-size errors; boxes and info JSON; a bad option fails with a message.

#include "raea.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static const char *PAGE =
    "<!doctype html><html><head><style>body{margin:0}"
    "#b{position:absolute;left:10px;top:10px;width:50px;height:30px;background:#e84a5f;"
    "animation:a 2s linear both}@keyframes a{to{left:200px;opacity:.4}}"
    "p{position:absolute;left:10px;top:60px;margin:0;font:18px sans-serif}</style></head>"
    "<body><div id=\"b\"></div><p>hello node</p></body></html>";

static int fails = 0;
#define CHECK(cond, ...) do { if (!(cond)) { fprintf(stderr, "FAIL: " __VA_ARGS__); fputc('\n', stderr); fails++; } } while (0)

static unsigned long long fnv(const uint8_t *p, size_t n) {
    unsigned long long h = 0xcbf29ce484222325ULL;
    for (size_t i = 0; i < n; i++) { h ^= p[i]; h *= 0x100000001b3ULL; }
    return h;
}

int main(void) {
    const char *opts = "{\"width\":300,\"height\":120,\"scale\":2}";
    raea_node *a = raea_node_open(PAGE, NULL, opts);
    raea_node *b = raea_node_open(PAGE, "", opts);
    CHECK(a && b, "open: %s", raea_last_error() ? raea_last_error() : "?");
    if (!a || !b) return 1;

    const size_t w = 600, h = 240, stride = w * 4 + 32, len = stride * h;
    uint8_t *buf = malloc(len);
    const double ts[] = {0.0, 0.4, 1.0, 1.6, 1.99};
    unsigned long long fwd[5], back[5];
    for (int i = 0; i < 5; i++) {
        CHECK(raea_node_render(a, ts[i], buf, len, stride) == RAEA_OK, "render a %g: %s", ts[i], raea_last_error());
        fwd[i] = fnv(buf, len);
    }
    for (int i = 4; i >= 0; i--) {
        memset(buf, 0, len);
        CHECK(raea_node_render(b, ts[i], buf, len, stride) == RAEA_OK, "render b %g", ts[i]);
        back[i] = fnv(buf, len);
    }
    for (int i = 0; i < 5; i++) CHECK(fwd[i] == back[i], "t=%g differs by seek order", ts[i]);
    CHECK(fwd[0] != fwd[2], "the animation should move");

    raea_node_render(a, 0.0, buf, len, stride);
    CHECK(buf[(h - 1) * stride + (w - 1) * 4 + 3] == 0, "unpainted pixel should be transparent");
    for (size_t y = 0; y < h; y++)
        for (size_t x = 0; x < w; x++) {
            const uint8_t *p = buf + y * stride + x * 4;
            if (p[0] > p[3] || p[1] > p[3] || p[2] > p[3]) { CHECK(0, "not premultiplied at %zu,%zu", x, y); y = h; break; }
        }

    CHECK(raea_node_render(a, 0.0, buf, 100, stride) == RAEA_ERR_BUFFER_TOO_SMALL, "small buffer should fail");
    CHECK(raea_node_render(a, 0.0, buf, len, 16) == RAEA_ERR_BUFFER_TOO_SMALL, "short stride should fail");
    CHECK(raea_node_render(a, 0.0, NULL, len, stride) == RAEA_ERR_INVALID_ARGUMENT, "NULL buffer should fail");

    char *boxes = raea_node_boxes(a, 0.5);
    CHECK(boxes && strstr(boxes, "hello node"), "boxes should hold the text");
    raea_free_string(boxes);
    char *info = raea_node_info(a);
    CHECK(info && strstr(info, "\"profile\":\"web\"") && strstr(info, "inputsHash"), "info: %s", info ? info : "NULL");
    raea_free_string(info);

    CHECK(raea_node_open(PAGE, NULL, "{\"bogus\":1}") == NULL && raea_last_error() != NULL, "bad option should fail with a message");

    raea_node_close(a);
    raea_node_close(b);
    raea_node_close(NULL);
    free(buf);
    if (fails) { fprintf(stderr, "%d failure(s)\n", fails); return 1; }
    printf("raea_node_smoke: ok (%s)\n", raea_version());
    return 0;
}
