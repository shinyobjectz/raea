#!/usr/bin/env python3
"""Clicks in a sound file (no-V8 goal, gate 6): a click is a sample whose second difference (the curvature a step or a
cut leaves) stands far above the sound around it: over RATIO times the median curvature of the 10 ms around it and over
an absolute floor (-40 dBFS), so a loud attack in loud sound isn't one and a step in a quiet passage is. Prints the
integrated loudness and true peak (ffmpeg's ebur128) and the clicks found, per channel, with their times.
    clicks.py FILE [FILE...]"""
import array, re, subprocess, sys

RATIO, FLOOR, RATE = 40.0, 10 ** (-40 / 20), 48000

def samples(path):
    raw = subprocess.run(["ffmpeg", "-v", "error", "-i", path, "-f", "f32le", "-ac", "2", "-ar", str(RATE), "-"], capture_output=True, check=True).stdout
    a = array.array("f"); a.frombytes(raw)
    return a[0::2], a[1::2]

def clicks(x):
    d2 = [abs(x[i] - 2 * x[i - 1] + x[i - 2]) for i in range(2, len(x))]
    w = RATE // 100
    out = []
    for start in range(0, len(d2), w):
        block = d2[max(0, start - w): start + 2 * w]
        med = sorted(block)[len(block) // 2] + 1e-9
        for i in range(start, min(start + w, len(d2))):
            if d2[i] > FLOOR and d2[i] > RATIO * med:
                out.append((i + 2) / RATE)
    # one click per 5 ms
    merged = []
    for t in out:
        if not merged or t - merged[-1] > 0.005:
            merged.append(t)
    return merged

for path in sys.argv[1:]:
    meter = subprocess.run(["ffmpeg", "-nostats", "-i", path, "-af", "ebur128=peak=true", "-f", "null", "-"], capture_output=True, text=True).stderr
    i = re.findall(r"I:\s+(-?[\d.]+) LUFS", meter)[-1]
    tp = re.findall(r"Peak:\s+(-?[\d.]+) dBFS", meter)[-1]
    found = [clicks(c) for c in samples(path)]
    times = sorted(set(round(t, 3) for c in found for t in c))
    print(f"{path}: {i} LUFS, true peak {tp} dBTP, clicks {len(times)}" + (f" at {times[:8]} s" if times else ""))
