# lightwave-cli

CLI for LightWave-Server: manage presets, colors, and brightness, and feed
the music visualizer preset with live audio.

```sh
cargo build --release
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

### Capturing what's playing (Linux/PipeWire)

By default the capture stream links to your *input* device (microphone or
line-in). To visualize the music you're playing, capture the output sink's
monitor instead: find the sink id with `wpctl status` and pass it as
`--target-node`:

```sh
wpctl status                  # Sinks: * 57. Ryzen HD Audio Controller ...
lightwave music --target-node 57
```

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
