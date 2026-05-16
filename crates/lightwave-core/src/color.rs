use anyhow::{Result, bail};

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
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphabetic()) {
        return Ok(s.to_lowercase());
    }

    bail!("could not parse color: {input:?}")
}

/// Parse a normalized `#RRGGBB` string into RGB bytes. Returns `None` for
/// named colors or any non-6-digit hex form.
pub fn parse_hex_rgb(normalized: &str) -> Option<[u8; 3]> {
    let h = normalized.strip_prefix('#')?;
    let bytes = h.as_bytes();

    if bytes.len() != 6 || !bytes.iter().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }

    Some([
        u8::from_str_radix(h.get(0..2)?, 16).ok()?,
        u8::from_str_radix(h.get(2..4)?, 16).ok()?,
        u8::from_str_radix(h.get(4..6)?, 16).ok()?,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_long_hex() {
        assert_eq!(normalize("#ff00aa").unwrap(), "#FF00AA");
        assert_eq!(normalize("ff00aa").unwrap(), "#FF00AA");
    }

    #[test]
    fn normalizes_short_hex() {
        assert_eq!(normalize("#f0a").unwrap(), "#FF00AA");
        assert_eq!(normalize("f0a").unwrap(), "#FF00AA");
    }

    #[test]
    fn normalizes_named_colors() {
        assert_eq!(normalize("Red").unwrap(), "red");
        assert_eq!(normalize("blue").unwrap(), "blue");
    }

    #[test]
    fn rejects_invalid_colors() {
        assert!(normalize("").is_err());
        assert!(normalize("#12").is_err());
        assert!(normalize("not-a-color").is_err());
        assert!(normalize("#red").is_err());
    }

    #[test]
    fn parses_rgb_hex() {
        assert_eq!(parse_hex_rgb("#FF00AA"), Some([255, 0, 170]));
        assert_eq!(parse_hex_rgb("#000000"), Some([0, 0, 0]));
        assert_eq!(parse_hex_rgb("#ffffff"), Some([255, 255, 255]));
    }

    #[test]
    fn does_not_parse_named_colors_as_rgb() {
        assert_eq!(parse_hex_rgb("red"), None);
    }

    #[test]
    fn does_not_panic_on_non_ascii_input() {
        assert_eq!(parse_hex_rgb("#ééé"), None);
    }

    #[test]
    fn rejects_invalid_hex_rgb() {
        assert_eq!(parse_hex_rgb("#GGGGGG"), None);
        assert_eq!(parse_hex_rgb("#123"), None);
        assert_eq!(parse_hex_rgb("FF00AA"), None);
    }
}
