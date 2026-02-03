mod client;

use api_types::{CreateAgentRequest, GameApi};
use clap::{Parser, Subcommand};
use client::{ApiClient, CliError};
use common::{AgentId, UserId};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "achtung", about = "Achtung platform CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage agents
    Agent {
        #[command(subcommand)]
        command: AgentCommands,
    },
    /// Manage registry images
    Registry {
        #[command(subcommand)]
        command: RegistryCommands,
    },
}

#[derive(Subcommand)]
enum AgentCommands {
    /// List your agents
    List,
    /// Create a new agent
    Create {
        /// Agent name (3-50 chars, alphanumeric/hyphens/underscores)
        #[arg(long)]
        name: String,
        /// Image name (from your registry namespace)
        #[arg(long)]
        image: String,
    },
    /// Activate an agent
    Activate {
        /// Agent ID
        id: AgentId,
    },
    /// Deactivate an agent
    Deactivate {
        /// Agent ID
        id: AgentId,
    },
    /// Delete an agent
    Delete {
        /// Agent ID
        id: AgentId,
    },
}

#[derive(Subcommand)]
enum RegistryCommands {
    /// List your registry images
    Images,
}

#[derive(Deserialize, Default)]
struct Config {
    api_url: Option<String>,
    user_id: Option<UserId>,
    api_token: Option<String>,
}

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("achtung")
        .join("config.toml")
}

fn load_config() -> Config {
    let path = config_path();
    match std::fs::read_to_string(&path) {
        Ok(contents) => toml::from_str(&contents).unwrap_or_else(|e| {
            eprintln!("Warning: failed to parse {}: {}", path.display(), e);
            Config::default()
        }),
        Err(_) => Config::default(),
    }
}

fn build_client() -> Result<ApiClient, CliError> {
    let config = load_config();

    let api_url = std::env::var("ACHTUNG_API_URL")
        .ok()
        .or(config.api_url)
        .ok_or_else(|| {
            CliError::Config(format!(
                "api_url not set. Set ACHTUNG_API_URL or add api_url to {}",
                config_path().display()
            ))
        })?;

    let user_id = std::env::var("ACHTUNG_USER_ID")
        .ok()
        .and_then(|s| s.parse().ok())
        .or(config.user_id)
        .ok_or_else(|| {
            CliError::Config(format!(
                "user_id not set. Set ACHTUNG_USER_ID or add user_id to {}",
                config_path().display()
            ))
        })?;

    let api_token = std::env::var("ACHTUNG_API_TOKEN")
        .ok()
        .or(config.api_token)
        .ok_or_else(|| {
            CliError::Config(format!(
                "api_token not set. Set ACHTUNG_API_TOKEN or add api_token to {}",
                config_path().display()
            ))
        })?;

    Ok(ApiClient::new(api_url, user_id, api_token))
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if let Err(e) = run(cli).await {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<(), CliError> {
    let client = build_client()?;

    match cli.command {
        Commands::Agent { command } => match command {
            AgentCommands::List => {
                let agents = client.list_agents().await?;
                println!("{}", serde_json::to_string_pretty(&agents).unwrap());
            }
            AgentCommands::Create { name, image } => {
                let agent = client
                    .create_agent(CreateAgentRequest { name, image })
                    .await?;
                println!("{}", serde_json::to_string_pretty(&agent).unwrap());
            }
            AgentCommands::Activate { id } => {
                let agent = client.activate_agent(id).await?;
                println!("{}", serde_json::to_string_pretty(&agent).unwrap());
            }
            AgentCommands::Deactivate { id } => {
                let agent = client.deactivate_agent(id).await?;
                println!("{}", serde_json::to_string_pretty(&agent).unwrap());
            }
            AgentCommands::Delete { id } => {
                client.delete_agent(id).await?;
                println!("Agent {} deleted.", id);
            }
        },
        Commands::Registry { command } => match command {
            RegistryCommands::Images => {
                let images = client.list_images().await?;
                println!("{}", serde_json::to_string_pretty(&images).unwrap());
            }
        },
    }

    Ok(())
}
