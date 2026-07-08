use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use reqwest::{
    StatusCode, Url,
    blocking::{Client as HttpClient, Response},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EffectSummary {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EffectsListResponse {
    pub effects: Vec<EffectSummary>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EffectInfo {
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
pub struct RunningEffect {
    pub name: String,
    pub description: String,
    /// Preset the effect was started from, if any.
    pub preset: Option<String>,
    pub start_time: String,
    pub duration_seconds: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PresetRecord {
    pub name: String,
    pub effect: String,
    pub args: Value,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PresetsListResponse {
    pub presets: Vec<PresetRecord>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StartStatus {
    pub status: String,
    pub effect: Option<String>,
    pub preset: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LedState {
    pub count: usize,
    pub brightness: f64,
    pub pixels: Vec<[u8; 3]>,
}

#[derive(Debug, Serialize)]
pub struct StartRequest<'a> {
    pub effect_name: &'a str,
    pub args: &'a Value,
}

#[derive(Debug, Serialize)]
struct PresetBody<'a> {
    effect: &'a str,
    args: &'a Value,
    description: &'a str,
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

    /// GET that treats 404 as "not there" instead of an error.
    fn get_json_opt<T>(&self, endpoint: &str, url: Url) -> Result<Option<T>>
    where
        T: DeserializeOwned,
    {
        let response = self
            .http
            .get(url.clone())
            .send()
            .with_context(|| format!("GET {url}"))?;

        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }

        let response = Self::ensure_success(response, endpoint)?;

        response
            .json()
            .map(Some)
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

    /// Host portion of the server URL, for protocols that bypass HTTP (e.g. UDP).
    pub fn host(&self) -> &str {
        self.base.host_str().unwrap_or("localhost")
    }

    // ---- effects ----

    pub fn list_effects(&self) -> Result<EffectsListResponse> {
        self.get_json("/effects", self.url(&["effects"])?)
    }

    /// Schema for one effect, or None if no effect has that name.
    pub fn effect_info(&self, name: &str) -> Result<Option<EffectInfo>> {
        let endpoint = format!("/effects/{name}");
        self.get_json_opt(&endpoint, self.url(&["effects", name])?)
    }

    pub fn running(&self) -> Result<Option<RunningEffect>> {
        self.get_json_opt("/effects/running", self.url(&["effects", "running"])?)
    }

    pub fn start(&self, name: &str, args: &Value) -> Result<()> {
        let body = StartRequest {
            effect_name: name,
            args,
        };

        self.post_json("/effects/start", self.url(&["effects", "start"])?, &body)
    }

    pub fn stop(&self) -> Result<()> {
        self.post_empty("/effects/stop", self.url(&["effects", "stop"])?)
    }

    // ---- presets ----

    pub fn list_presets(&self) -> Result<PresetsListResponse> {
        self.get_json("/presets", self.url(&["presets"])?)
    }

    pub fn save_preset(
        &self,
        name: &str,
        effect: &str,
        args: &Value,
        description: &str,
    ) -> Result<PresetRecord> {
        let endpoint = format!("/presets/{name}");
        let url = self.url(&["presets", name])?;
        let body = PresetBody {
            effect,
            args,
            description,
        };

        let response = self
            .http
            .put(url.clone())
            .json(&body)
            .send()
            .with_context(|| format!("PUT {url}"))?;

        let response = Self::ensure_success(response, &endpoint)?;

        response
            .json()
            .with_context(|| format!("decoding response from {endpoint}"))
    }

    /// Delete a preset; Ok(false) means the server has no preset by that name.
    pub fn delete_preset(&self, name: &str) -> Result<bool> {
        let endpoint = format!("/presets/{name}");
        let url = self.url(&["presets", name])?;

        let response = self
            .http
            .delete(url.clone())
            .send()
            .with_context(|| format!("DELETE {url}"))?;

        if response.status() == StatusCode::NOT_FOUND {
            return Ok(false);
        }

        Self::ensure_success(response, &endpoint)?;
        Ok(true)
    }

    /// Start a saved preset; Ok(None) means no preset by that name.
    pub fn start_preset(&self, name: &str) -> Result<Option<StartStatus>> {
        let endpoint = format!("/presets/{name}/start");
        let url = self.url(&["presets", name, "start"])?;

        let response = self
            .http
            .post(url.clone())
            .send()
            .with_context(|| format!("POST {url}"))?;

        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }

        let response = Self::ensure_success(response, &endpoint)?;

        response
            .json()
            .map(Some)
            .with_context(|| format!("decoding response from {endpoint}"))
    }

    // ---- leds ----

    pub fn led_state(&self) -> Result<LedState> {
        self.get_json("/leds", self.url(&["leds"])?)
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
