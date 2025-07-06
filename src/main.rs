// src/main.rs
mod packet_capture;
mod event_processor;
mod ollama_client;

use clap::Parser;
use log::{info, error};
use tokio::sync::mpsc;
use event_processor::{NetworkEvent, NetworkEventProcessor};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Network interface to monitor (e.g., eth0, wlan0, en0)
    #[arg(short, long)]
    interface: String,

    /// URL for the Ollama API (e.g., http://localhost:11434)
    #[arg(short, long, default_value = "http://localhost:11434")]
    ollama_url: String,

    /// Ollama model to use (e.g., llama2, mistral)
    #[arg(short, long, default_value = "llama2")]
    model: String,

    /// Max number of events to buffer before sending to Ollama
    #[arg(short, long, default_value_t = 20)]
    buffer_size: usize,

    /// Capacity for internal communication channels
    #[arg(long, default_value_t = 100)]
    channel_capacity: usize,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init(); // Initialize logging

    let args = Args::parse();

    info!("Starting network monitor with settings: {:?}", args);

    // MPSC channel for sending parsed events from capture to processor
    let (packet_event_sender, packet_event_receiver) = mpsc::channel::<NetworkEvent>(args.channel_capacity);

    // MPSC channel for sending buffered events from processor to Ollama client
    let (ollama_prompt_sender, mut ollama_prompt_receiver) = mpsc::channel::<Vec<NetworkEvent>>(args.channel_capacity);

    // Packet Capture Task
    // This will run in a separate async task and send events to packet_event_sender
    tokio::spawn(async move {
        if let Err(e) = packet_capture::start_capture(&args.interface, packet_event_sender).await {
            error!("Packet capture error: {}", e);
        }
    });

    // Network Event Processor Task
    let mut event_processor = NetworkEventProcessor::new(args.buffer_size, args.channel_capacity);
    let processor_ollama_sender = ollama_prompt_sender.clone(); // Clone for sending
    tokio::spawn(async move {
        event_processor.run(packet_event_receiver, processor_ollama_sender).await;
    });

    // Ollama Client Task
    let ollama_client = ollama_client::OllamaClient::new(args.ollama_url.clone(), args.model.clone());
    if let Err(e) = ollama_client.check_connection().await {
        error!("Ollama connection check failed: {}", e);
        return Ok(()); // Exit if we can't connect
    }

    tokio::spawn(async move {
        while let Some(events_batch) = ollama_prompt_receiver.recv().await {
            info!("Received {} events for Ollama processing.", events_batch.len());

            let mut prompt = String::from("Analyze the following network events for anomalies, unusual patterns, or interesting insights. Focus on potential security concerns, performance issues, or unusual communication flows. Provide a concise summary and highlight any anomalies.\n\n");
            for event in events_batch {
                prompt.push_str(&format!("Timestamp: {}, Source: {}, Dest: {}, Protocol: {}, Summary: {}\n",
                                         event.timestamp, event.source_ip, event.dest_ip, event.protocol, event.summary));
            }

            match ollama_client.generate(&prompt).await {
                Ok(response) => {
                    info!("Ollama Analysis:\n{}", response);
                    // Here you would implement logic to store, alert, or display the analysis
                },
                Err(e) => {
                    error!("Error generating Ollama response: {}", e);
                }
            }
        }
    });

    // Keep the main thread alive, allowing Tokio tasks to run
    // This is a simple way; for a more robust application, you'd handle shutdown signals.
    tokio::signal::ctrl_c().await?;
    info!("Ctrl+C received, shutting down...");

    Ok(())
}