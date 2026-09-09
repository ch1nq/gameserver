pub use api_types::ApiError;
pub use api_types::client::HttpClient as ApiClient;

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("API error: {0}")]
    Api(#[from] ApiError),
    #[error("Config error: {0}")]
    Config(String),
}
