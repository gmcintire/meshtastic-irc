mod bridge;
mod config;
mod irc_handler;
mod meshtastic_handler;
mod mqtt_handler;
mod serial_detector;

use anyhow::Result;
use bridge::Bridge;
use clap::Parser;
use config::Config;
use env_logger;
use log::{error, info};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about = "Bridge between Meshtastic and IRC", long_about = None)]
struct Args {
    #[arg(short, long, value_name = "FILE", help = "Configuration file path", default_value = "config.json")]
    config: PathBuf,
    
    #[arg(long, help = "IRC server address")]
    irc_server: Option<String>,
    
    #[arg(long, help = "IRC server port")]
    irc_port: Option<u16>,
    
    #[arg(long, help = "IRC channel to join")]
    irc_channel: Option<String>,
    
    #[arg(long, help = "IRC nickname")]
    irc_nick: Option<String>,
    
    #[arg(long, help = "Use TLS/SSL for IRC connection")]
    irc_tls: Option<bool>,
    
    #[arg(long, help = "Meshtastic serial port")]
    serial_port: Option<PathBuf>,
    
    #[arg(long, help = "Meshtastic channel number")]
    meshtastic_channel: Option<u32>,
    
    #[arg(long, help = "MQTT broker address")]
    mqtt_broker: Option<String>,
    
    #[arg(long, help = "MQTT broker port")]
    mqtt_port: Option<u16>,
    
    #[arg(long, help = "MQTT topic")]
    mqtt_topic: Option<String>,
    
    #[arg(long, help = "MQTT username")]
    mqtt_username: Option<String>,
    
    #[arg(long, help = "MQTT password")]
    mqtt_password: Option<String>,
    
    #[arg(long, help = "List available serial ports and exit")]
    list_ports: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logger with custom settings to reduce noise
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .filter_module("meshtastic::connections::stream_buffer", log::LevelFilter::Error)
        .init();
    
    let args = Args::parse();
    
    // Handle --list-ports
    if args.list_ports {
        println!("Available serial ports:");
        match serialport::available_ports() {
            Ok(ports) => {
                if ports.is_empty() {
                    println!("  No serial ports found");
                } else {
                    for port_info in ports {
                        let desc = match &port_info.port_type {
                            serialport::SerialPortType::UsbPort(usb) => {
                                format!("{} - {} (VID:{:04X} PID:{:04X})",
                                    usb.manufacturer.as_deref().unwrap_or("Unknown"),
                                    usb.product.as_deref().unwrap_or("Unknown"),
                                    usb.vid, usb.pid)
                            }
                            _ => "Unknown device".to_string(),
                        };
                        println!("  {} - {}", port_info.port_name, desc);
                    }
                }
            }
            Err(e) => {
                println!("  Error listing ports: {}", e);
            }
        }
        return Ok(());
    }
    
    let mut config = if args.config.exists() {
        info!("Loading config from: {}", args.config.display());
        let config_str = std::fs::read_to_string(&args.config)?;
        match serde_json::from_str::<Config>(&config_str) {
            Ok(c) => {
                info!("Successfully loaded config from file");
                c
            }
            Err(e) => {
                error!("Could not parse config file: {}. Using defaults.", e);
                Config::default()
            }
        }
    } else {
        info!("Config file not found at {}. Using defaults.", args.config.display());
        Config::default()
    };
    
    if let Some(server) = args.irc_server {
        config.irc.server = server;
    }
    if let Some(port) = args.irc_port {
        config.irc.port = port;
    }
    if let Some(channel) = args.irc_channel {
        config.irc.channel = channel;
    }
    if let Some(nick) = args.irc_nick {
        config.irc.nickname = nick;
    }
    if let Some(tls) = args.irc_tls {
        config.irc.use_tls = tls;
    }
    // Handle serial port configuration
    if let Some(port) = args.serial_port {
        config.meshtastic.serial_port = Some(port);
    }
    
    // Handle MQTT configuration
    if let Some(broker) = args.mqtt_broker {
        // If MQTT broker is specified, create MQTT config
        let mqtt_config = config::MqttConfig {
            broker_address: broker,
            port: args.mqtt_port.unwrap_or(1883),
            topic: args.mqtt_topic.unwrap_or_else(|| "meshtastic/2/e/#".to_string()),
            username: args.mqtt_username,
            password: args.mqtt_password,
            client_id: None,
        };
        config.meshtastic.mqtt = Some(mqtt_config);
    }
    
    // Auto-detect serial port if neither serial nor MQTT is configured
    if config.meshtastic.serial_port.is_none() && config.meshtastic.mqtt.is_none() {
        // Try auto-detection
        match serial_detector::detect_meshtastic_port().await {
            Ok(detected_port) => {
                info!("Auto-detected serial port: {}", detected_port.display());
                config.meshtastic.serial_port = Some(detected_port);
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to auto-detect serial port: {}. Please specify with --serial-port or configure MQTT", e));
            }
        }
    }
    
    if let Some(channel) = args.meshtastic_channel {
        config.meshtastic.channel = channel;
    }
    
    info!("Starting Meshtastic-IRC bridge");
    info!("IRC: {}:{} channel {} as {}", 
          config.irc.server, config.irc.port, config.irc.channel, config.irc.nickname);
    
    // Log Meshtastic connection type
    if let Some(mqtt) = &config.meshtastic.mqtt {
        info!("Meshtastic: MQTT {}:{} topic {} channel {}", 
              mqtt.broker_address, mqtt.port, mqtt.topic, config.meshtastic.channel);
    } else {
        info!("Meshtastic: Serial {} channel {}", 
              config.meshtastic.serial_port.as_ref()
                  .map(|p| p.display().to_string())
                  .unwrap_or_else(|| "auto-detect".to_string()), 
              config.meshtastic.channel);
    }
    
    info!("Initializing connections...");
    let bridge = Bridge::new(config);
    bridge.run().await
}