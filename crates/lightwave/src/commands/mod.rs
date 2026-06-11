use std::io::{self, Write};

use anyhow::Result;
use serde_json::{Value, json};

pub mod leds;
pub mod presets;
pub mod start;
pub mod stop;

pub fn print_json(value: &Value) -> Result<()> {
    let stdout = io::stdout();
    let mut stdout = stdout.lock();

    serde_json::to_writer(&mut stdout, value)?;
    writeln!(stdout)?;

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
