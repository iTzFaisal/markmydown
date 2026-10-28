//! Theme token model shared by editor, preview, and export.
//!
//! A single [`ThemeTokens`] value maps to the Iced application theme, the
//! editor's canvas colors, preview widget styles, and export CSS. Typography
//! is separate so the editor and preview can use independent fonts.

use iced::Color;

/// Font and spacing settings for one surface.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Typography {
    pub font_family: String,
    pub font_size: f32,
    pub line_height: f32,
    pub max_width: f32,
}

impl Typography {
    pub fn editor_default() -> Self {
        Self {
            font_family: String::from("SF Mono"),
            font_size: 14.0,
            line_height: 1.55,
            max_width: 0.0,
        }
    }

    pub fn preview_default() -> Self {
        Self {
            font_family: String::from("Georgia"),
            font_size: 17.0,
            line_height: 1.65,
            max_width: 760.0,
        }
    }
}

impl Default for Typography {
    fn default() -> Self {
        Self::preview_default()
    }
}

/// Every color used by the app, editor, preview, and exported documents.
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeTokens {
    pub background: Color,
    pub surface: Color,
    pub surface_raised: Color,
    pub text: Color,
    pub muted: Color,
    pub accent: Color,
    pub border: Color,
    pub selection: Color,
    pub code_background: Color,
    pub code_text: Color,
    pub link: Color,
    pub heading: Color,
    pub quote: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub is_dark: bool,
}

impl ThemeTokens {
    /// Maps the tokens to an Iced theme for built-in widgets.
    pub fn to_iced_theme(&self, name: &str) -> iced::Theme {
        iced::Theme::custom(
            name.to_string(),
            iced::theme::Palette {
                background: self.background,
                text: self.text,
                primary: self.accent,
                success: self.success,
                warning: self.warning,
                danger: self.error,
            },
        )
    }

    pub fn light() -> Self {
        Self {
            background: color(0xffffff),
            surface: color(0xf6f7f9),
            surface_raised: color(0xeceef2),
            text: color(0x1f2328),
            muted: color(0x59636e),
            accent: color(0x0969da),
            border: color(0xd1d9e0),
            selection: color(0xb6d7fe),
            code_background: color(0xf6f8fa),
            code_text: color(0x1f2328),
            link: color(0x0969da),
            heading: color(0x1f2328),
            quote: color(0x59636e),
            success: color(0x1a7f37),
            warning: color(0x9a6700),
            error: color(0xcf222e),
            is_dark: false,
        }
    }

    pub fn dark() -> Self {
        Self {
            background: color(0x0d1117),
            surface: color(0x161b22),
            surface_raised: color(0x21262d),
            text: color(0xe6edf3),
            muted: color(0x8d96a0),
            accent: color(0x2f81f7),
            border: color(0x30363d),
            selection: color(0x264f78),
            code_background: color(0x161b22),
            code_text: color(0xe6edf3),
            link: color(0x4493f8),
            heading: color(0xf0f6fc),
            quote: color(0x8d96a0),
            success: color(0x3fb950),
            warning: color(0xd29922),
            error: color(0xf85149),
            is_dark: true,
        }
    }

    pub fn sepia() -> Self {
        Self {
            background: color(0xf4ecd8),
            surface: color(0xede3cc),
            surface_raised: color(0xe4d8bd),
            text: color(0x3b3228),
            muted: color(0x6b5d4f),
            accent: color(0x8a5a2b),
            border: color(0xd8c8a8),
            selection: color(0xdcc9a5),
            code_background: color(0xeadfc8),
            code_text: color(0x3b3228),
            link: color(0x8a5a2b),
            heading: color(0x2f2820),
            quote: color(0x6b5d4f),
            success: color(0x4f6b2f),
            warning: color(0x8a6a1f),
            error: color(0xa33b2b),
            is_dark: false,
        }
    }

    pub fn nord() -> Self {
        Self {
            background: color(0x2e3440),
            surface: color(0x3b4252),
            surface_raised: color(0x434c5e),
            text: color(0xeceff4),
            muted: color(0xa3abb8),
            accent: color(0x88c0d0),
            border: color(0x4c566a),
            selection: color(0x4c566a),
            code_background: color(0x2e3440),
            code_text: color(0xeceff4),
            link: color(0x88c0d0),
            heading: color(0xeceff4),
            quote: color(0xa3abb8),
            success: color(0xa3be8c),
            warning: color(0xebcb8b),
            error: color(0xbf616a),
            is_dark: true,
        }
    }

    pub fn solarized_light() -> Self {
        Self {
            background: color(0xfdf6e3),
            surface: color(0xeee8d5),
            surface_raised: color(0xe4ddc8),
            text: color(0x073642),
            muted: color(0x657b83),
            accent: color(0x268bd2),
            border: color(0xd5cdb6),
            selection: color(0xd5cdb6),
            code_background: color(0xeee8d5),
            code_text: color(0x073642),
            link: color(0x268bd2),
            heading: color(0x073642),
            quote: color(0x657b83),
            success: color(0x859900),
            warning: color(0xb58900),
            error: color(0xdc322f),
            is_dark: false,
        }
    }

