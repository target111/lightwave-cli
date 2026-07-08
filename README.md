# lightwave-cli

[![CI](https://github.com/target111/lightwave-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/target111/lightwave-cli/actions/workflows/ci.yml)

CLI for LightWave-Server: run effects, manage shared presets, colors, and
brightness, and feed the music visualizer and ambilight effects with live
audio and screen colors.

```sh
cargo install --path crates/lightwave
lightwave effects                 # list effects on the server
lightwave start <effect> --help   # show an effect's args
lightwave leds                    # current strip state
```

The server URL comes from `--server` or the `LIGHTWAVE_URL` env var
(default `http://localhost:8080`).

## Presets

A preset is a saved effect configuration, stored on the server so every
client sees the same list. `preset save` accepts the same args as `start`
plus an optional `--description`; `start` takes an effect or a preset name
(the server keeps them from colliding).

```sh
lightwave preset save cozy Fire --cooling 1.3 --description "warm evenings"
lightwave preset list
lightwave start cozy
lightwave preset rm cozy
```

## Music visualizer

`lightwave music` captures audio, runs an FFT, and streams log-spaced
spectrum bins over UDP to the `MusicVisualizer` effect, which it starts and
stops for you. See `lightwave music --help` for tuning options (`--fft-size`,
`--bins`, `--gain`, `--sample-rate`, `--fps`, `--min-freq`/`--max-freq`).

```sh
lightwave music                      # capture whatever is playing
lightwave music --list-devices       # show capture devices
lightwave music --device alc897      # pick a device by substring
```

### Scripting (`--json`)

With `--json`, `lightwave music` emits newline-delimited JSON events on
stdout, so supervisors (status bars, plugins) can track the stream without
polling: a `start` event once audio is flowing, a `stop` event on clean
shutdown, or a final `{"ok": false, ...}` object on error.

```json
{"event":"start","effect":"MusicVisualizer","device":"output_default","sample_rate":48000,"target":"192.168.10.2:5555","fft_size":2048,"bins":32,"fps":60}
{"event":"stop","reason":"interrupt"}
```

Whether the visualizer effect is active server-side (regardless of who
started it) is a separate question: ask `lightwave running --json`.

### Choosing what to capture

On Linux, cpal uses PipeWire natively (falling back to ALSA when PipeWire
isn't running). By default `lightwave music` captures the default output
sink's monitor, so it visualizes whatever is playing. To capture something
specific, pass `--device <substring>`: an output sink picks up its monitor,
an input device (microphone, line-in) is captured directly. Run
`--list-devices` to see the names.

On Windows, WASAPI loopback devices appear in `--list-devices`, so system
audio can be captured by picking an output device with `--device`. On
macOS, loopback needs a virtual device such as BlackHole.

## Ambilight (Linux & Windows)

`lightwave ambilight` captures the screen (xdg-desktop-portal + PipeWire
on Linux, Windows.Graphics.Capture on Windows), averages an edge band
into N box colors, and streams them over UDP to the `Ambilight` effect,
which it starts and stops for you. On Linux the first run shows the
portal's screen picker and saves the permission (`--reselect` shows it
again); on Windows it captures the primary monitor, or pick one with
`--monitor <N>`.

```sh
lightwave ambilight                    # bottom edge, 16 boxes, 30 fps
lightwave ambilight --edge left --reverse
lightwave ambilight --boxes 32 --depth 0.3
```

Color tuning (see `--help` for everything):

- `--depth` — how far the sampled band reaches in from the edge.
- `--vividness` — how strongly colorful pixels outweigh grey ones in the
  average (0 = plain mean).
- `--gamma` — brightness gamma matching the strip to the screen (default
  2.2; the strip's PWM is linear, so raw sRGB values make dark scenes
  glow). Hue-preserving: it dims, it never shifts colors.
- `--min-saturation` — render near-grey content as a clear dim color by
  amplifying its existing tint (0 = off; ~0.3 is plenty).

`--json` emits the same newline-delimited events as `music`. On Linux the
`music` and `ambilight` features (on by default) need the PipeWire headers
and libclang; Windows builds with the stock toolchain. Other platforms:
build with `--no-default-features --features music`.
