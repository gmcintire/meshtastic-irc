use anyhow::Result;
use log::{debug, error, info};
use meshtastic::api::StreamApi;
use meshtastic::protobufs::{mesh_packet, FromRadio, MeshPacket, PortNum, Data};
use meshtastic::utils;
use tokio::sync::mpsc;
use std::collections::HashMap;

use crate::config::MeshtasticConfig;
use crate::irc_handler::IrcMessage;

pub struct MeshtasticHandler {
    stream_api: meshtastic::api::ConnectedStreamApi,
    decoded_listener: mpsc::UnboundedReceiver<FromRadio>,
    channel: u32,
    node_names: HashMap<u32, String>,  // Map node IDs to short names
}

impl MeshtasticHandler {
    pub async fn new(config: &MeshtasticConfig) -> Result<Self> {
        let stream_api = StreamApi::new();
        
        let serial_port = config.serial_port.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Serial port not specified"))?;
            
        info!("Connecting to Meshtastic device at {}", serial_port.display());
        
        // Standard Meshtastic serial settings
        let baud_rate = Some(115200);
        let dtr = Some(true);  // Data Terminal Ready
        let rts = Some(true);  // Request To Send
        
        let serial_stream = utils::stream::build_serial_stream(
            serial_port.to_str().unwrap().to_string(),
            baud_rate,
            dtr,
            rts,
        ).map_err(|e| {
            if e.to_string().contains("Device or resource busy") {
                anyhow::anyhow!(
                    "Serial port {} is busy. Make sure no other Meshtastic apps are running.\n\
                    Common causes:\n\
                    - Meshtastic Python CLI is running\n\
                    - Meshtastic web interface is open\n\
                    - Another serial terminal is connected\n\
                    Try: lsof {} (on macOS/Linux) to see what's using it",
                    serial_port.display(),
                    serial_port.display()
                )
            } else {
                anyhow::anyhow!("Failed to open serial port {}: {}", serial_port.display(), e)
            }
        })?;
        
        let (decoded_listener, stream_api) = stream_api.connect(serial_stream).await;
        
        // Give the device a moment to settle after connection
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        
        info!("Meshtastic device connected, skipping initial packet wait");
        
        // Configure with a random ID
        let config_id = utils::generate_rand_id();
        let stream_api = stream_api
            .configure(config_id)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to configure: {}", e))?;
        
        Ok(Self {
            stream_api,
            decoded_listener,
            channel: config.channel,
            node_names: HashMap::new(),
        })
    }

