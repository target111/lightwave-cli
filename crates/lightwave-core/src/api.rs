use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct PresetSummary {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Deserialize)]
pub struct PresetsListResponse {
    pub presets: Vec<PresetSummary>,
}

#[derive(Debug, Deserialize)]
pub struct PresetInfo {
    pub description: String,
    pub args: Vec<ArgSchema>,
}

#[derive(Debug, Deserialize)]
pub struct ArgSchema {
    pub name: String,
    #[serde(rename = "type")]
    pub arg_type: String,
    pub default: Value,
    pub description: String,
}

#[derive(Debug, Deserialize)]
pub struct RunningPreset {
    pub name: String,
    pub description: String,
    pub start_time: String,
    pub duration_seconds: f64,
}

#[derive(Debug, Serialize)]
pub struct StartRequest<'a> {
    pub preset_name: &'a str,
    pub args: &'a Value,
}

pub struct Client {
    base: String,
    http: reqwest::blocking::Client,
}

impl Client {
    pub fn new(base: impl Into<String>) -> Self {
        Self {
            base: base.into().trim_end_matches('/').to_string(),
            http: reqwest::blocking::Client::new(),
        }
    }

    pub fn list_presets(&self) -> Result<PresetsListResponse> {
        self.http.get(format!("{}/presets", self.base))
            .send()?.error_for_status()?.json()
            .context("decoding /presets")
    }

    pub fn preset_info(&self, name: &str) -> Result<PresetInfo> {
        self.http.get(format!("{}/presets/{}", self.base, name))
            .send()?.error_for_status()?.json()
            .with_context(|| format!("decoding /presets/{name}"))
    }

    pub fn running(&self) -> Result<Option<RunningPreset>> {
        let resp = self.http.get(format!("{}/presets/running", self.base)).send()?;
        if resp.status() == 404 { return Ok(None); }
        Ok(Some(resp.error_for_status()?.json()?))
    }

    pub fn start(&self, name: &str, args: &Value) -> Result<()> {
        self.http.post(format!("{}/presets/start", self.base))
            .json(&StartRequest { preset_name: name, args })
            .send()?.error_for_status()?;
        Ok(())
    }

    pub fn stop(&self) -> Result<()> {
        self.http.post(format!("{}/presets/stop", self.base))
            .send()?.error_for_status()?;
        Ok(())
    }

    pub fn set_color(&self, hex: &str) -> Result<()> {
        self.http.post(format!("{}/leds/color/set", self.base))
            .json(&serde_json::json!({ "color": hex }))
            .send()?.error_for_status()?;
        Ok(())
    }

    pub fn set_brightness(&self, b: f32) -> Result<()> {
        self.http.post(format!("{}/leds/color/brightness", self.base))
            .json(&serde_json::json!({ "brightness": b }))
            .send()?.error_for_status()?;
        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        self.http.post(format!("{}/leds/color/clear", self.base))
            .send()?.error_for_status()?;
        Ok(())
    }
}
