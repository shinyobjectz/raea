// raea's perception (raea/perception/perception.feature): footage read the way an editor reads it, in Rust.
// Paths are UTF-8; every call returns JSON the caller frees with raea_perception_free; failures are {"error": "..."}.
#pragma once
#ifdef __cplusplus
extern "C" {
#endif

// Ingest a clip once, keyed by its content (facts, sound, performance, SigLIP columns) into `store` (NULL: the
// default, $RAEA_INGEST or ~/.cache/raea-ingest): {"dir", "hash", "steps"}.
char *raea_perception_ingest(const char *clip, const char *store);

// A clip's facts (facts.json and a still per shot) into `dir`, returned as the facts JSON.
char *raea_perception_analyze(const char *clip, const char *dir);

void raea_perception_free(char *s);

#ifdef __cplusplus
}
#endif
