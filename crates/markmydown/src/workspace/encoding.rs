//! UTF-8 decoding and line-ending preservation.
//!
//! The editor always works with `\n` internally. The original line ending and
//! byte-order mark are recorded and restored on save so a round-trip never
//! rewrites an untouched document.

/// The dominant line ending of a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineEnding {
    #[default]
    Lf,
    Crlf,
    Cr,
}

impl LineEnding {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::Crlf => "\r\n",
            Self::Cr => "\r",
        }
    }

    /// Detects the first line ending style in `text`.
    pub fn detect(text: &str) -> Self {
        let bytes = text.as_bytes();
        for (index, byte) in bytes.iter().enumerate() {
            if *byte == b'\n' {
                return if index > 0 && bytes[index - 1] == b'\r' {
                    Self::Crlf
                } else {
                    Self::Lf
                };
            }
            if *byte == b'\r' {
                return if bytes.get(index + 1) == Some(&b'\n') {
                    Self::Crlf
                } else {
                    Self::Cr
                };
            }
        }

        Self::Lf
    }

    /// Normalizes every line ending to `\n`.
    pub fn normalize(text: &str) -> String {
        if !text.contains('\r') {
            return text.to_string();
        }

        text.replace("\r\n", "\n").replace('\r', "\n")
    }

    /// Restores this line ending in text that uses `\n`.
    pub fn apply(self, text: &str) -> String {
        match self {
            Self::Lf => text.to_string(),
            Self::Crlf => text.replace('\n', "\r\n"),
            Self::Cr => text.replace('\n', "\r"),
        }
    }
}

/// A decoded document and the metadata needed for a lossless save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedText {
    /// Text with normalized `\n` line endings and no UTF-8 BOM.
    pub text: String,
    pub line_ending: LineEnding,
    pub had_bom: bool,
}

impl DecodedText {
    /// Encodes the text back to the original style.
    pub fn encode(&self, text: &str) -> Vec<u8> {
        let text = self.line_ending.apply(&LineEnding::normalize(text));
        let mut bytes = Vec::with_capacity(text.len() + 3);

        if self.had_bom {
            bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
        }
        bytes.extend_from_slice(text.as_bytes());
        bytes
    }
}

/// Decodes UTF-8, preserving line endings and the BOM. Other encodings are
/// reported rather than guessed at.
pub fn decode(bytes: &[u8]) -> Result<DecodedText, DecodeError> {
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        return Err(DecodeError::UnsupportedEncoding("UTF-16"));
    }

    let (had_bom, bytes) = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        (true, &bytes[3..])
    } else {
        (false, bytes)
    };

    let source = std::str::from_utf8(bytes).map_err(|error| DecodeError::InvalidUtf8 {
        valid_up_to: error.valid_up_to(),
    })?;

    Ok(DecodedText {
        text: LineEnding::normalize(source),
        line_ending: LineEnding::detect(source),
        had_bom,
    })
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DecodeError {
    #[error("document is not valid UTF-8 at byte {valid_up_to}")]
    InvalidUtf8 { valid_up_to: usize },
    #[error("unsupported document encoding: {0}")]
    UnsupportedEncoding(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lf_document_round_trips() {
        let bytes = b"# Title\n\nBody\n";
        let decoded = decode(bytes).expect("decode");

        assert_eq!(decoded.text, "# Title\n\nBody\n");
        assert_eq!(decoded.line_ending, LineEnding::Lf);
        assert!(!decoded.had_bom);
        assert_eq!(decoded.encode(&decoded.text), bytes);
    }

    #[test]
    fn crlf_document_preserves_line_endings() {
        let bytes = b"# Title\r\n\r\nBody\r\n";
        let decoded = decode(bytes).expect("decode");

        assert_eq!(decoded.text, "# Title\n\nBody\n");
        assert_eq!(decoded.line_ending, LineEnding::Crlf);
        assert_eq!(decoded.encode(&decoded.text), bytes);
    }

    #[test]
    fn bom_is_preserved() {
        let bytes = b"\xEF\xBB\xBF# Title\n";
        let decoded = decode(bytes).expect("decode");

        assert!(decoded.had_bom);
        assert_eq!(decoded.text, "# Title\n");
        assert_eq!(decoded.encode(&decoded.text), bytes);
    }

    #[test]
    fn invalid_utf8_is_reported_without_loss() {
        let bytes = [0xFF, 0xFE, 0x00, 0x61];
        let error = decode(&bytes).expect_err("invalid utf16");

        assert!(matches!(error, DecodeError::UnsupportedEncoding("UTF-16")));
    }

    #[test]
    fn broken_utf8_is_rejected() {
        let bytes = b"hello \xFF world";
        let error = decode(bytes).expect_err("invalid utf8");

        assert!(matches!(error, DecodeError::InvalidUtf8 { valid_up_to: 6 }));
    }

    #[test]
    fn mixed_line_endings_use_the_first_style_on_save() {
        let decoded = decode(b"one\r\ntwo\nthree\r\n").expect("decode");
        assert_eq!(decoded.line_ending, LineEnding::Crlf);
        assert_eq!(decoded.encode(&decoded.text), b"one\r\ntwo\r\nthree\r\n");
    }
}
