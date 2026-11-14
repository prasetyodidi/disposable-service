//! Pandoc Preset Specs: validated, adjustable conversion parameters.
//!
//! Values come from server defaults, optionally overridden per request. Every
//! override is validated against an allowlist or a strict pattern before it is
//! turned into pandoc arguments, so a client can never inject arbitrary
//! command-line options or Typst code.

use disposable_common::{PandocPresetInfo, PandocPresetOverrides, PandocPresetValues};

pub const PAPER_SIZES: &[&str] = &["a3", "a4", "a5", "letter", "legal"];

/// (name, typst hex colour)
pub const LINK_COLORS: &[(&str, &str)] = &[
    ("navy", "#000080"),
    ("black", "#000000"),
    ("blue", "#0000ff"),
    ("red", "#ff0000"),
    ("green", "#008000"),
    ("purple", "#800080"),
    ("teal", "#008080"),
    ("gray", "#808080"),
];

const LENGTH_UNITS: &[&str] = &["cm", "mm", "in", "pt"];

/// Default preset (Section 4.3.2 of the PRD).
pub fn default_values() -> PandocPresetValues {
    let main_font = std::env::var("PANDOC_MAIN_FONT")
        .ok()
        .filter(|f| valid_font(f));
    PandocPresetValues {
        paper_size: "a4".to_string(),
        margin_x: "2cm".to_string(),
        margin_y: "1.8cm".to_string(),
        font_size: "10.5pt".to_string(),
        line_height: 1.25,
        link_color: "navy".to_string(),
        main_font,
    }
}

/// Engine name for display and for choosing the argument style.
pub fn active_engine() -> String {
    let doc_engine = std::env::var("DOC_ENGINE").unwrap_or_default().to_lowercase();
    let pdf_engine =
        std::env::var("PANDOC_PDF_ENGINE").unwrap_or_else(|_| "pdflatex".to_string());
    if doc_engine == "typst" || pdf_engine == "typst" {
        "typst".to_string()
    } else {
        pdf_engine
    }
}

pub fn info() -> PandocPresetInfo {
    let engine = active_engine();
    let mut notes = vec![
        "Paper size, margins, font size and font apply to both engines.".to_string(),
    ];
    if engine == "typst" {
        notes.push(
            "Line height maps to Typst paragraph leading (leading = line height - 0.6 em)."
                .to_string(),
        );
        notes.push("Link colour is applied through a Typst show rule.".to_string());
    } else {
        notes.push("Link colour uses the LaTeX colour name (colorlinks=true).".to_string());
    }
    PandocPresetInfo {
        engine,
        defaults: default_values(),
        paper_sizes: PAPER_SIZES.iter().map(|s| s.to_string()).collect(),
        link_colors: LINK_COLORS.iter().map(|(n, _)| n.to_string()).collect(),
        notes,
    }
}

fn valid_font(font: &str) -> bool {
    let len = font.chars().count();
    (1..=64).contains(&len)
        && font
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == ' ' || c == '-')
        && font.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
}

/// Parses `<number><unit>`; returns the number and the unit.
fn parse_length(value: &str) -> Option<(f64, &str)> {
    let unit = LENGTH_UNITS.iter().find(|u| value.ends_with(**u))?;
    let number = &value[..value.len() - unit.len()];
    let well_formed = !number.is_empty()
        && number.len() <= 6
        && number.chars().all(|c| c.is_ascii_digit() || c == '.')
        && number.matches('.').count() <= 1
        && !number.starts_with('.')
        && !number.ends_with('.');
    if !well_formed {
        return None;
    }
    number.parse::<f64>().ok().map(|n| (n, *unit))
}

fn validate_margin(name: &str, value: &str) -> Result<(), String> {
    match parse_length(value) {
        Some((n, unit)) => {
            let max = match unit {
                "cm" => 10.0,
                "mm" => 100.0,
                "in" => 4.0,
                _ => 300.0, // pt
            };
            if n <= max {
                Ok(())
            } else {
                Err(format!("{} must not exceed {}{}", name, max, unit))
            }
        }
        None => Err(format!(
            "{} must be a number followed by cm, mm, in or pt (e.g. 2cm)",
            name
        )),
    }
}

