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

    // MPSC channel for sending requests to the event processor and receiving responses
    let (processor_request_sender, processor_request_receiver) = mpsc::channel::<(String, mpsc::Sender<Vec<NetworkEvent>>)>(1);

    // Packet Capture Task
    // This will run in a separate async task and send events to packet_event_sender
    tokio::spawn(async move {
        if let Err(e) = packet_capture::start_capture(&args.interface, packet_event_sender).await {
            error!("Packet capture error: {}", e);
        }
    });

    // Network Event Processor Task
    let mut event_processor = NetworkEventProcessor::new(args.buffer_size, args.channel_capacity);
    tokio::spawn(async move {
        event_processor.run(packet_event_receiver, processor_request_receiver).await;
    });

    // Ollama Client
    let ollama_client = ollama_client::OllamaClient::new(args.ollama_url.clone(), args.model.clone());
    if let Err(e) = ollama_client.check_connection().await {
        error!("Ollama connection check failed: {}", e);
        return Ok(()); // Exit if we can't connect
    }

    info!("Network monitoring started. Ask me a question about the network traffic.");
    info!("Type 'exit' or 'quit' to stop.");

    use std::io::{self, Write};

    let stdin = tokio::io::stdin();
    let mut reader = tokio::io::BufReader::new(stdin);
    let mut line = String::new();

    loop {
        line.clear();
        print!(">> ");
        io::stdout().flush()?;
        tokio::io::AsyncBufReadExt::read_line(&mut reader, &mut line).await?;
        let input = line.trim();

        if input.eq_ignore_ascii_case("exit") || input.eq_ignore_ascii_case("quit") || input.eq_ignore_ascii_case("/q") {
            break;
        }

        if input.is_empty() {
            continue;
        }

        info!("You asked: {}", input);
        info!("Processing your question...");

        // Channel for receiving events from the processor for this specific query
        let (query_events_sender, mut query_events_receiver) = mpsc::channel::<Vec<NetworkEvent>>(1);

        // Send a request to the event processor to get current events
        if let Err(e) = processor_request_sender.send((input.to_string(), query_events_sender)).await {
            error!("Failed to send request to event processor: {}", e);
            continue;
        }

        // Wait for the event processor to send back the events
        let events_batch = match query_events_receiver.recv().await {
            Some(events) => events,
            None => {
                error!("Event processor disconnected or sent no events.");
                continue;
            }
        };

        if events_batch.is_empty() {
            info!("No network events captured yet to analyze.");
            continue;
        }

        info!("Preparing prompt for Ollama with {} events.", events_batch.len());

        let mut prompt = format!("User Question: {}\n\nAnalyze the following network events to answer the user's question. Provide a concise and direct answer based on the data. If the data doesn't directly support the answer, state that. \n\nNetwork Events:\n", input);
        for event in events_batch {
            prompt.push_str(&format!("Timestamp: {}, Source: {}, Dest: {}, Protocol: {}, Summary: {}\n",
                                     event.timestamp, event.source_ip, event.dest_ip, event.protocol, event.summary));
        }

        match ollama_client.generate(&prompt).await {
            Ok(response) => {
                println!("\nOllama Response:\n{}", response);
            },
            Err(e) => {
                error!("Error generating Ollama response: {}", e);
            }
        }
    }

    info!("Shutting down...");
    info!("Ctrl+C received, shutting down...");

    Ok(())
}