use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

// Request Models
#[derive(Serialize, Debug)]
pub struct ColorRequest {
    pub color: String,
}

#[derive(Serialize, Debug)]
pub struct BrightnessRequest {
    pub brightness: f32,
}

#[derive(Serialize, Debug)]
pub struct EffectStartRequest {
    pub name: String,
    pub parameters: HashMap<String, serde_json::Value>,
}

// Response Models
#[derive(Deserialize, Debug)]
pub struct ErrorResponse {
    pub detail: String,
}

impl fmt::Display for ErrorResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.detail)
    }
}

#[derive(Deserialize, Debug)]
pub struct EffectInfo {
    pub name: String,
    pub description: String,
}

#[derive(Deserialize, Debug)]
pub struct EffectsListResponse {
    pub effects: Vec<EffectInfo>,
}

#[derive(Deserialize, Debug)]
pub struct EffectParameter {
    pub name: String,
    #[serde(rename = "type")]
    pub param_type: String,
    pub description: String,
    pub default: serde_json::Value,
    pub min_value: Option<serde_json::Value>,
    pub max_value: Option<serde_json::Value>,
    pub options: Option<Vec<String>>,
}

#[derive(Deserialize, Debug)]
pub struct EffectDetailedInfo {
    pub name: String,
    pub description: String,
    pub parameters: Vec<EffectParameter>,
}

#[derive(Deserialize, Debug)]
pub struct EffectStatusResponse {
    pub running: bool,
    pub name: Option<String>,
    pub description: Option<String>,
    pub parameters: Option<HashMap<String, serde_json::Value>>,
    pub start_time: Option<String>,
    pub runtime: Option<f64>,
}