    pub fn solarized_dark() -> Self {
        Self {
            background: color(0x002b36),
            surface: color(0x073642),
            surface_raised: color(0x0a4550),
            text: color(0xeee8d5),
            muted: color(0x93a1a1),
            accent: color(0x268bd2),
            border: color(0x0f4b57),
            selection: color(0x0f4b57),
            code_background: color(0x073642),
            code_text: color(0xeee8d5),
            link: color(0x268bd2),
            heading: color(0xeee8d5),
            quote: color(0x93a1a1),
            success: color(0x859900),
            warning: color(0xb58900),
            error: color(0xdc322f),
            is_dark: true,
        }
    }

    pub fn catppuccin_latte() -> Self {
        Self {
            background: color(0xeff1f5),
            surface: color(0xe6e9ef),
            surface_raised: color(0xdce0e8),
            text: color(0x4c4f69),
            muted: color(0x6c6f85),
            accent: color(0x1e66f5),
            border: color(0xccd0da),
            selection: color(0xbcc0cc),
            code_background: color(0xe6e9ef),
            code_text: color(0x4c4f69),
            link: color(0x1e66f5),
            heading: color(0x4c4f69),
            quote: color(0x6c6f85),
            success: color(0x40a02b),
            warning: color(0xdf8e1d),
            error: color(0xd20f39),
            is_dark: false,
        }
    }

    pub fn catppuccin_mocha() -> Self {
        Self {
            background: color(0x1e1e2e),
            surface: color(0x181825),
            surface_raised: color(0x313244),
            text: color(0xcdd6f4),
            muted: color(0xa6adc8),
            accent: color(0x89b4fa),
            border: color(0x45475a),
            selection: color(0x585b70),
            code_background: color(0x181825),
            code_text: color(0xcdd6f4),
            link: color(0x89b4fa),
            heading: color(0xcdd6f4),
            quote: color(0xa6adc8),
            success: color(0xa6e3a1),
            warning: color(0xf9e2af),
            error: color(0xf38ba8),
            is_dark: true,
        }
    }
}

pub const fn color(rgb: u32) -> Color {
    Color::from_rgb(
        ((rgb >> 16) & 0xFF) as f32 / 255.0,
        ((rgb >> 8) & 0xFF) as f32 / 255.0,
        (rgb & 0xFF) as f32 / 255.0,
    )
}

/// Builds an Iced font from a family name. Iced requires `'static` names, so
/// user-selected family names are interned for the process lifetime.
pub fn font_for(family: &str) -> iced::Font {
    iced::Font {
        family: iced::font::Family::Name(Box::leak(family.to_string().into_boxed_str())),
        ..iced::Font::DEFAULT
    }
}

/// Detects the operating system's current dark-mode preference without any
/// network access.
pub fn system_is_dark() -> bool {
    #[cfg(target_os = "macos")]
    {
        return std::process::Command::new("defaults")
            .args(["read", "-g", "AppleInterfaceStyle"])
            .output()
            .is_ok_and(|output| {
                output.status.success()
                    && String::from_utf8_lossy(&output.stdout)
                        .to_ascii_lowercase()
                        .contains("dark")
            });
    }

    #[cfg(target_os = "windows")]
    {
        return std::process::Command::new("reg")
            .args([
                "query",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
                "/v",
                "AppsUseLightTheme",
            ])
            .output()
            .is_ok_and(|output| {
                let text = String::from_utf8_lossy(&output.stdout);
                text.contains("0x0")
            });
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Ok(theme) = std::env::var("GTK_THEME") {
            return theme.to_ascii_lowercase().contains("dark");
        }
        if let Ok(color_scheme) = std::process::Command::new("gsettings")
            .args(["get", "org.gnome.desktop.interface", "color-scheme"])
            .output()
        {
            return String::from_utf8_lossy(&color_scheme.stdout).contains("dark");
        }
        return false;
    }

    #[allow(unreachable_code)]
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_and_dark_tokens_have_contrasting_text() {
        assert!(!ThemeTokens::light().is_dark);
        assert!(ThemeTokens::dark().is_dark);
        assert_ne!(ThemeTokens::light().text, ThemeTokens::light().background);
        assert_ne!(ThemeTokens::dark().text, ThemeTokens::dark().background);
    }

    #[test]
    fn tokens_map_to_iced_palette() {
        let tokens = ThemeTokens::light();
        let theme = tokens.to_iced_theme("Test");

        assert_eq!(theme.palette().background, tokens.background);
        assert_eq!(theme.palette().primary, tokens.accent);
    }

    #[test]
    fn typography_defaults_are_sane() {
        let editor = Typography::editor_default();
        let preview = Typography::preview_default();

        assert!(editor.font_size > 0.0);
        assert!(preview.line_height > 1.0);
    }
}
