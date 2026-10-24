#!/usr/bin/env python3
"""Synthesises assets/sounds/spooky.wav and witching.wav from scratch.

Both are original: sine and FM-bell voices, a vibrato glide and a short echo,
generated here with numpy and written as 16-bit mono WAV. Run from the
repository root.
"""
import wave
import numpy as np

RATE = 44100

def t(seconds):
    return np.arange(int(RATE * seconds)) / RATE

def env(n, attack, decay):
    x = np.arange(n) / RATE
    return np.minimum(x / attack, 1.0) * np.exp(-x / decay)

def bell(freq, seconds=1.0, ratio=3.5, index=2.2, decay=0.28):
    x = t(seconds)
    mod = np.sin(2 * np.pi * freq * ratio * x) * index * np.exp(-x / (decay * 0.6))
    return np.sin(2 * np.pi * freq * x + mod) * env(len(x), 0.004, decay)

def glide(f0, f1, seconds, vibrato=6.0, depth=0.015):
    x = t(seconds)
    freq = np.linspace(f0, f1, len(x))
    freq = freq * (1 + depth * np.sin(2 * np.pi * vibrato * x) * np.minimum(x / 0.25, 1))
    phase = 2 * np.pi * np.cumsum(freq) / RATE
    wave_ = np.sin(phase) + 0.35 * np.sin(2 * phase) + 0.12 * np.sin(3 * phase)
    return wave_ * np.sin(np.pi * np.linspace(0, 1, len(x))) ** 1.5

def place(out, sound, at):
    start = int(at * RATE)
    out[start:start + len(sound)] += sound[: len(out) - start]

def echo(x, delay, gain, repeats):
    out = np.zeros(len(x) + int(delay * RATE) * repeats)
    for i in range(repeats + 1):
        place(out, x * gain ** i, delay * i)
    return out

def save(name, x):
    x = x / max(1e-9, np.max(np.abs(x))) * 0.85
    x = x[: np.nonzero(np.abs(x) > 0.03)[0][-1] + 1].copy()
    fade = int(0.02 * RATE)
    x[-fade:] *= np.linspace(1, 0, fade)
    with wave.open(f"assets/sounds/{name}.wav", "wb") as f:
        f.setnchannels(1); f.setsampwidth(2); f.setframerate(RATE)
        f.writeframes((x * 32767).astype("<i2").tobytes())

# Spooky: a music-box phrase in E minor that sinks into a ghostly "oooo".
m = np.zeros(int(RATE * 1.9))
for at, hz in [(0.00, 659.3), (0.08, 783.99), (0.16, 987.77), (0.26, 932.33)]:
    place(m, bell(hz, 0.5, decay=0.12) * 0.7, at)
place(m, glide(520, 330, 0.42) * 0.28, 0.3)
save("spooky", echo(m, 0.12, 0.3, 2))

# Witching: a bat's rising squeaks over a low gong, for mentions.
w = np.zeros(int(RATE * 2.2))
for at, f0, f1 in [(0.0, 1500, 2600), (0.09, 1800, 3000), (0.18, 2100, 3400)]:
    x = t(0.09)
    sweep = np.sin(2 * np.pi * np.cumsum(np.linspace(f0, f1, len(x))) / RATE)
    place(w, sweep * np.sin(np.pi * np.linspace(0, 1, len(x))) * 0.25, at)
for hz, gain in [(146.8, 0.9), (155.6, 0.7), (220.0, 0.35), (329.6, 0.2)]:
    place(w, bell(hz, 0.9, ratio=2.76, index=1.4, decay=0.22) * gain, 0.24)
place(w, glide(260, 200, 0.4, vibrato=5, depth=0.03) * 0.18, 0.3)
save("witching", echo(w, 0.14, 0.3, 1))
