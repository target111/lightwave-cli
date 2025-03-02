use reqwest::blocking::{Client, Response};
use reqwest::{Error as ReqwestError, StatusCode};
use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::fmt;

use crate::models::*;

// Define a custom error type for API operations
#[derive(Debug)]
pub enum ApiError {
    RequestError(ReqwestError),
    DeserializationError(String),
    ApiResponseError(String, StatusCode),
    ClientError(String),
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::RequestError(e) => write!(f, "Request failed: {}", e),
            ApiError::DeserializationError(e) => write!(f, "Failed to parse response: {}", e),
            ApiError::ApiResponseError(msg, status) => {
                write!(f, "API error ({}): {}", status.as_u16(), msg)
            }
            ApiError::ClientError(msg) => write!(f, "Client error: {}", msg),
        }
    }
}

impl Error for ApiError {}

impl From<ReqwestError> for ApiError {
    fn from(error: ReqwestError) -> Self {
        ApiError::RequestError(error)
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(error: serde_json::Error) -> Self {
        ApiError::DeserializationError(error.to_string())
    }
}

pub struct LightWaveClient {
    client: Client,
    base_url: String,
}

impl LightWaveClient {
    pub fn new() -> Result<Self, ApiError> {
        // Initialize with default or environment-provided values
        let base_url = Self::get_base_url(None)?;
        Ok(Self {
            client: Client::new(),
            base_url,
        })
    }

    pub fn with_base_url(base_url: &str) -> Result<Self, ApiError> {
        let base_url = Self::get_base_url(Some(base_url))?;
        Ok(Self {
            client: Client::new(),
            base_url,
        })
    }

    fn get_base_url(url_arg: Option<&str>) -> Result<String, ApiError> {
        // Priority:
        // 1. Command line argument
        // 2. LIGHTWAVE_URL environment variable
        // 3. Default URL
        if let Some(url) = url_arg {
            return Ok(Self::format_base_url(url));
        }

        if let Ok(url) = env::var("LIGHTWAVE_URL") {
            return Ok(Self::format_base_url(&url));
        }

        Ok(String::from("http://localhost:8000/api"))
    }

    fn format_base_url(url: &str) -> String {
        let url = url.trim_end_matches('/');
        
        // If URL contains "/api", use it as is
        if url.ends_with("/api") {
            return url.to_string();
        }
        
        // Otherwise, append "/api" to the URL
        format!("{}/api", url)
    }

    fn handle_response_error<T>(&self, resp: Response) -> Result<T, ApiError> {
        // Extract status code
        let status = resp.status();
        
        // Try to parse the error message from the response
        match resp.json::<ErrorResponse>() {
            Ok(error) => Err(ApiError::ApiResponseError(error.detail, status)),
            Err(_) => Err(ApiError::ApiResponseError(
                format!("Error status: {}", status),
                status,
            )),
        }
    }

    fn deserialize_response<T>(&self, resp: Response) -> Result<T, ApiError>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        if resp.status().is_success() {
            match resp.json::<T>() {
                Ok(data) => Ok(data),
                Err(e) => Err(ApiError::DeserializationError(format!(
                    "Failed to parse response: {}",
                    e
                ))),
            }
        } else {
            self.handle_response_error(resp)
        }
    }

    // ---- API Methods ----

    // Effects
    pub fn list_effects(&self) -> Result<EffectsListResponse, ApiError> {
        let resp = self.client.get(format!("{}/effects", self.base_url)).send()?;
        self.deserialize_response(resp)
    }

    pub fn get_effect_info(&self, name: &str) -> Result<EffectDetailedInfo, ApiError> {
        let resp = self
            .client
            .get(format!("{}/effects/{}", self.base_url, name))
            .send()?;
        self.deserialize_response(resp)
    }

    pub fn get_effect_status(&self) -> Result<EffectStatusResponse, ApiError> {
        let resp = self.client.get(format!("{}/status", self.base_url)).send()?;
        self.deserialize_response(resp)
    }

    pub fn start_effect(
        &self,
        name: &str,
        parameters: HashMap<String, serde_json::Value>,
    ) -> Result<(), ApiError> {
        let request = EffectStartRequest {
            name: name.to_string(),
            parameters,
        };

        let resp = self
            .client
            .post(format!("{}/effects/start", self.base_url))
            .json(&request)
            .send()?;

        if resp.status().is_success() {
            Ok(())
        } else {
            self.handle_response_error(resp)
        }
    }

    pub fn stop_effect(&self) -> Result<(), ApiError> {
        let resp = self
            .client
            .post(format!("{}/effects/stop", self.base_url))
            .send()?;

        if resp.status().is_success() {
            Ok(())
        } else {
            self.handle_response_error(resp)
        }
    }

    // LED Controls
    pub fn set_color(&self, color: &str) -> Result<(), ApiError> {
        let request = ColorRequest {
            color: color.to_string(),
        };

        let resp = self
            .client
            .post(format!("{}/leds/color", self.base_url))
            .json(&request)
            .send()?;

        if resp.status().is_success() {
            Ok(())
        } else {
            self.handle_response_error(resp)
        }
    }

    pub fn set_brightness(&self, brightness: f32) -> Result<(), ApiError> {
        if brightness < 0.0 || brightness > 1.0 {
            return Err(ApiError::ClientError(
                "Brightness must be between 0.0 and 1.0".to_string(),
            ));
        }

        let request = BrightnessRequest { brightness };

        let resp = self
            .client
            .post(format!("{}/leds/brightness", self.base_url))
            .json(&request)
            .send()?;

        if resp.status().is_success() {
            Ok(())
        } else {
            self.handle_response_error(resp)
        }
    }

    pub fn clear_leds(&self) -> Result<(), ApiError> {
        let resp = self
            .client
            .post(format!("{}/leds/clear", self.base_url))
            .send()?;

        if resp.status().is_success() {
            Ok(())
        } else {
            self.handle_response_error(resp)
        }
    }
}