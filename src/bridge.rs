use anyhow::Result;
use log::{error, info};
use tokio::sync::mpsc;

use crate::config::Config;
use crate::irc_handler::{IrcHandler, IrcMessage};
use crate::meshtastic_handler::MeshtasticHandler;
use crate::mqtt_handler::MqttHandler;

pub struct Bridge {
    config: Config,
}

impl Bridge {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub async fn run(self) -> Result<()> {
        info!("Starting bridge...");

        // Create message channels
        let (irc_to_mesh_tx, irc_to_mesh_rx) = mpsc::channel::<IrcMessage>(100);
        let (mesh_to_irc_tx, mesh_to_irc_rx) = mpsc::channel::<String>(100);

        // Start both handlers in parallel
        let irc_config = self.config.irc.clone();
        let meshtastic_config = self.config.meshtastic.clone();

        // Spawn IRC handler initialization
        let irc_handle = tokio::spawn(async move {
            info!("Initializing IRC connection...");
            match IrcHandler::new(&irc_config).await {
                Ok(handler) => {
                    info!("IRC handler initialized successfully");
                    info!("Starting IRC message handler loop");
                    if let Err(e) = handler.run(mesh_to_irc_rx, irc_to_mesh_tx).await {
                        error!("IRC handler error: {}", e);
                    }
                }
                Err(e) => {
                    error!("Failed to initialize IRC handler: {}", e);
                }
            }
        });

        // Spawn Meshtastic handler initialization (either serial or MQTT)
        let mesh_handle = if let Some(mqtt_config) = &meshtastic_config.mqtt {
            let mqtt_config = mqtt_config.clone();
            let channel = meshtastic_config.channel;
            tokio::spawn(async move {
                info!("Initializing MQTT connection...");
                match MqttHandler::new(&mqtt_config, channel).await {
                    Ok(handler) => {
                        info!("MQTT handler initialized successfully");
                        info!("Starting MQTT message handler loop");
                        if let Err(e) = handler.run(irc_to_mesh_rx, mesh_to_irc_tx).await {
                            error!("MQTT handler error: {}", e);
                        }
                    }
                    Err(e) => {
                        error!("Failed to initialize MQTT handler: {}", e);
                    }
                }
            })
        } else {
            tokio::spawn(async move {
                info!("Initializing Meshtastic serial connection...");
                match MeshtasticHandler::new(&meshtastic_config).await {
                    Ok(handler) => {
                        info!("Meshtastic handler initialized successfully");
                        info!("Starting Meshtastic message handler loop");
                        if let Err(e) = handler.run(irc_to_mesh_rx, mesh_to_irc_tx).await {
                            error!("Meshtastic handler error: {}", e);
                        }
                    }
                    Err(e) => {
                        error!("Failed to initialize Meshtastic handler: {}", e);
                    }
                }
            })
        };

        info!("Bridge is running! Waiting for both connections to establish...");

        // Wait for tasks to complete
        tokio::select! {
            _ = irc_handle => {
                error!("IRC handler terminated");
            }
            _ = mesh_handle => {
                error!("Meshtastic handler terminated");
            }
        }

        Err(anyhow::anyhow!("Bridge terminated unexpectedly"))
    }
}