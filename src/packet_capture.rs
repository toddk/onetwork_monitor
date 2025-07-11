// src/packet_capture.rs
use pcap::{Capture, Device};
use log::{info, error};
use tokio::sync::mpsc;
use crate::event_processor::NetworkEvent;
use etherparse::{SlicedPacket, InternetSlice, TransportSlice};

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
        let (source_ip, dest_ip, protocol, summary) = match SlicedPacket::from_ethernet(&packet.data) {
            Ok(sliced_packet) => {
                let mut current_summary = String::new();
                let mut src_ip = "N/A".to_string();
                let mut dst_ip = "N/A".to_string();
                let mut proto = "UNKNOWN".to_string();

                if let Some(net) = sliced_packet.net {
                    match net {
                        InternetSlice::Ipv4(header) => {
                            src_ip = std::net::Ipv4Addr::from(header.header().source()).to_string();
                            dst_ip = std::net::Ipv4Addr::from(header.header().destination()).to_string();
                            proto = match header.header().protocol().0 {
                                6 => "TCP".to_string(),
                                17 => "UDP".to_string(),
                                1 => "ICMP".to_string(),
                                _ => format!("IP_PROTO:{:?}", header.header().protocol()),
                            };
                            current_summary.push_str(&format!("IPv4 {}:{} -> {}:{}", src_ip, "N/A", dst_ip, "N/A"));
                        },
                        InternetSlice::Ipv6(header) => {
                            src_ip = std::net::Ipv6Addr::from(header.header().source()).to_string();
                            dst_ip = std::net::Ipv6Addr::from(header.header().destination()).to_string();
                            proto = match header.header().next_header().0 {
                                6 => "TCP".to_string(),
                                17 => "UDP".to_string(),
                                58 => "ICMPv6".to_string(),
                                _ => format!("IPv6_NEXT_HDR:{:?}", header.header().next_header()),
                            };
                            current_summary.push_str(&format!("IPv6 {}:{} -> {}:{}", src_ip, "N/A", dst_ip, "N/A"));
                        },
                    }
                }

                if let Some(transport) = sliced_packet.transport {
                    match transport {
                        TransportSlice::Tcp(header) => {
                            if proto == "UNKNOWN" { proto = "TCP".to_string(); }
                            current_summary.push_str(&format!(" Ports: {}->{}", header.source_port(), header.destination_port()));
                        },
                        TransportSlice::Udp(header) => {
                            if proto == "UNKNOWN" { proto = "UDP".to_string(); }
                            current_summary.push_str(&format!(" Ports: {}->{}", header.source_port(), header.destination_port()));
                        },
                        _ => {},
                    }
                }
                (src_ip, dst_ip, proto, current_summary)
            },
            Err(value) => {
                error!("Failed to slice packet: {:?}", value);
                ("N/A".to_string(), "N/A".to_string(), "UNKNOWN".to_string(), format!("Raw packet len={}", packet.data.len()))
            }
        };

        let event = NetworkEvent {
            timestamp: chrono::Utc::now(),
            source_ip: source_ip,
            dest_ip: dest_ip,
            protocol: protocol,
            summary: summary,
        };
        if let Err(e) = sender.send(event).await {
            error!("Failed to send packet event: {}", e);
            break;
        }
    }

    Ok(())
}