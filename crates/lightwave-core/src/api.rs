use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use reqwest::{
    StatusCode, Url,
    blocking::{Client as HttpClient, Response},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PresetSummary {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PresetsListResponse {
    pub presets: Vec<PresetSummary>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PresetInfo {
    pub description: String,
    pub args: Vec<ArgSchema>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ArgSchema {
    pub name: String,
    #[serde(rename = "type")]
    pub arg_type: String,
    pub default: Value,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
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

#[derive(Clone)]
pub struct Client {
    base: Url,
    http: HttpClient,
}

impl Client {
    pub fn new(base: impl AsRef<str>) -> Result<Self> {
        let input = base.as_ref().trim();

        if input.is_empty() {
            bail!("server URL cannot be empty");
        }

        let mut base = Url::parse(input)
            .with_context(|| format!("invalid LightWave server URL: {input:?}"))?;

        match base.scheme() {
            "http" | "https" => {}
            scheme => bail!("unsupported server URL scheme {scheme:?}; expected http or https"),
        }

        base.set_query(None);
        base.set_fragment(None);

        let http = HttpClient::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .context("building HTTP client")?;

        Ok(Self { base, http })
    }

    fn url(&self, segments: &[&str]) -> Result<Url> {
        let mut url = self.base.clone();

        {
            let mut path = url
                .path_segments_mut()
                .map_err(|_| anyhow!("server URL {} cannot be used as a base URL", self.base))?;

            path.pop_if_empty();
            path.extend(segments.iter().copied());
        }

        Ok(url)
    }

    fn ensure_success(response: Response, endpoint: &str) -> Result<Response> {
        let status = response.status();

        if status.is_success() {
            return Ok(response);
        }

        let body = response
            .text()
            .unwrap_or_else(|_| "<failed to read response body>".to_string());

        if body.trim().is_empty() {
            bail!("{endpoint} failed with HTTP {status}");
        }

        bail!("{endpoint} failed with HTTP {status}: {body}");
    }

    fn get_json<T>(&self, endpoint: &str, url: Url) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let response = self
            .http
            .get(url.clone())
            .send()
            .with_context(|| format!("GET {url}"))?;

        let response = Self::ensure_success(response, endpoint)?;

        response
            .json()
            .with_context(|| format!("decoding response from {endpoint}"))
    }

    fn post_json<T>(&self, endpoint: &str, url: Url, body: &T) -> Result<()>
    where
        T: Serialize + ?Sized,
    {
        let response = self
            .http
            .post(url.clone())
            .json(body)
            .send()
            .with_context(|| format!("POST {url}"))?;

        Self::ensure_success(response, endpoint)?;
        Ok(())
    }

    fn post_empty(&self, endpoint: &str, url: Url) -> Result<()> {
        let response = self
            .http
            .post(url.clone())
            .send()
            .with_context(|| format!("POST {url}"))?;

        Self::ensure_success(response, endpoint)?;
        Ok(())
    }

    pub fn list_presets(&self) -> Result<PresetsListResponse> {
        self.get_json("/presets", self.url(&["presets"])?)
    }

    pub fn preset_info(&self, name: &str) -> Result<PresetInfo> {
        let endpoint = format!("/presets/{name}");
        self.get_json(&endpoint, self.url(&["presets", name])?)
    }

    pub fn running(&self) -> Result<Option<RunningPreset>> {
        let url = self.url(&["presets", "running"])?;

        let response = self
            .http
            .get(url.clone())
            .send()
            .with_context(|| format!("GET {url}"))?;

        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }

        let response = Self::ensure_success(response, "/presets/running")?;

        response
            .json()
            .map(Some)
            .context("decoding response from /presets/running")
    }

    pub fn start(&self, name: &str, args: &Value) -> Result<()> {
        let body = StartRequest {
            preset_name: name,
            args,
        };

        self.post_json("/presets/start", self.url(&["presets", "start"])?, &body)
    }

    pub fn stop(&self) -> Result<()> {
        self.post_empty("/presets/stop", self.url(&["presets", "stop"])?)
    }

    pub fn set_color(&self, hex: &str) -> Result<()> {
        let body = serde_json::json!({ "color": hex });

        self.post_json(
            "/leds/color/set",
            self.url(&["leds", "color", "set"])?,
            &body,
        )
    }

    pub fn set_brightness(&self, brightness: f32) -> Result<()> {
        if !(0.0..=1.0).contains(&brightness) {
            bail!("brightness must be between 0.0 and 1.0");
        }

        let body = serde_json::json!({ "brightness": brightness });

        self.post_json(
            "/leds/brightness",
            self.url(&["leds", "brightness"])?,
            &body,
        )
    }

    pub fn clear(&self) -> Result<()> {
        self.post_empty("/leds/color/clear", self.url(&["leds", "color", "clear"])?)
    }
}