/// Applies overrides on top of the defaults and validates the result.
pub fn resolve(overrides: Option<&PandocPresetOverrides>) -> Result<PandocPresetValues, String> {
    let mut v = default_values();
    let Some(o) = overrides else {
        return Ok(v);
    };

    if let Some(p) = &o.paper_size {
        let p = p.trim().to_lowercase();
        if !PAPER_SIZES.contains(&p.as_str()) {
            return Err(format!("paper_size must be one of: {}", PAPER_SIZES.join(", ")));
        }
        v.paper_size = p;
    }
    if let Some(m) = &o.margin_x {
        validate_margin("margin_x", m.trim())?;
        v.margin_x = m.trim().to_string();
    }
    if let Some(m) = &o.margin_y {
        validate_margin("margin_y", m.trim())?;
        v.margin_y = m.trim().to_string();
    }
    if let Some(f) = &o.font_size {
        let f = f.trim();
        match parse_length(f) {
            Some((n, "pt")) if (6.0..=24.0).contains(&n) => v.font_size = f.to_string(),
            _ => return Err("font_size must be between 6pt and 24pt (e.g. 10.5pt)".to_string()),
        }
    }
    if let Some(l) = o.line_height {
        if !l.is_finite() || !(1.0..=2.5).contains(&l) {
            return Err("line_height must be between 1.0 and 2.5".to_string());
        }
        v.line_height = (l * 100.0).round() / 100.0;
    }
    if let Some(c) = &o.link_color {
        let c = c.trim().to_lowercase();
        if !LINK_COLORS.iter().any(|(n, _)| *n == c) {
            let names: Vec<&str> = LINK_COLORS.iter().map(|(n, _)| *n).collect();
            return Err(format!("link_color must be one of: {}", names.join(", ")));
        }
        v.link_color = c;
    }
    if let Some(f) = &o.main_font {
        let f = f.trim();
        if f.is_empty() {
            v.main_font = None;
        } else if valid_font(f) {
            v.main_font = Some(f.to_string());
        } else {
            return Err(
                "main_font may contain only letters, digits, spaces and hyphens (max 64)"
                    .to_string(),
            );
        }
    }
    Ok(v)
}

/// `-V` arguments for the LaTeX engines.
pub fn latex_args(v: &PandocPresetValues) -> Vec<String> {
    let mut args = vec![
        "-V".to_string(),
        format!("papersize={}", v.paper_size),
        "-V".to_string(),
        format!(
            "geometry:top={y}, bottom={y}, left={x}, right={x}",
            x = v.margin_x,
            y = v.margin_y
        ),
        "-V".to_string(),
        format!("fontsize={}", v.font_size),
        "-V".to_string(),
        format!("lineheight={}", v.line_height),
        "-V".to_string(),
        "colorlinks=true".to_string(),
        "-V".to_string(),
        format!("linkcolor={}", v.link_color),
    ];
    if let Some(font) = &v.main_font {
        args.push("-V".to_string());
        args.push(format!("mainfont={}", font));
    }
    args
}

fn typst_paper(paper: &str) -> &str {
    match paper {
        "letter" => "us-letter",
        "legal" => "us-legal",
        other => other,
    }
}

/// Content of the pandoc `--metadata-file` (JSON is valid YAML) for Typst.
pub fn typst_metadata(v: &PandocPresetValues) -> String {
    let mut map = serde_json::json!({
        "papersize": typst_paper(&v.paper_size),
        "fontsize": v.font_size,
        "margin": { "x": v.margin_x, "y": v.margin_y },
    });
    if let Some(font) = &v.main_font {
        map["mainfont"] = serde_json::Value::String(font.clone());
    }
    map.to_string()
}

