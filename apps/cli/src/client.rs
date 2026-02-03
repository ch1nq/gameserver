pub use api_types::ApiError;
pub use api_types::client::HttpClient as ApiClient;

#[derive(Debug)]
pub enum CliError {
    Api(ApiError),
    Config(String),
}

impl From<ApiError> for CliError {
    fn from(e: ApiError) -> Self {
        CliError::Api(e)
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliError::Api(e) => write!(f, "API error: {}", e),
            CliError::Config(e) => write!(f, "Config error: {}", e),
        }
    }
}
