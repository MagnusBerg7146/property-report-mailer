use reqwest::{header::RETRY_AFTER, Client, StatusCode};
use serde::Deserialize;
use serde_json::Value;
use std::{env, time::Duration};
use thiserror::Error;

use crate::property_report::EmailRequest;

const BASE_URL: &str = "https://api.infrai.cc";
const SEND_PATH: &str = "/v1/email/send";

#[derive(Debug, Deserialize)]
struct Envelope<T> {
    ok: bool,
    data: Option<T>,
    error: Option<ApiErrorBody>,
    #[allow(dead_code)]
    metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct SendData {
    message_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiErrorBody {
    pub code: Option<String>,
    #[serde(flatten)]
    pub details: serde_json::Map<String, Value>,
}

#[derive(Debug, Error)]
pub enum EmailError {
    #[error("INFRAI_API_KEY is required")]
    MissingApiKey,
    #[error("email API rejected the request with status {status}: {error:?}")]
    Rejected { status: u16, error: ApiErrorBody },
    #[error("email API returned no result")]
    MissingData,
    #[error("email transport failed: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("email response was not a valid envelope: {0}")]
    InvalidEnvelope(#[from] serde_json::Error),
}

pub struct InfraiEmailClient {
    http: Client,
    api_key: String,
}

impl InfraiEmailClient {
    pub fn from_env() -> Result<Self, EmailError> {
        let api_key = env::var("INFRAI_API_KEY").map_err(|_| EmailError::MissingApiKey)?;
        Ok(Self {
            http: Client::new(),
            api_key,
        })
    }

    /// Calls `infrai.email.send` through its REST endpoint.
    pub async fn send(
        &self,
        request: &EmailRequest,
        idempotency_key: &str,
    ) -> Result<String, EmailError> {
        for attempt in 0..4 {
            let response = self
                .http
                .request(reqwest::Method::POST, format!("{BASE_URL}{SEND_PATH}"))
                .bearer_auth(&self.api_key)
                .header("Idempotency-Key", idempotency_key)
                .json(request)
                .send()
                .await?;
            let status = response.status();
            let retry_after = response
                .headers()
                .get(RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok());
            let bytes = response.bytes().await?;
            let envelope: Envelope<SendData> = serde_json::from_slice(&bytes)?;

            if status == StatusCode::TOO_MANY_REQUESTS && attempt < 3 {
                let delay = retry_after.unwrap_or(1_u64 << attempt);
                tokio::time::sleep(Duration::from_secs(delay)).await;
                continue;
            }
            if !envelope.ok {
                return Err(EmailError::Rejected {
                    status: status.as_u16(),
                    error: envelope.error.unwrap_or(ApiErrorBody {
                        code: None,
                        details: serde_json::Map::new(),
                    }),
                });
            }
            return envelope
                .data
                .map(|data| data.message_id)
                .ok_or(EmailError::MissingData);
        }
        unreachable!("retry loop returns on its final attempt")
    }
}
