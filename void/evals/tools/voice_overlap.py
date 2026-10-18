"""Does a reference say what the raws say? The fraction of its word trigrams found in the raws' transcripts (parakeet-mlx,
local). A creator's edit of the same raws scores 0.75-0.96; a different video (an animated cut, another script) 0-0.2."""
import hashlib, os, re, subprocess, sys
CACHE = os.path.expanduser("~/raea/media/library/transcripts")  # local: transcripts of client footage
os.makedirs(CACHE, exist_ok=True)

def text(path, maxsec):
    key = hashlib.sha1(f"{path}|{maxsec}".encode()).hexdigest()[:16]
    out = os.path.join(CACHE, key + ".txt")
    if not os.path.exists(out):
        wav = os.path.join(CACHE, key + ".wav")
        subprocess.run(["ffmpeg", "-v", "error", "-y", "-t", str(maxsec), "-i", path, "-vn", "-ac", "1", "-ar", "16000", wav], timeout=600)
        if not os.path.exists(wav): open(out, "w").close(); return ""
        subprocess.run(["parakeet-mlx", wav, "--output-dir", CACHE, "--output-format", "txt"], capture_output=True, timeout=900)
        os.remove(wav)
        if not os.path.exists(out): open(out, "w").close()
    return open(out).read()

def toks(s): return re.findall(r"[a-z0-9']+", s.lower())

def overlap(ref, raws):
    r = toks(text(ref, 90))
    if len(r) < 6: return None  # no talk in the reference
    have = set()
    for w in raws:
        t = toks(text(w, 600)); have |= {tuple(t[i:i + 3]) for i in range(len(t) - 2)}
    tri = [tuple(r[i:i + 3]) for i in range(len(r) - 2)]
    return sum(x in have for x in tri) / len(tri)

if __name__ == "__main__":
    print(overlap(sys.argv[1], sys.argv[2:]))
