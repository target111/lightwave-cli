# lightwave-cli

CLI for LightWave-Server: manage presets, colors, and brightness, and feed
the music visualizer preset with live audio.

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
