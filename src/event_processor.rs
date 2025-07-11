// src/network_event_processor.rs
use chrono::{DateTime, Utc};
use log::{info, error};
use std::collections::VecDeque;


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
}

impl NetworkEventProcessor {
    pub fn new(max_buffer_size: usize, _channel_capacity: usize) -> Self {
        NetworkEventProcessor {
            buffer: VecDeque::with_capacity(max_buffer_size),
            max_buffer_size,
        }
    }

    pub async fn run(&mut self, mut packet_event_receiver: tokio::sync::mpsc::Receiver<NetworkEvent>, mut processor_request_receiver: tokio::sync::mpsc::Receiver<(String, tokio::sync::mpsc::Sender<Vec<NetworkEvent>>)>) {
        loop {
            tokio::select! {
                Some(event) = packet_event_receiver.recv() => {
                    self.buffer.push_back(event);
                    // Optionally, trim buffer if it exceeds max_buffer_size
                    if self.buffer.len() > self.max_buffer_size {
                        self.buffer.pop_front();
                    }
                },
                Some((_query, response_sender)) = processor_request_receiver.recv() => {
                    info!("Received request for network events. Sending {} events.", self.buffer.len());
                    let events: Vec<NetworkEvent> = self.buffer.drain(..).collect();
                    if let Err(e) = response_sender.send(events).await {
                        error!("Failed to send events back to main: {}", e);
                    }
                },
            }
        }
    }
}