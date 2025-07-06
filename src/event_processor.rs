// src/network_event_processor.rs
use chrono::{DateTime, Utc};
use log::{info, error};
use std::collections::VecDeque;
use tokio::time::Duration;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct NetworkEvent {
    pub timestamp: DateTime<Utc>,
    pub source_ip: String,
    pub dest_ip: String,
    pub protocol: String,
    pub summary: String, // e.g., "TCP SYN from 192.168.1.100:12345 to 8.8.8.8:53"
    // Add more fields as you parse them (ports, flags, etc.)
}

pub struct NetworkEventProcessor {
    buffer: VecDeque<NetworkEvent>,
    max_buffer_size: usize,
    pub sender: tokio::sync::mpsc::Sender<NetworkEvent>,
    receiver: tokio::sync::mpsc::Receiver<NetworkEvent>,
}

impl NetworkEventProcessor {
    pub fn new(max_buffer_size: usize, channel_capacity: usize) -> Self {
        let (sender, receiver) = tokio::sync::mpsc::channel(channel_capacity);
        NetworkEventProcessor {
            buffer: VecDeque::with_capacity(max_buffer_size),
            max_buffer_size,
            sender,
            receiver,
        }
    }

    // This would run in its own async task
    pub async fn run(&mut self, mut receiver: tokio::sync::mpsc::Receiver<NetworkEvent>, ollama_client_sender: tokio::sync::mpsc::Sender<Vec<NetworkEvent>>) {
        let mut interval = tokio::time::interval(Duration::from_secs(30)); // Process every 30 seconds
        interval.tick().await; // Initial tick to avoid immediate processing

        loop {
            tokio::select! {
                Some(event) = receiver.recv() => {
                    self.buffer.push_back(event);
                    if self.buffer.len() >= self.max_buffer_size {
                        info!("Buffer full, triggering Ollama processing.");
                        self.process_buffer_and_send_to_ollama(ollama_client_sender.clone()).await;
                    }
                },
                _ = interval.tick() => {
                    if !self.buffer.is_empty() {
                        info!("Time interval passed, triggering Ollama processing.");
                        self.process_buffer_and_send_to_ollama(ollama_client_sender.clone()).await;
                    }
                },
            }
        }
    }

    async fn process_buffer_and_send_to_ollama(&mut self, sender: tokio::sync::mpsc::Sender<Vec<NetworkEvent>>) {
        let events: Vec<NetworkEvent> = self.buffer.drain(..).collect();
        if !events.is_empty() {
            if let Err(e) = sender.send(events).await {
                error!("Failed to send events to Ollama client: {}", e);
            }
        }
    }
}