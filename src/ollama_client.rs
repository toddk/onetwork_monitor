// src/ollama_client.rs
use serde::{Serialize, Deserialize};
use reqwest;
use log::{info, error};

// Define the Ollama request and response structures
#[derive(Serialize, Debug)]
pub struct OllamaGenerateRequest {
    pub model: String,
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    // Add other parameters if needed, e.g., options, system
}

#[derive(Deserialize, Debug)]
pub struct OllamaGenerateResponse {
    pub response: String,
    // Other fields like model, created_at, done, etc.
}

pub struct OllamaClient {
    base_url: String,
    model: String,
}

impl OllamaClient {
    pub fn new(base_url: String, model: String) -> Self {
        OllamaClient { base_url, model }
    }

    pub async fn check_connection(&self) -> Result<(), Box<dyn std::error::Error>> {
        let client = reqwest::Client::new();
        let url = format!("{}/api/tags", self.base_url);
        let res = client.get(&url).send().await?;
        if res.status().is_success() {
            info!("Successfully connected to Ollama at {}", self.base_url);
            Ok(())
        } else {
            let status = res.status();
            let text = res.text().await?;
            error!("Failed to connect to Ollama at {}. Status: {}. Response: {}", self.base_url, status, text);
            Err(format!("Ollama API error: {} - {}", status, text).into())
        }
    }

    pub async fn generate(&self, prompt: &str) -> Result<String, Box<dyn std::error::Error>> {
        let client = reqwest::Client::new();
        let url = format!("{}/api/generate", self.base_url);

        let request_body = OllamaGenerateRequest {
            model: self.model.clone(),
            prompt: prompt.to_string(),
            stream: Some(false), // We want the full response at once for analysis
        };

        info!("Sending prompt to Ollama model '{}'", self.model);
        let res = client.post(&url)
            .json(&request_body)
            .send()
            .await?;

        if !res.status().is_success() {
            let status = res.status();
            let text = res.text().await?;
            error!("Ollama API request failed with status: {}. Response: {}", status, text);
            return Err(format!("Ollama API error: {} - {}", status, text).into());
        }

        let response: OllamaGenerateResponse = res.json().await?;
        Ok(response.response)
    }
}