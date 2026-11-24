//! VIN anonymisation. Only the first 8 characters (WMI + start of VDS:
//! manufacturer, platform, model family) stay visible. Position 9 (check
//! digit) and the VIS (10-17: model year, plant, serial) are masked so the
//! full VIN cannot be reconstructed from shared logs.

pub const VISIBLE: usize = 8;

pub fn mask_vin(vin: &str) -> String {
    vin.chars()
        .enumerate()
        .map(|(i, c)| if i < VISIBLE { c } else { 'X' })
        .collect()
}

pub fn is_plausible_vin(s: &str) -> bool {
    s.len() == 17
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() && !"IOQ".contains(c.to_ascii_uppercase()))
}

/// Scrub any VIN-looking token (17 chars, no I/O/Q, with a digit and a letter)
/// from free text such as raw adapter responses, error strings and notes.
pub fn scrub_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut token = String::new();
    let flush = |token: &mut String, out: &mut String| {
        if token.len() == 17
            && is_plausible_vin(token)
            && token.chars().any(|c| c.is_ascii_digit())
            && token.chars().any(|c| c.is_ascii_alphabetic())
        {
            out.push_str(&mask_vin(token));
        } else {
            out.push_str(token);
        }
        token.clear();
    };
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            token.push(c);
        } else {
            flush(&mut token, &mut out);
            out.push(c);
        }
    }
    flush(&mut token, &mut out);
    out
}

/// Render a raw Mode 09 PID 02 reply without leaking the VIN: the ASCII VIN
/// bytes are replaced by the masked VIN and hex dumps are dropped.
pub fn redact_vin_reply(masked_vin: Option<&str>) -> String {
    match masked_vin {
        Some(v) => format!("[VIN redacted] {v}"),
        None => "[VIN reply redacted]".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_after_eighth_char() {
        assert_eq!(mask_vin("WAUZZZ8K9BA012345"), "WAUZZZ8KXXXXXXXXX");
    }

    #[test]
    fn scrubs_embedded_vin() {
        let s = scrub_text("vin=WAUZZZ8K9BA012345 ok");
        assert!(!s.contains("9BA012345"));
        assert!(s.contains("WAUZZZ8K"));
    }

    #[test]
    fn leaves_other_text() {
        assert_eq!(scrub_text("41 0C 1A F8"), "41 0C 1A F8");
        assert_eq!(scrub_text("ELM327 v1.5"), "ELM327 v1.5");
    }
}
