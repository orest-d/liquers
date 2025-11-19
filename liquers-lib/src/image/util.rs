/// Parse a color string (name or RRGGBB[AA] hex value, without #) into (r, g, b, a) as f32 tuple.
/// Supports common color names and hex values like "ff0000" or "ff000080".
pub fn parse_color(s: &str) -> Option<(f32, f32, f32, f32)> {
    let s = s.trim().to_lowercase();
    // Named colors
    let named = match s.as_str() {
        "black" => (0.0, 0.0, 0.0, 1.0),
        "white" => (1.0, 1.0, 1.0, 1.0),
        "red" => (1.0, 0.0, 0.0, 1.0),
        "green" => (0.0, 1.0, 0.0, 1.0),
        "blue" => (0.0, 0.0, 1.0, 1.0),
        "yellow" => (1.0, 1.0, 0.0, 1.0),
        "cyan" => (0.0, 1.0, 1.0, 1.0),
        "magenta" => (1.0, 0.0, 1.0, 1.0),
        "gray" | "grey" => (0.5, 0.5, 0.5, 1.0),
        "orange" => (1.0, 0.65, 0.0, 1.0),
        "purple" => (0.5, 0.0, 0.5, 1.0),
        "brown" => (0.6, 0.4, 0.2, 1.0),
        "pink" => (1.0, 0.75, 0.8, 1.0),
        "lime" => (0.0, 1.0, 0.0, 1.0),
        "navy" => (0.0, 0.0, 0.5, 1.0),
        "teal" => (0.0, 0.5, 0.5, 1.0),
        "olive" => (0.5, 0.5, 0.0, 1.0),
        "maroon" => (0.5, 0.0, 0.0, 1.0),
        "silver" => (0.75, 0.75, 0.75, 1.0),
        _ => {
            // Try hex without #
            match s.len() {
                6 => {
                    // RRGGBB
                    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
                    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
                    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
                    return Some((r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0));
                }
                8 => {
                    // RRGGBBAA
                    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
                    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
                    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
                    let a = u8::from_str_radix(&s[6..8], 16).ok()?;
                    return Some((
                        r as f32 / 255.0,
                        g as f32 / 255.0,
                        b as f32 / 255.0,
                        a as f32 / 255.0,
                    ));
                }
                _ => return None,
            }
        }
    };
    Some(named)
}
