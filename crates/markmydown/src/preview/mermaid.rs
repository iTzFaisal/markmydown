//! Mermaid to SVG rendering.
//!
//! `merman` renders diagrams headlessly. The preview embeds the resulting SVG
//! and scales it to the available width.

use merman::{OperationControl, RenderOutput, RenderRequest, Renderer, SvgRequest};

/// Renders a Mermaid source block to an SVG document.
pub fn render_svg(source: &str) -> Result<String, MermaidError> {
    let output = Renderer::new()
        .render(RenderRequest::svg(
            source,
            OperationControl::new(),
            SvgRequest::default(),
        ))
        .map_err(|error| MermaidError(error.to_string()))?;

    match output {
        RenderOutput::Svg(Some(svg)) => Ok(svg.svg().to_string()),
        _ => Err(MermaidError(String::from(
            "the Mermaid source produced no diagram",
        ))),
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("Mermaid rendering failed: {0}")]
pub struct MermaidError(pub String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_a_flowchart() {
        let svg = render_svg("flowchart TD\n    A[Start] --> B[Done]").expect("render");

        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
    }

    #[test]
    fn invalid_source_fails_without_panicking() {
        let result = render_svg("this is not a mermaid diagram");

        assert!(result.is_err());
    }
}
