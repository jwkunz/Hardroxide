#!/usr/bin/env python3
"""Writes chime.wav: a short rising arpeggio with a decaying envelope, mono, 16 bits, 8 kHz (about 2 s, 32 KB),
so the audio applications have a file to play that is made from nothing (public domain, deterministic).
"""
import math, struct, sys

RATE = 8000
NOTES = [(60, 0.30), (64, 0.30), (67, 0.30), (72, 0.30), (67, 0.20), (72, 0.60)]  # MIDI pitch, seconds
samples = []
for midi, dur in NOTES:
    f = 440.0 * 2 ** ((midi - 69) / 12)
    n = int(RATE * dur)
    for i in range(n):
        t = i / RATE
        env = min(1.0, i / 80) * math.exp(-3.0 * i / n)  # click-free attack, bell-like decay
        v = math.sin(2 * math.pi * f * t) + 0.3 * math.sin(2 * math.pi * 2 * f * t)
        samples.append(int(max(-1, min(1, 0.45 * env * v)) * 32767))
data = b"".join(struct.pack("<h", s) for s in samples)
header = (b"RIFF" + struct.pack("<I", 36 + len(data)) + b"WAVEfmt " + struct.pack("<IHHIIHH", 16, 1, 1, RATE, RATE * 2, 2, 16)
          + b"data" + struct.pack("<I", len(data)))
open(sys.argv[1] if len(sys.argv) > 1 else "chime.wav", "wb").write(header + data)
