use anyhow::Result;
use log::{debug, error, info};
use rumqttc::{AsyncClient, Event, EventLoop, MqttOptions, Packet, QoS};
use tokio::sync::mpsc;
use std::collections::HashMap;
use std::time::Duration;

use crate::config::MqttConfig;
use crate::irc_handler::IrcMessage;
use meshtastic::protobufs::{mesh_packet, MeshPacket, PortNum, Data, ServiceEnvelope};

pub struct MqttHandler {
    client: AsyncClient,
    eventloop: EventLoop,
    topic: String,
    channel: u32,
    node_names: HashMap<u32, String>,
}

impl MqttHandler {
    pub async fn new(config: &MqttConfig, channel: u32) -> Result<Self> {
        let client_id = config.client_id.clone()
            .unwrap_or_else(|| format!("meshtastic-irc-{}", std::process::id()));
        
        info!("Connecting to MQTT broker {}:{}", config.broker_address, config.port);
        
        let mut mqtt_options = MqttOptions::new(
            client_id,
            &config.broker_address,
            config.port,
        );
        
        mqtt_options.set_keep_alive(Duration::from_secs(30));
        
        // Set credentials if provided
        if let (Some(username), Some(password)) = (&config.username, &config.password) {
            mqtt_options.set_credentials(username, password);
        }
        
        let (client, eventloop) = AsyncClient::new(mqtt_options, 100);
        
        Ok(Self {
            client,
            eventloop,
            topic: config.topic.clone(),
            channel,
            node_names: HashMap::new(),
        })
    }
    
    pub async fn run(
        mut self,
        from_irc: mpsc::Receiver<IrcMessage>,
        to_irc: mpsc::Sender<String>,
    ) -> Result<()> {
        // Subscribe to the Meshtastic topic
        self.client.subscribe(&self.topic, QoS::AtLeastOnce).await?;
        info!("Subscribed to MQTT topic: {}", self.topic);
        
        // Spawn task to handle messages from IRC
        let client_clone = self.client.clone();
        let topic = self.topic.clone();
        let channel = self.channel;
        tokio::spawn(async move {
            Self::handle_irc_messages(from_irc, client_clone, topic, channel).await;
        });
        
        // Main event loop
        loop {
            match self.eventloop.poll().await {
                Ok(event) => {
                    if let Err(e) = self.handle_mqtt_event(event, &to_irc).await {
                        error!("Error handling MQTT event: {}", e);
                    }
                }
                Err(e) => {
                    error!("MQTT connection error: {}", e);
                    // Try to reconnect after a delay
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
    }
    
    async fn handle_irc_messages(
        mut from_irc: mpsc::Receiver<IrcMessage>,
        client: AsyncClient,
        topic: String,
        channel: u32,
    ) {
        while let Some(message) = from_irc.recv().await {
            debug!("Received message from IRC: {} - {}", message.sender, message.content);
            
            if let Err(e) = Self::send_to_mqtt(&client, &topic, &message, channel).await {
                error!("Failed to send message to MQTT: {}", e);
            }
        }
    }
    
    async fn send_to_mqtt(
        client: &AsyncClient,
        topic: &str,
        message: &IrcMessage,
        channel: u32,
    ) -> Result<()> {
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
            channel,
            id: 0, // Will be assigned by the device
            priority: mesh_packet::Priority::Default as i32,
            payload_variant: Some(mesh_packet::PayloadVariant::Decoded(data)),
            ..Default::default()
        };
        
        // Create service envelope
        let service_envelope = ServiceEnvelope {
            packet: Some(mesh_packet),
            channel_id: format!("LongFast"),
            gateway_id: format!("irc-bridge"),
        };
        
        // Serialize to protobuf
        let payload = prost::Message::encode_to_vec(&service_envelope);
        
        info!("Sending to MQTT topic {}: {}", topic, text);
        client.publish(topic, QoS::AtLeastOnce, false, payload).await?;
        
        Ok(())
    }
    
    async fn handle_mqtt_event(
        &mut self,
        event: Event,
        to_irc: &mpsc::Sender<String>,
    ) -> Result<()> {
        match event {
            Event::Incoming(Packet::Publish(publish)) => {
                debug!("Received MQTT message on topic: {}", publish.topic);
                
                // Only process messages from our subscribed topic
                if publish.topic == self.topic {
                    // Try to decode as ServiceEnvelope
                    match prost::Message::decode(&publish.payload[..]) {
                        Ok(envelope) => {
                            let service_envelope: ServiceEnvelope = envelope;
                            if let Some(packet) = service_envelope.packet {
                                self.process_mesh_packet(packet, to_irc).await?;
                            }
                        }
                        Err(e) => {
                            debug!("Failed to decode ServiceEnvelope: {}", e);
                        }
                    }
                }
            }
            Event::Incoming(Packet::ConnAck(_)) => {
                info!("Connected to MQTT broker");
            }
            Event::Incoming(Packet::SubAck(_)) => {
                info!("Successfully subscribed to topic");
            }
            Event::Incoming(Packet::Disconnect) => {
                info!("Disconnected from MQTT broker");
            }
            _ => {}
        }
        
        Ok(())
    }
    
    async fn process_mesh_packet(
        &mut self,
        packet: MeshPacket,
        to_irc: &mpsc::Sender<String>,
    ) -> Result<()> {
        debug!("Processing MeshPacket from node {:08x}", packet.from);
        
        // Check if this is a Data packet with decoded payload
        if let Some(payload_variant) = &packet.payload_variant {
            match payload_variant {
                mesh_packet::PayloadVariant::Decoded(data) => {
                    // Only process text messages
                    if data.portnum() == PortNum::TextMessageApp {
                        if data.payload.len() > 0 {
                            if let Ok(text) = std::str::from_utf8(&data.payload) {
                                // Don't forward our own messages back to IRC
                                if !text.starts_with("[IRC-") {
                                    // Use short name if available, otherwise use ID
                                    let sender = self.node_names.get(&packet.from)
                                        .cloned()
                                        .unwrap_or_else(|| format!("{:08x}", packet.from));
                                    let message = format!("[mesh-{}]: {}", sender, text);
                                    
                                    info!("Received Meshtastic message via MQTT: {}", message);
                                    to_irc.send(message).await?;
                                    debug!("Forwarded Meshtastic message to IRC");
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
}