/// Raw Typst for `--include-in-header` (settings the pandoc template lacks).
pub fn typst_header(v: &PandocPresetValues) -> String {
    let hex = LINK_COLORS
        .iter()
        .find(|(n, _)| *n == v.link_color)
        .map(|(_, h)| *h)
        .unwrap_or("#000080");
    let leading = (v.line_height - 0.6).max(0.2);
    format!(
        "#set par(leading: {leading:.2}em)\n#show link: set text(fill: rgb(\"{hex}\"))\n",
        leading = leading,
        hex = hex
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ov() -> PandocPresetOverrides {
        PandocPresetOverrides::default()
    }

    #[test]
    fn defaults_match_prd() {
        let v = resolve(None).unwrap();
        assert_eq!(v.paper_size, "a4");
        assert_eq!(v.margin_x, "2cm");
        assert_eq!(v.margin_y, "1.8cm");
        assert_eq!(v.font_size, "10.5pt");
        assert_eq!(v.line_height, 1.25);
        assert_eq!(v.link_color, "navy");
    }

    #[test]
    fn valid_overrides_are_applied() {
        let o = PandocPresetOverrides {
            paper_size: Some("Letter".into()),
            margin_x: Some("25mm".into()),
            margin_y: Some("1in".into()),
            font_size: Some("12pt".into()),
            line_height: Some(1.5),
            link_color: Some("Blue".into()),
            main_font: Some("Helvetica Neue".into()),
        };
        let v = resolve(Some(&o)).unwrap();
        assert_eq!(v.paper_size, "letter");
        assert_eq!(v.margin_x, "25mm");
        assert_eq!(v.font_size, "12pt");
        assert_eq!(v.link_color, "blue");
        assert_eq!(v.main_font.as_deref(), Some("Helvetica Neue"));
    }

    #[test]
    fn invalid_values_are_rejected() {
        for bad in [
            PandocPresetOverrides { paper_size: Some("b5".into()), ..ov() },
            PandocPresetOverrides { margin_x: Some("2".into()), ..ov() },
            PandocPresetOverrides { margin_x: Some("2cm; rm".into()), ..ov() },
            PandocPresetOverrides { margin_y: Some("99cm".into()), ..ov() },
            PandocPresetOverrides { margin_y: Some("-1cm".into()), ..ov() },
            PandocPresetOverrides { font_size: Some("5pt".into()), ..ov() },
            PandocPresetOverrides { font_size: Some("12cm".into()), ..ov() },
            PandocPresetOverrides { line_height: Some(0.5), ..ov() },
            PandocPresetOverrides { line_height: Some(f64::NAN), ..ov() },
            PandocPresetOverrides { link_color: Some("#ff0000".into()), ..ov() },
            PandocPresetOverrides { main_font: Some("Foo\"); #bad".into()), ..ov() },
            PandocPresetOverrides { main_font: Some("a".repeat(65)), ..ov() },
        ] {
            assert!(resolve(Some(&bad)).is_err(), "should reject {:?}", bad);
        }
    }

    #[test]
    fn latex_args_follow_the_preset() {
        let args = latex_args(&resolve(None).unwrap()).join(" ");
        assert!(args.contains("papersize=a4"));
        assert!(args.contains("geometry:top=1.8cm, bottom=1.8cm, left=2cm, right=2cm"));
        assert!(args.contains("fontsize=10.5pt"));
        assert!(args.contains("linkcolor=navy"));
    }

    #[test]
    fn typst_files_follow_the_preset() {
        let o = PandocPresetOverrides {
            paper_size: Some("letter".into()),
            line_height: Some(1.5),
            link_color: Some("red".into()),
            ..ov()
        };
        let v = resolve(Some(&o)).unwrap();
        let meta: serde_json::Value = serde_json::from_str(&typst_metadata(&v)).unwrap();
        assert_eq!(meta["papersize"], "us-letter");
        assert_eq!(meta["margin"]["x"], "2cm");
        assert_eq!(meta["fontsize"], "10.5pt");
        let header = typst_header(&v);
        assert!(header.contains("leading: 0.90em"));
        assert!(header.contains("#ff0000"));
    }
}
