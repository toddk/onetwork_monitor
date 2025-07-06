// src/packet_capture.rs
use pcap::{Capture, Device};
use log::{info, error};

use tokio::sync::mpsc;
use crate::event_processor::NetworkEvent;

pub async fn start_capture(interface_name: &str, sender: mpsc::Sender<NetworkEvent>) -> Result<(), Box<dyn std::error::Error>> {
    info!("Starting packet capture on interface: {}", interface_name);

    let device = Device::list()?
        .into_iter()
        .find(|d| d.name == interface_name)
        .ok_or(format!("Device {} not found", interface_name))?;

    let mut cap = Capture::from_device(device)?
        .snaplen(65535) // capture full packets
        .promisc(true)  // promiscuous mode
        .timeout(1000)  // read timeout
        .open()?;

    info!("Capture opened. Press Ctrl+C to stop.");

    while let Ok(packet) = cap.next_packet() {
        let event = NetworkEvent {
            timestamp: chrono::Utc::now(),
            source_ip: "N/A".to_string(),
            dest_ip: "N/A".to_string(),
            protocol: "N/A".to_string(),
            summary: format!("Packet received: len={}, caplen={}", packet.header.len, packet.header.caplen),
        };
        if let Err(e) = sender.send(event).await {
            error!("Failed to send packet event: {}", e);
            break;
        }
    }

    Ok(())
}