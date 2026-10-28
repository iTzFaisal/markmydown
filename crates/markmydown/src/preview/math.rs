//! Block math rendering.
//!
//! `iced_math` renders LaTeX to SVG. The preview embeds the SVG so it uses the
//! same theme color as the rest of the document; the same output is used by
//! HTML and PDF export.

use iced::Color;
use iced_math::MathRenderer;

/// Renders a display equation to standalone SVG bytes.
pub fn render_svg(source: &str, font_size: f32, color: Color) -> Result<Vec<u8>, MathError> {
    let color = iced_math::Color::rgb(to_u8(color.r), to_u8(color.g), to_u8(color.b));

    MathRenderer::new()
        .font_size(font_size)
        .display_style(true)
        .color(color)
        .to_svg(source)
        .map_err(|error| MathError(error.to_string()))
}

/// Renders a display equation as an SVG string.
pub fn render_svg_string(source: &str, font_size: f32, color: Color) -> Result<String, MathError> {
    let bytes = render_svg(source, font_size, color)?;
    String::from_utf8(bytes).map_err(|error| MathError(error.to_string()))
}

fn to_u8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("math rendering failed: {0}")]
pub struct MathError(pub String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_a_valid_equation_to_svg() {
        let svg = render_svg_string(r"\frac{a}{b}", 18.0, Color::BLACK).expect("render");

        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("</svg>"));
    }

    #[test]
    fn invalid_equation_is_an_error() {
        let error = render_svg_string(r"\frac{a", 18.0, Color::BLACK);

        assert!(error.is_err());
    }
}
