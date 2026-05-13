use anyhow::{anyhow, Result};

pub fn normalize(input: &str) -> Result<String> {
    let s = input.trim();

    let hex = s.strip_prefix('#').unwrap_or(s);
    if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Ok(format!("#{}", hex.to_uppercase()));
    }
    if hex.len() == 3 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        let mut full = String::with_capacity(7);
        full.push('#');
        for c in hex.chars() {
            full.push(c);
            full.push(c);
        }
        return Ok(full.to_uppercase());
    }

    // Pass named colors through; pydantic_extra_types Color resolves them server-side.
    if s.chars().all(|c| c.is_ascii_alphabetic()) {
        return Ok(s.to_lowercase());
    }

    Err(anyhow!("could not parse color: {input:?}"))
}

/// Parse a normalized `#RRGGBB` string into RGB bytes. Returns `None` for
/// named colors or any non-6-digit hex form.
pub fn parse_hex_rgb(normalized: &str) -> Option<[u8; 3]> {
    let h = normalized.strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    Some([
        u8::from_str_radix(&h[0..2], 16).ok()?,
        u8::from_str_radix(&h[2..4], 16).ok()?,
        u8::from_str_radix(&h[4..6], 16).ok()?,
    ])
}
