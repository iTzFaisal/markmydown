//! The built-in theme gallery.

use std::sync::LazyLock;

use super::tokens::ThemeTokens;

/// A named theme.
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeDefinition {
    pub id: &'static str,
    pub name: &'static str,
    pub tokens: ThemeTokens,
    /// Whether this theme appears in the gallery in addition to the light/dark
    /// quick choices.
    pub gallery: bool,
}

static THEMES: LazyLock<Vec<ThemeDefinition>> = LazyLock::new(|| {
    vec![
        ThemeDefinition {
            id: "light",
            name: "Light",
            tokens: ThemeTokens::light(),
            gallery: false,
        },
        ThemeDefinition {
            id: "dark",
            name: "Dark",
            tokens: ThemeTokens::dark(),
            gallery: false,
        },
        ThemeDefinition {
            id: "sepia",
            name: "Sepia",
            tokens: ThemeTokens::sepia(),
            gallery: true,
        },
        ThemeDefinition {
            id: "nord",
            name: "Nord",
            tokens: ThemeTokens::nord(),
            gallery: true,
        },
        ThemeDefinition {
            id: "solarized-light",
            name: "Solarized Light",
            tokens: ThemeTokens::solarized_light(),
            gallery: true,
        },
        ThemeDefinition {
            id: "solarized-dark",
            name: "Solarized Dark",
            tokens: ThemeTokens::solarized_dark(),
            gallery: true,
        },
        ThemeDefinition {
            id: "catppuccin-latte",
            name: "Catppuccin Latte",
            tokens: ThemeTokens::catppuccin_latte(),
            gallery: true,
        },
        ThemeDefinition {
            id: "catppuccin-mocha",
            name: "Catppuccin Mocha",
            tokens: ThemeTokens::catppuccin_mocha(),
            gallery: true,
        },
    ]
});

/// Every theme, including light and dark.
pub fn all() -> &'static [ThemeDefinition] {
    &THEMES
}

/// Themes shown in the gallery.
pub fn gallery() -> impl Iterator<Item = &'static ThemeDefinition> {
    THEMES.iter().filter(|theme| theme.gallery)
}

pub fn get(id: &str) -> Option<&'static ThemeDefinition> {
    THEMES.iter().find(|theme| theme.id == id)
}

pub fn light() -> &'static ThemeDefinition {
    get("light").expect("built-in light theme")
}

pub fn dark() -> &'static ThemeDefinition {
    get("dark").expect("built-in dark theme")
}

/// Resolves a theme id, with `system` choosing light or dark.
pub fn resolve(id: &str, system_dark: bool) -> &'static ThemeDefinition {
    match id {
        "system" => {
            if system_dark {
                dark()
            } else {
                light()
            }
        }
        other => get(other).unwrap_or_else(|| if system_dark { dark() } else { light() }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_light_dark_exist() {
        let mut ids: Vec<_> = all().iter().map(|theme| theme.id).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();

        assert_eq!(ids.len(), count);
        assert_eq!(light().id, "light");
        assert_eq!(dark().id, "dark");
        assert!(all().len() >= 8);
        assert_eq!(gallery().count(), all().len() - 2);
    }

    #[test]
    fn system_resolves_to_light_or_dark() {
        assert_eq!(resolve("system", false).id, "light");
        assert_eq!(resolve("system", true).id, "dark");
    }

    #[test]
    fn unknown_theme_falls_back() {
        assert_eq!(resolve("missing", false).id, "light");
        assert_eq!(resolve("missing", true).id, "dark");
    }

    #[test]
    fn every_theme_has_contrast() {
        for theme in all() {
            assert_ne!(
                theme.tokens.text, theme.tokens.background,
                "{} has no text contrast",
                theme.id
            );
        }
    }
}
