# LightWave CLI

A command-line client for controlling LightWave LED server.

## Features

- Control LED strip colors and brightness
- Manage LED effects (list, view, start, stop)
- Pretty-printed, colored output
- Structured error handling
- Parameter handling for effect configuration

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/target111/lightwave-cli
cd lightwave-cli

# Build and install
cargo install --path .
```

## Usage

```bash
# Show help
lightwave-cli --help

# List all available effects
lightwave-cli effects list

# Get detailed info about a specific effect
lightwave-cli effects info <effect-name>

# Start an effect with parameters
lightwave-cli effects start <effect-name> --param speed=0.5 --param color="#ff0000"

# Stop the currently running effect
lightwave-cli effects stop

# Show the currently running effect
lightwave-cli effects running

# Set LED color (supports any color format: hex, rgb, hsl, hsv, etc.)
lightwave-cli leds color "#ff0000"
lightwave-cli leds color "rgb(255,0,0)"
lightwave-cli leds color "red"

# Set LED brightness (0.0-1.0)
lightwave-cli leds brightness 0.5

# Turn off all LEDs
lightwave-cli leds clear

# Get the current status
lightwave-cli status

# Connect to a different server (default is http://localhost:8000/api)
lightwave-cli --base-url http://other-server:8000/api effects list
```

## Examples

### Starting the Rainbow Effect

```bash
lightwave-cli effects start rainbow --param speed=0.8 --param brightness=0.7
```

### Setting a Custom Color

```bash
lightwave-cli leds color "#00ff00"  # Green
```

## License

This project is licensed under the MIT License - see the LICENSE file for details.
