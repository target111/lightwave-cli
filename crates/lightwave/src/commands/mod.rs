use std::io::{self, Write};

use anyhow::Result;
use lightwave_core::RunningEffect;
use serde_json::{Value, json};

#[cfg(feature = "ambilight")]
pub mod ambilight;
pub mod effects;
pub mod leds;
#[cfg(feature = "music")]
pub mod music;
pub mod presets;
pub mod start;
pub mod status;
pub mod stop;

/// `{preset} · {name}`, or just `{name}` when the effect wasn't started from a preset.
pub fn running_title(r: &RunningEffect) -> String {
    match &r.preset {
        Some(preset) => format!("{preset} · {}", r.name),
        None => r.name.clone(),
    }
}

/// 20-char bar of filled/empty blocks for a 0.0..=1.0 level.
pub fn brightness_bar(level: f64) -> String {
    let filled = (level * 20.0).round() as usize;

    (0..20)
        .map(|i| if i < filled { '█' } else { '░' })
        .collect()
}

pub fn print_json(value: &Value) -> Result<()> {
    let stdout = io::stdout();
    let mut stdout = stdout.lock();

    serde_json::to_writer(&mut stdout, value)?;
    writeln!(stdout)?;
    stdout.flush()?;

    Ok(())
}

pub fn print_ok_json(extra: Value) -> Result<()> {
    let mut obj = serde_json::Map::new();
    obj.insert("ok".to_string(), Value::Bool(true));

    if let Value::Object(map) = extra {
        for (key, value) in map {
            if key != "ok" {
                obj.insert(key, value);
            }
        }
    }

    print_json(&Value::Object(obj))
}

pub fn print_error_json(error: impl ToString) -> Result<()> {
    print_json(&json!({
        "ok": false,
        "error": error.to_string(),
    }))
}

pub fn print_arg_error_json(detail: impl ToString) -> Result<()> {
    print_json(&json!({
        "ok": false,
        "error": "arg_parse",
        "detail": detail.to_string(),
    }))
}
