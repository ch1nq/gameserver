mod client;

use api_types::{CreateAgentRequest, GameApi};
use clap::{Parser, Subcommand};
use client::{ApiClient, ApiError, CliError};
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

/// Raw config file format (all fields optional)
#[derive(Deserialize)]
struct ConfigFile {
    api_url: Option<String>,
    user_id: Option<UserId>,
    api_token: Option<String>,
}

/// Resolved runtime configuration (all fields required)
struct Config {
    api_url: String,
    user_id: UserId,
    api_token: String,
}

fn load_config() -> Result<Config, CliError> {
    let path = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("achtung")
        .join("config.toml");

    let config_file: ConfigFile = match std::fs::read_to_string(&path) {
        Ok(contents) => toml::from_str(&contents)
            .map_err(|e| CliError::Config(format!("failed to parse {}: {}", path.display(), e)))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => ConfigFile {
            api_url: None,
            user_id: None,
            api_token: None,
        },
        Err(e) => {
            return Err(CliError::Config(format!(
                "failed to read {}: {}",
                path.display(),
                e
            )));
        }
    };

    // Apply env var overrides
    let api_url = std::env::var("ACHTUNG_API_URL")
        .ok()
        .or(config_file.api_url)
        .ok_or_else(|| {
            CliError::Config(format!(
                "api_url not set. Set ACHTUNG_API_URL or add api_url to {}",
                path.display()
            ))
        })?;
    let user_id = std::env::var("ACHTUNG_USER_ID")
        .ok()
        .and_then(|s| s.parse().ok())
        .or(config_file.user_id)
        .ok_or_else(|| {
            CliError::Config(format!(
                "user_id not set. Set ACHTUNG_USER_ID or add user_id to {}",
                path.display()
            ))
        })?;
    let api_token = std::env::var("ACHTUNG_API_TOKEN")
        .ok()
        .or(config_file.api_token)
        .ok_or_else(|| {
            CliError::Config(format!(
                "api_token not set. Set ACHTUNG_API_TOKEN or add api_token to {}",
                path.display()
            ))
        })?;

    Ok(Config {
        api_url,
        user_id,
        api_token,
    })
}

fn build_client(config: &Config) -> Result<ApiClient, CliError> {
    Ok(ApiClient::new(
        config.api_url.clone(),
        config.user_id,
        config.api_token.clone(),
    ))
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
    let config = load_config()?;
    let client = build_client(&config)?;

    match cli.command {
        Commands::Agent { command } => match command {
            AgentCommands::List => {
                let agents = client.list_agents().await?;
                println!("{}", serde_json::to_string_pretty(&agents).unwrap());
            }
            AgentCommands::Create { name, image } => {
                match client.validate_image(&image).await {
                    Ok(_) => {
                        // Image is validated, proceed with creation
                        let agent = client
                            .create_agent(CreateAgentRequest { name, image })
                            .await?;
                        println!("{}", serde_json::to_string_pretty(&agent).unwrap());
                    }
                    Err(e) => {
                        // Build enhanced error message with helpful context
                        let mut error_msg = format!("{}", e);
                        error_msg.push_str("\n\n");

                        // Try to list available images
                        if let Ok(available_images) = client.list_images().await
                            && !available_images.is_empty()
                        {
                            error_msg.push_str("Available images:\n");
                            for img in &available_images {
                                error_msg.push_str(&format!("  - {}\n", img.repository_name()));
                            }
                            error_msg.push('\n');
                        }

                        // Add helpful tip about pushing images
                        let image_base = image.split(':').next().unwrap_or(&image);
                        let image_with_namespace =
                            format!("user-{}/{}", config.user_id, image_base);
                        error_msg.push_str("Tip: Push your image to the registry first:\n");
                        error_msg.push_str(&format!(
                            "  docker tag your-image:tag achtung-registry.fly.dev/{}\n",
                            image_with_namespace
                        ));
                        error_msg.push_str(&format!(
                            "  docker push achtung-registry.fly.dev/{}",
                            image_with_namespace
                        ));

                        return Err(CliError::Api(ApiError::Validation(error_msg)));
                    }
                }
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
