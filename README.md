# Meshtastic IRC Bridge

A bridge between Meshtastic mesh network and IRC (Internet Relay Chat), allowing bidirectional message relay between the two networks.

## Features

- Connects to Meshtastic devices via USB serial port or MQTT
- Auto-detects Meshtastic serial port on macOS and Linux
- Connects to IRC servers with TLS/SSL support
- Bidirectional message relay between Meshtastic and IRC
- Configurable via JSON file or command-line arguments
- Channel filtering for both networks
- Shows Meshtastic node short names instead of raw IDs
- Acknowledges received Meshtastic messages when requested

## Requirements

- Rust 1.70 or later
- Either:
  - A Meshtastic device connected via USB, or
  - Access to an MQTT broker with Meshtastic messages
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

Edit `config.json` with your settings. See `config.example.jsonc` for a fully commented version with all available options.

### Serial/USB connection example:

```json
{
  "irc": {
    "server": "irc.libera.chat",
    "port": 6697,
    "channel": "#meshtastic",
    "nickname": "meshtastic-bridge",
    "use_tls": true
  },
  "meshtastic": {
    "serial_port": "/dev/ttyUSB0",
    "channel": 0
  }
}
```

### MQTT connection example:

```json
{
  "irc": {
    "server": "irc.libera.chat",
    "port": 6697,
    "channel": "#meshtastic",
    "nickname": "mesh-bridge",
    "use_tls": true
  },
  "meshtastic": {
    "mqtt": {
      "broker_address": "mqtt.meshtastic.org",
      "port": 1883,
      "topic": "meshtastic/2/e/#"
    },
    "channel": 0
  }
}
```

Note: Choose either `serial_port` OR `mqtt` configuration, not both. If neither is specified, the bridge will attempt to auto-detect a connected Meshtastic device.

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

### Using MQTT instead of serial:

```bash
./target/release/meshtastic-irc \
  --mqtt-broker mqtt.meshtastic.org \
  --mqtt-port 1883 \
  --mqtt-topic "meshtastic/2/e/#"
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
- `--mqtt-broker <ADDRESS>`: MQTT broker address
- `--mqtt-port <PORT>`: MQTT broker port (default: 1883)
- `--mqtt-topic <TOPIC>`: MQTT topic to subscribe to
- `--mqtt-username <USERNAME>`: MQTT username (optional)
- `--mqtt-password <PASSWORD>`: MQTT password (optional)
- `--list-ports`: List available serial ports and exit

## How it works

1. The bridge connects to both the Meshtastic network (via serial or MQTT) and IRC server
2. Messages from Meshtastic are forwarded to IRC with `[mesh-nodename]:` prefix (or `[mesh-XXXXXXXX]:` if name unknown)
3. Messages from IRC are forwarded to Meshtastic with `[IRC-nickname]` prefix
4. Only text messages on the configured channels are relayed
5. The bridge discovers and uses Meshtastic node short names as they appear
6. Received Meshtastic messages are acknowledged if the sender requests it

## Notes

- The Meshtastic send functionality is currently limited and may require updates based on the Meshtastic API evolution
- Ensure your Meshtastic device is properly configured and connected
- The bridge will log all activities using the Rust logging framework

## License

[Your license here]

## Contributing

Contributions are welcome! Please submit pull requests or open issues for bugs and feature requests.