    pub async fn run(
        mut self,
        mut from_irc: mpsc::Receiver<IrcMessage>,
        to_irc: mpsc::Sender<String>,
    ) -> Result<()> {
        info!("Meshtastic handler run loop started, listening on channel {}", self.channel);
        
        loop {
            tokio::select! {
                Some(from_radio) = self.decoded_listener.recv() => {
                    debug!("Received packet from Meshtastic radio");
                    if let Err(e) = self.handle_meshtastic_packet(from_radio, &to_irc).await {
                        error!("Error handling Meshtastic packet: {}", e);
                    }
                }
                Some(message) = from_irc.recv() => {
                    info!("Received message from IRC to send to Meshtastic: {} - {}", 
                          message.sender, message.content);
                    if let Err(e) = self.send_to_meshtastic(&message).await {
                        error!("Error sending to Meshtastic: {}", e);
                    }
                }
                else => {
                    debug!("No messages in either channel");
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            }
        }
    }

    async fn handle_meshtastic_packet(
        &mut self,
        from_radio: FromRadio,
        to_irc: &mpsc::Sender<String>,
    ) -> Result<()> {
        match from_radio.payload_variant {
            Some(meshtastic::protobufs::from_radio::PayloadVariant::Packet(mesh_packet)) => {
                debug!("Received MeshPacket on channel {}, configured channel is {}", 
                      mesh_packet.channel, self.channel);
                
                // Only process messages from our configured channel
                if mesh_packet.channel == self.channel {
                    self.process_mesh_packet(mesh_packet, to_irc).await?;
                } else {
                    debug!("Ignoring packet from channel {}", mesh_packet.channel);
                }
            }
            Some(meshtastic::protobufs::from_radio::PayloadVariant::NodeInfo(node_info)) => {
                // Store node information
                let node_id = node_info.num;
                if let Some(user) = node_info.user {
                    let short_name = user.short_name.clone();
                    if !short_name.is_empty() {
                        info!("Discovered node: {} (ID: {:08x})", short_name, node_id);
                        self.node_names.insert(node_id, short_name);
                    }
                }
            }
            Some(meshtastic::protobufs::from_radio::PayloadVariant::MyInfo(my_info)) => {
                info!("Connected to Meshtastic node: ID {:08x}", my_info.my_node_num);
            }
            Some(other) => {
                debug!("Received non-packet payload: {:?}", other);
            }
            None => {
                debug!("Received empty payload");
            }
        }
        
        Ok(())
    }

    async fn process_mesh_packet(
        &mut self,
        packet: MeshPacket,
        to_irc: &mpsc::Sender<String>,
    ) -> Result<()> {
        // Check if the packet wants an ACK
        let wants_ack = packet.want_ack;
        let packet_id = packet.id;
        let from_node = packet.from;
        
        // Check if this is a Data packet with decoded payload
        if let Some(payload_variant) = &packet.payload_variant {
            match payload_variant {
                mesh_packet::PayloadVariant::Decoded(data) => {
                    // Only process text messages
                    if data.portnum() == PortNum::TextMessageApp {
                        if data.payload.len() > 0 {
                            if let Ok(text) = std::str::from_utf8(&data.payload) {
                                // Use short name if available, otherwise use ID
                                let sender = self.node_names.get(&packet.from)
                                    .cloned()
                                    .unwrap_or_else(|| format!("{:08x}", packet.from));
                                let message = format!("[mesh-{}]: {}", sender, text);
                                
                                info!("Received Meshtastic message: {}", message);
                                to_irc.send(message).await?;
                                debug!("Forwarded Meshtastic message to IRC");
                                
                                // Send ACK if requested
                                if wants_ack && packet_id != 0 {
                                    self.send_ack(packet_id, from_node).await?;
                                }
                            }
                        }
                    }
                }
                _ => {
                    // Ignore encrypted or other packet types
                }
            }
        }
        
        Ok(())
    }

    async fn send_to_meshtastic(&mut self, message: &IrcMessage) -> Result<()> {
        let text = format!("[IRC-{}] {}", message.sender, message.content);
        
        // Create a text message data payload
        let data = Data {
            portnum: PortNum::TextMessageApp as i32,
            payload: text.as_bytes().to_vec(),
            want_response: false,
            ..Default::default()
        };
        
        // Create mesh packet for broadcast
        let mesh_packet = MeshPacket {
            to: 0xffffffff, // Broadcast address
            from: 0, // Will be filled by the device
            channel: self.channel,
            id: 0, // Will be assigned by the device
            priority: mesh_packet::Priority::Default as i32,
            payload_variant: Some(mesh_packet::PayloadVariant::Decoded(data)),
            ..Default::default()
        };
        
        // Create the payload variant
        let payload_variant = Some(meshtastic::protobufs::to_radio::PayloadVariant::Packet(mesh_packet));
        
        // Send using the stream API's send_to_radio_packet method
        info!("Attempting to send packet to Meshtastic radio...");
        match self.stream_api.send_to_radio_packet(payload_variant).await {
            Ok(_) => {
                info!("Successfully sent to Meshtastic: {}", text);
                Ok(())
            }
            Err(e) => {
                error!("Failed to send to Meshtastic: {}", e);
                Err(anyhow::anyhow!("Failed to send message: {}", e))
            }
        }
    }

    async fn send_ack(&mut self, packet_id: u32, to_node: u32) -> Result<()> {
        debug!("Sending ACK for packet {} to node {:08x}", packet_id, to_node);
        
        // Create an empty Data payload for the ACK
        let data = Data {
            portnum: PortNum::RoutingApp as i32,
            payload: vec![],
            want_response: false,
            request_id: packet_id, // This is the packet we're acknowledging
            ..Default::default()
        };
        
        // Create an ACK packet - this is a routing packet with ACK flag
        let mesh_packet = MeshPacket {
            to: to_node,
            from: 0, // Will be filled by the device
            channel: 0, // ACKs typically use channel 0
            id: 0, // Will be assigned by the device
            priority: mesh_packet::Priority::Ack as i32,
            payload_variant: Some(mesh_packet::PayloadVariant::Decoded(data)),
            want_ack: false, // Don't request ACK for our ACK
            ..Default::default()
        };
        
        // Create the payload variant
        let payload_variant = Some(meshtastic::protobufs::to_radio::PayloadVariant::Packet(mesh_packet));
        
        // Send the ACK
        match self.stream_api.send_to_radio_packet(payload_variant).await {
            Ok(_) => {
                debug!("Successfully sent ACK for packet {}", packet_id);
                Ok(())
            }
            Err(e) => {
                error!("Failed to send ACK for packet {}: {}", packet_id, e);
                Err(anyhow::anyhow!("Failed to send ACK: {}", e))
            }
        }
    }
}