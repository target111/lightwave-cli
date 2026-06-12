# lightwave-cli

CLI for LightWave-Server: manage presets, colors, and brightness, and feed
the music visualizer and ambilight presets with live audio and screen
colors.

```sh
cargo install --path crates/lightwave
lightwave presets                 # list presets on the server
lightwave start <preset> --help   # show a preset's args
```

The server URL comes from `--server` or the `LIGHTWAVE_URL` env var
(default `http://localhost:8080`).

## Music visualizer

`lightwave music` captures audio, runs an FFT, and streams log-spaced
spectrum bins over UDP to the `music` preset, which it starts and stops for
you. See `lightwave music --help` for tuning options (`--fft-size`,
`--bins`, `--gain`, `--sample-rate`, `--fps`, `--min-freq`/`--max-freq`).

```sh
lightwave music                      # capture the default input device
lightwave music --list-devices       # show capture devices
lightwave music --device pipewire    # pick a device by substring
```

### Scripting (`--json`)

With `--json`, `lightwave music` emits newline-delimited JSON events on
stdout, so supervisors (status bars, plugins) can track the stream without
polling: a `start` event once audio is flowing, a `stop` event on clean
shutdown, or a final `{"ok": false, ...}` object on error.

```json
{"event":"start","preset":"MusicVisualizer","device":"pipewire","sample_rate":44100,"target":"192.168.10.2:5555","fft_size":2048,"bins":32,"fps":60}
{"event":"stop","reason":"interrupt"}
```

Whether the visualizer preset is active server-side (regardless of who
started it) is a separate question: ask `lightwave running --json`.

### Capturing what's playing (Linux/PipeWire)

By default the capture stream links to your *input* device (microphone or
line-in). To visualize the music you're playing, capture the output sink's
monitor instead: find the sink id with `wpctl status` and pass it as
`--target-node`:

```sh
wpctl status                  # Sinks: * 51. Ryzen HD Audio Controller ...
lightwave music --target-node 51
```

`--target-node` also accepts a node *name* (stable across reboots, unlike
ids) — but only for capture-class nodes such as microphones
(`alsa_input...`). WirePlumber resolves names against capture-suitable
nodes only, so a sink's monitor must be targeted by numeric id; look it up
fresh from `wpctl status` each time.

`--target-node` accepts a PipeWire node id or name and works through the
PipeWire ALSA plugin (`pipewire-alsa` must be installed; the flag sets
`PIPEWIRE_NODE` under the hood). It is also the fix when capture fails with
`snd_pcm_hw_params ... No such file or directory` — that is PipeWire
reporting "no target node available" because no default source is
configured (`wpctl inspect @DEFAULT_AUDIO_SOURCE@` returns -1); pinning the
node sidesteps default-source selection entirely.

On Windows, WASAPI loopback devices appear in `--list-devices`, so system
audio can be captured by picking an output device with `--device`. On
macOS, loopback needs a virtual device such as BlackHole.

## Ambilight (Linux only)

`lightwave ambilight` captures the screen via xdg-desktop-portal +
PipeWire, averages an edge band into N box colors, and streams them over
UDP to the `Ambilight` preset, which it starts and stops for you. The
first run shows the portal's screen picker; the permission is saved after
that (`--reselect` shows it again).

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

`--json` emits the same newline-delimited events as `music`. Building the
`ambilight` feature (on by default) needs the PipeWire headers and
libclang; on non-Linux targets build with `--no-default-features
--features music`.
