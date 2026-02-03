use common::{AgentId, ApiTokenId, UserId};
use reqwest::Client;
use serde::de::DeserializeOwned;

use crate::{
    Agent, ApiError, ApiToken, CreateAgentRequest, CreateTokenRequest, CreateTokenResponse, GameApi,
    routes,
};

pub struct HttpClient {
    client: Client,
    base_url: String,
    user_id: UserId,
    api_token: String,
}

impl HttpClient {
    pub fn new(base_url: String, user_id: UserId, api_token: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
            user_id,
            api_token,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}/api/v1{}", self.base_url, path)
    }

    fn auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        req.basic_auth(format!("user-{}", self.user_id), Some(&self.api_token))
    }

    async fn parse_response<T: DeserializeOwned>(
        &self,
        response: reqwest::Response,
    ) -> Result<T, ApiError> {
        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;

        if !status.is_success() {
            return Err(Self::parse_error(status, &text));
        }

        serde_json::from_str(&text)
            .map_err(|e| ApiError::Internal(format!("Failed to parse response: {}", e)))
    }

    async fn parse_empty_response(&self, response: reqwest::Response) -> Result<(), ApiError> {
        let status = response.status();

        if !status.is_success() {
            let text = response
                .text()
                .await
                .map_err(|e| ApiError::Internal(e.to_string()))?;
            return Err(Self::parse_error(status, &text));
        }

        Ok(())
    }

    fn parse_error(status: reqwest::StatusCode, body: &str) -> ApiError {
        // Try to parse the structured error from the server
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
            if let Some(msg) = value["error"].as_str() {
                return match status.as_u16() {
                    401 => ApiError::Unauthorized,
                    404 => ApiError::NotFound,
                    422 => ApiError::Validation(msg.to_string()),
                    _ => ApiError::Internal(msg.to_string()),
                };
            }
        }
        ApiError::Internal(format!("HTTP {}: {}", status, body))
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, ApiError> {
        let response = self
            .auth(self.client.get(self.url(path)))
            .send()
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        self.parse_response(response).await
    }

    async fn post<B: serde::Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, ApiError> {
        let response = self
            .auth(self.client.post(self.url(path)))
            .json(body)
            .send()
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        self.parse_response(response).await
    }

    async fn delete_request(&self, path: &str) -> Result<(), ApiError> {
        let response = self
            .auth(self.client.delete(self.url(path)))
            .send()
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        self.parse_empty_response(response).await
    }
}

impl GameApi for HttpClient {
    async fn list_agents(&self) -> Result<Vec<Agent>, ApiError> {
        self.get(&routes::agents_path()).await
    }

    async fn create_agent(&self, req: CreateAgentRequest) -> Result<Agent, ApiError> {
        self.post(&routes::agents_path(), &req).await
    }

    async fn activate_agent(&self, id: AgentId) -> Result<Agent, ApiError> {
        let response = self
            .auth(self.client.post(self.url(&routes::agent_activate_path(id))))
            .send()
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        self.parse_response(response).await
    }

    async fn deactivate_agent(&self, id: AgentId) -> Result<Agent, ApiError> {
        let response = self
            .auth(self.client.post(self.url(&routes::agent_deactivate_path(id))))
            .send()
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        self.parse_response(response).await
    }

    async fn delete_agent(&self, id: AgentId) -> Result<(), ApiError> {
        self.delete_request(&routes::agent_path(id)).await
    }

    async fn list_images(&self) -> Result<Vec<String>, ApiError> {
        self.get(&routes::images_path()).await
    }

    async fn list_tokens(&self) -> Result<Vec<ApiToken>, ApiError> {
        self.get(&routes::tokens_path()).await
    }

    async fn create_token(&self, req: CreateTokenRequest) -> Result<CreateTokenResponse, ApiError> {
        self.post(&routes::tokens_path(), &req).await
    }

    async fn revoke_token(&self, id: ApiTokenId) -> Result<(), ApiError> {
        self.delete_request(&routes::token_path(id)).await
    }
}
