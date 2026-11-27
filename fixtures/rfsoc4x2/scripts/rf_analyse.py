#!/usr/bin/env python3
"""What the RF loopback captured: for each ADC input, its level and strongest tone, compared with the tones the two DACs
play (probe_rf_loopback.tcl): from the waveform memories (memory_<input>.hex) and from the converters' mixers
(mixer_<input>.hex), 32-bit words as xsdb prints them, in the given directory. With --require-tones, fails unless each
DAC's tone arrives at some input in both runs (cables fitted). Standard library only."""
import cmath
import math
import sys
from pathlib import Path

RATE = 2457.6e6  # ADC samples per second after decimation
RUNS = {  # file prefix: what plays, and each DAC's frequency
    "memory": ("waveform memory", {"DAC B (vout00)": 1365 * 75e3, "DAC A (vout20)": 2731 * 75e3}),
    "mixer": ("constant and mixer", {"DAC B (vout00)": 307.2e6, "DAC A (vout20)": 614.4e6}),
}
N = 32768


def samples(path: Path) -> list[int]:
    out = []
    for line in path.read_text().split():
        w = int(line, 0) & 0xFFFFFFFF if not line.startswith("-") else int(line) & 0xFFFFFFFF
        for half in (w & 0xFFFF, w >> 16):  # the first sample is in the low half
            out.append(half - 0x10000 if half >= 0x8000 else half)
    return out


def fft(x: list[complex]) -> list[complex]:
    n = len(x)
    if n == 1:
        return x
    even, odd = fft(x[0::2]), fft(x[1::2])
    t = [cmath.exp(-2j * math.pi * k / n) * odd[k] for k in range(n // 2)]
    return [even[k] + t[k] for k in range(n // 2)] + [even[k] - t[k] for k in range(n // 2)]


def analyse(path: Path, tones: dict[str, float]) -> set[str] | None:
    """Print one capture's summary; the tones found in it, or None if the converter delivers a constant."""
    x = samples(path)[:N]
    name = path.stem.split("_", 1)[1]
    name = {"vin2_01": "ADC B (vin2_01)", "vin2_23": "ADC A (vin2_23)"}.get(name, name)
    mean = sum(x) / len(x)
    rms = math.sqrt(sum((v - mean) ** 2 for v in x) / len(x))
    if rms == 0:
        print(f"PROBE FAIL  {name}: the converter delivers a constant")
        return None
    window = [0.5 - 0.5 * math.cos(2 * math.pi * i / N) for i in range(N)]  # Hann
    spectrum = [abs(v) for v in fft([complex((s - mean) * w) for s, w in zip(x, window)])[: N // 2]]
    k = max(range(4, len(spectrum)), key=spectrum.__getitem__)

    def dbfs(bin_: int) -> float:
        return 20 * math.log10(max(4 * spectrum[bin_] / N, 1e-9) / 32768)

    others = [v for i, v in enumerate(spectrum) if i >= 4 and abs(i - k) > 8]
    above = 20 * math.log10(spectrum[k] / (sorted(others)[len(others) // 2] + 1e-12))
    line = (f"  {name}: range {min(x)}..{max(x)}, rms {rms:.1f}; strongest {k * RATE / N / 1e6:.3f} MHz "
            f"at {dbfs(k):.1f} dBFS, {above:.0f} dB above the median")
    found = set()
    for tone, f in tones.items():
        if abs(k * RATE / N - f) <= 2 * RATE / N and dbfs(k) > -40:
            found.add(tone)
            line += f"  <- {tone}"
            for other, g in tones.items():  # how much of the other DAC's tone leaks into this input
                if other != tone:
                    at = round(g * N / RATE)
                    line += f"; {other} at {max(dbfs(i) for i in range(at - 1, at + 2)):.0f} dBFS"
    print(line)
    return found


def main() -> int:
    directory = Path(sys.argv[1])
    require = "--require-tones" in sys.argv
    ok = True
    for prefix, (what, tones) in RUNS.items():
        files = sorted(directory.glob(f"{prefix}_*.hex"))
        if not files:
            print(f"PROBE FAIL  {what}: no captures")
            ok = False
            continue
        print(f" {what}:")
        heard = set()
        for path in files:
            found = analyse(path, tones)
            ok = ok and found is not None
            heard |= found or set()
        print(f"  tones received: {', '.join(sorted(heard)) or 'none (no cables?)'}")
        if require and len(heard) != len(tones):
            print(f"PROBE FAIL  {what}: not every DAC's tone arrives at an input")
            ok = False
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
