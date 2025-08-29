use anyhow::Result;
use irc::client::prelude::*;
use log::{debug, error, info};
use tokio::sync::mpsc;
use futures_util::StreamExt;

use crate::config::IrcConfig;

pub struct IrcHandler {
    client: Client,
    channel: String,
}

#[derive(Debug, Clone)]
pub struct IrcMessage {
    pub sender: String,
    pub content: String,
}

impl IrcHandler {
    pub async fn new(config: &IrcConfig) -> Result<Self> {
        let irc_config = Config {
            nickname: Some(config.nickname.clone()),
            username: config.username.clone(),
            realname: config.realname.clone(),
            server: Some(config.server.clone()),
            port: Some(config.port),
            channels: vec![config.channel.clone()],
            password: config.password.clone(),
            use_tls: Some(config.use_tls),
            ..Config::default()
        };

        info!("Connecting to IRC server {}:{} with TLS={}", 
              config.server, config.port, config.use_tls);
              
        let client = Client::from_config(irc_config).await?;
        client.identify()?;

        info!("Connected to IRC server: {}:{}", config.server, config.port);
        info!("Joining channel: {}", config.channel);

        Ok(Self {
            client,
            channel: config.channel.clone(),
        })
    }

    pub async fn run(
        mut self,
        mut from_meshtastic: mpsc::Receiver<String>,
        to_meshtastic: mpsc::Sender<IrcMessage>,
    ) -> Result<()> {
        let mut stream = self.client.stream()?;
        info!("IRC handler run loop started");

        loop {
            tokio::select! {
                result = stream.next() => {
                    if let Some(Ok(message)) = result {
                        debug!("Received IRC message: {:?}", message);
                        if let Err(e) = self.handle_irc_message(message, &to_meshtastic).await {
                            error!("Error handling IRC message: {}", e);
                        }
                    } else if result.is_none() {
                        error!("IRC stream ended");
                        break;
                    }
                }
                Some(message) = from_meshtastic.recv() => {
                    info!("Received message from Meshtastic to send to IRC: {}", message);
                    if let Err(e) = self.send_to_irc(&message).await {
                        error!("Error sending to IRC: {}", e);
                    }
                }
                else => {
                    debug!("No messages in either channel");
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            }
        }
        
        error!("IRC handler run loop ended");
        Ok(())
    }

    async fn handle_irc_message(
        &self,
        message: Message,
        to_meshtastic: &mpsc::Sender<IrcMessage>,
    ) -> Result<()> {
        match message.command {
            Command::PRIVMSG(target, content) => {
                if target == self.channel {
                    if let Some(Prefix::Nickname(nick, _, _)) = message.prefix {
                        // Ignore our own messages to prevent loops
                        if nick == self.client.current_nickname() {
                            debug!("Ignoring own message");
                            return Ok(());
                        }
                        
                        info!("IRC message from {}: {}", nick, content);
                        
                        let irc_msg = IrcMessage {
                            sender: nick,
                            content,
                        };
                        
                        match to_meshtastic.send(irc_msg).await {
                            Ok(_) => {
                                info!("Successfully sent IRC message to Meshtastic channel");
                            }
                            Err(e) => {
                                error!("Failed to send to Meshtastic channel: {}", e);
                                return Err(e.into());
                            }
                        }
                    }
                }
            }
            Command::Response(Response::RPL_ENDOFMOTD, _) |
            Command::Response(Response::ERR_NOMOTD, _) => {
                info!("IRC connection ready - fully connected to {}", self.channel);
            }
            Command::PING(server1, server2) => {
                // Respond to PING to keep connection alive
                debug!("Received PING, sending PONG");
                self.client.send_pong(&server1)?;
                if let Some(server2) = server2 {
                    self.client.send_pong(&server2)?;
                }
            }
            Command::JOIN(channel, _, _) => {
                if let Some(Prefix::Nickname(nick, _, _)) = message.prefix {
                    if nick == self.client.current_nickname() {
                        info!("Successfully joined {}", channel);
                    }
                }
            }
            Command::NOTICE(target, content) => {
                debug!("Notice to {}: {}", target, content);
                // Don't forward notices
            }
            _ => {
                // Ignore other message types
                debug!("Ignoring IRC command: {:?}", message.command);
            }
        }
        
        Ok(())
    }

    async fn send_to_irc(&self, message: &str) -> Result<()> {
        info!("Sending to IRC channel {}: {}", self.channel, message);
        self.client.send_privmsg(&self.channel, message)?;
        info!("Successfully sent to IRC");
        Ok(())
    }
}