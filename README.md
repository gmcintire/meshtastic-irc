# Meshtastic IRC Bridge

A bridge between Meshtastic mesh network and IRC (Internet Relay Chat), allowing bidirectional message relay between the two networks.

## Features

- Connects to Meshtastic devices via USB serial port
- Auto-detects Meshtastic serial port on macOS and Linux
- Connects to IRC servers with TLS/SSL support
- Bidirectional message relay between Meshtastic and IRC
- Configurable via JSON file or command-line arguments
- Channel filtering for both networks

## Requirements

- Rust 1.70 or later
- A Meshtastic device connected via USB
- Internet connection for IRC server access

## Installation

Clone the repository and build:

```bash
git clone <repository-url>
cd meshtastic-irc
cargo build --release
```

## Configuration

Create a configuration file based on the example:

```bash
cp config.example.json config.json
```

Edit `config.json` with your settings:

```json
{
  "irc": {
    "server": "irc.libera.chat",
    "port": 6697,
    "channel": "#meshtastic",
    "nickname": "meshtastic-bridge",
    "username": "meshtastic",
    "realname": "Meshtastic IRC Bridge",
    "use_tls": true
  },
  "meshtastic": {
    "serial_port": "/dev/ttyUSB0",
    "channel": 0
  }
}
```

## Usage

### Default usage (auto-detects serial port, uses config.json):

```bash
cargo run
# or
./target/release/meshtastic-irc
```

### Using a different configuration file:

```bash
./target/release/meshtastic-irc --config my-config.json
```

### Using command-line arguments:

```bash
./target/release/meshtastic-irc \
  --irc-server irc.libera.chat \
  --irc-port 6697 \
  --irc-channel "#meshtastic" \
  --irc-nick "mesh-bridge" \
  --irc-tls true \
  --serial-port /dev/cu.usbserial-1234 \
  --meshtastic-channel 0
```

### Available command-line options:

- `--config <FILE>`: Configuration file path (default: config.json)
- `--irc-server <SERVER>`: IRC server address
- `--irc-port <PORT>`: IRC server port (default: 6697 for TLS, 6667 for non-TLS)
- `--irc-channel <CHANNEL>`: IRC channel to join
- `--irc-nick <NICKNAME>`: IRC nickname
- `--irc-tls <true|false>`: Use TLS/SSL for IRC connection
- `--serial-port <PORT>`: Meshtastic serial port (auto-detected if not specified)
- `--meshtastic-channel <CHANNEL>`: Meshtastic channel number (default: 0)

## How it works

1. The bridge connects to both the Meshtastic device and IRC server
2. Messages from Meshtastic are forwarded to IRC with `[mesh-XXXXXXXX]:` prefix
3. Messages from IRC are forwarded to Meshtastic with `[IRC-nickname]` prefix
4. Only text messages on the configured channels are relayed

## Notes

- The Meshtastic send functionality is currently limited and may require updates based on the Meshtastic API evolution
- Ensure your Meshtastic device is properly configured and connected
- The bridge will log all activities using the Rust logging framework

## License

[Your license here]

## Contributing

Contributions are welcome! Please submit pull requests or open issues for bugs and feature requests.