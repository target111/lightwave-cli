use colored::Colorize;
use std::collections::HashMap;

// Format a JSON value with colors
pub fn format_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "null".dimmed().to_string(),
        serde_json::Value::Bool(b) => {
            if *b {
                "true".green()
            } else {
                "false".red()
            }
            .to_string()
        }
        serde_json::Value::Number(n) => n.to_string().cyan().to_string(),
        serde_json::Value::String(s) => format!("\"{}\"", s).yellow().to_string(),
        serde_json::Value::Array(a) => {
            let items: Vec<String> = a.iter().map(format_value).collect();
            format!("[{}]", items.join(", "))
        }
        serde_json::Value::Object(o) => {
            let items: Vec<String> = o
                .iter()
                .map(|(k, v)| format!("{}: {}", k.cyan(), format_value(v)))
                .collect();
            format!("{{{}}}", items.join(", "))
        }
    }
}

// Format time in seconds to a user-friendly string
pub fn format_time(seconds: f64) -> String {
    if seconds < 60.0 {
        format!("{:.1}s", seconds)
    } else if seconds < 3600.0 {
        let minutes = (seconds / 60.0).floor();
        let secs = seconds - (minutes * 60.0);
        format!("{}m {:.1}s", minutes, secs)
    } else {
        let hours = (seconds / 3600.0).floor();
        let minutes = ((seconds - (hours * 3600.0)) / 60.0).floor();
        let secs = seconds - (hours * 3600.0) - (minutes * 60.0);
        format!("{}h {}m {:.1}s", hours, minutes, secs)
    }
}

// Parse command-line parameters for effects
pub fn parse_params(params: &[String]) -> HashMap<String, serde_json::Value> {
    let mut result = HashMap::new();

    for param in params {
        if let Some((key, value)) = param.split_once('=') {
            // Try to parse as different types (int, float, boolean, or fallback to string)
            let parsed_value = if value.eq_ignore_ascii_case("true") {
                serde_json::Value::Bool(true)
            } else if value.eq_ignore_ascii_case("false") {
                serde_json::Value::Bool(false)
            } else if let Ok(num) = value.parse::<i64>() {
                serde_json::Value::Number(num.into())
            } else if let Ok(num) = value.parse::<f64>() {
                // Create a number from a float
                match serde_json::Number::from_f64(num) {
                    Some(n) => serde_json::Value::Number(n),
                    None => serde_json::Value::String(value.to_string()),
                }
            } else {
                serde_json::Value::String(value.to_string())
            };

            result.insert(key.to_string(), parsed_value);
        }
    }

    result
}

// Error formatting helper
pub fn format_error(err: &dyn std::error::Error) -> String {
    err.to_string().red().to_string()
}