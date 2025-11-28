//! Theme system for syntax highlighting.
//!
//! Defines token styles and color schemes.

/// Token style categories for syntax highlighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenStyle {
    /// Keywords (fn, let, if, else, etc.)
    Keyword,
    /// Control flow keywords (if, else, for, while, match, etc.)
    ControlFlow,
    /// String literals
    String,
    /// Character literals
    Char,
    /// Numeric literals (integers, floats)
    Number,
    /// Comments (line and block)
    Comment,
    /// Function names
    Function,
    /// Type names
    Type,
    /// Variable names
    Variable,
    /// Constants and static values
    Constant,
    /// Operators (+, -, *, /, etc.)
    Operator,
    /// Punctuation (brackets, commas, semicolons)
    Punctuation,
    /// Attributes and annotations (#[...])
    Attribute,
    /// Macros (println!, vec!, etc.)
    Macro,
    /// Module/namespace names
    Module,
    /// Lifetime annotations ('a, 'static)
    Lifetime,
    /// Boolean literals (true, false)
    Boolean,
    /// Default text (no special highlighting)
    Default,
}

/// RGBA color represented as [r, g, b, a] with values 0.0-1.0.
pub type Color = [f32; 4];

/// A syntax highlighting theme.
#[derive(Debug, Clone)]
pub struct Theme {
    /// Theme name.
    pub name: String,
    /// Background color.
    pub background: Color,
    /// Default text color.
    pub foreground: Color,
    /// Colors for each token style.
    colors: std::collections::HashMap<TokenStyle, Color>,
}

impl Theme {
    /// Creates a new theme with the given name and default colors.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            background: [0.102, 0.102, 0.122, 1.0],    // #1A1A1F
            foreground: [0.902, 0.902, 0.902, 1.0],    // #E6E6E6
            colors: std::collections::HashMap::new(),
        }
    }

    /// Sets the color for a token style.
    pub fn set_color(&mut self, style: TokenStyle, color: Color) {
        self.colors.insert(style, color);
    }

    /// Gets the color for a token style, falling back to foreground.
    pub fn color(&self, style: TokenStyle) -> Color {
        self.colors.get(&style).copied().unwrap_or(self.foreground)
    }

    /// Creates the default dark theme (similar to One Dark).
    pub fn dark() -> Self {
        let mut theme = Self::new("Dark");

        // One Dark inspired colors
        theme.background = [0.102, 0.102, 0.122, 1.0];    // #1A1A1F
        theme.foreground = [0.682, 0.710, 0.749, 1.0];    // #ABB2BF

        // Keywords - purple/magenta
        theme.set_color(TokenStyle::Keyword, [0.769, 0.471, 0.839, 1.0]);      // #C477D6
        theme.set_color(TokenStyle::ControlFlow, [0.769, 0.471, 0.839, 1.0]); // #C477D6

        // Strings - green
        theme.set_color(TokenStyle::String, [0.596, 0.765, 0.475, 1.0]);       // #98C379
        theme.set_color(TokenStyle::Char, [0.596, 0.765, 0.475, 1.0]);         // #98C379

        // Numbers - orange
        theme.set_color(TokenStyle::Number, [0.824, 0.608, 0.467, 1.0]);       // #D29B77
        theme.set_color(TokenStyle::Boolean, [0.824, 0.608, 0.467, 1.0]);      // #D29B77

        // Comments - gray
        theme.set_color(TokenStyle::Comment, [0.455, 0.506, 0.557, 1.0]);      // #74818E

        // Functions - blue
        theme.set_color(TokenStyle::Function, [0.380, 0.686, 0.937, 1.0]);     // #61AFEF

        // Types - yellow/gold
        theme.set_color(TokenStyle::Type, [0.890, 0.780, 0.478, 1.0]);         // #E3C77A

        // Variables - red/coral
        theme.set_color(TokenStyle::Variable, [0.878, 0.439, 0.439, 1.0]);     // #E07070

        // Constants - orange
        theme.set_color(TokenStyle::Constant, [0.824, 0.608, 0.467, 1.0]);     // #D29B77

        // Operators - foreground
        theme.set_color(TokenStyle::Operator, [0.682, 0.710, 0.749, 1.0]);     // #ABB2BF

        // Punctuation - slightly dimmer
        theme.set_color(TokenStyle::Punctuation, [0.600, 0.627, 0.667, 1.0]);  // #99A0AA

        // Attributes - yellow
        theme.set_color(TokenStyle::Attribute, [0.890, 0.780, 0.478, 1.0]);    // #E3C77A

        // Macros - cyan
        theme.set_color(TokenStyle::Macro, [0.337, 0.788, 0.784, 1.0]);        // #56C9C8

        // Module - yellow
        theme.set_color(TokenStyle::Module, [0.890, 0.780, 0.478, 1.0]);       // #E3C77A

        // Lifetime - orange
        theme.set_color(TokenStyle::Lifetime, [0.824, 0.608, 0.467, 1.0]);     // #D29B77

        // Default
        theme.set_color(TokenStyle::Default, theme.foreground);

        theme
    }

    /// Creates a light theme.
    pub fn light() -> Self {
        let mut theme = Self::new("Light");

        theme.background = [0.984, 0.984, 0.984, 1.0];    // #FBFBFB
        theme.foreground = [0.231, 0.259, 0.322, 1.0];    // #3B4252

        // Keywords - purple
        theme.set_color(TokenStyle::Keyword, [0.627, 0.314, 0.706, 1.0]);      // #A050B4
        theme.set_color(TokenStyle::ControlFlow, [0.627, 0.314, 0.706, 1.0]); // #A050B4

        // Strings - green
        theme.set_color(TokenStyle::String, [0.306, 0.604, 0.024, 1.0]);       // #4E9A06
        theme.set_color(TokenStyle::Char, [0.306, 0.604, 0.024, 1.0]);         // #4E9A06

        // Numbers - blue
        theme.set_color(TokenStyle::Number, [0.114, 0.404, 0.804, 1.0]);       // #1D67CD
        theme.set_color(TokenStyle::Boolean, [0.114, 0.404, 0.804, 1.0]);      // #1D67CD

        // Comments - gray
        theme.set_color(TokenStyle::Comment, [0.502, 0.549, 0.596, 1.0]);      // #808C98

        // Functions - blue
        theme.set_color(TokenStyle::Function, [0.071, 0.345, 0.667, 1.0]);     // #1258AA

        // Types - teal
        theme.set_color(TokenStyle::Type, [0.016, 0.490, 0.490, 1.0]);         // #047D7D

        // Variables - dark red
        theme.set_color(TokenStyle::Variable, [0.753, 0.204, 0.204, 1.0]);     // #C03434

        // Constants - blue
        theme.set_color(TokenStyle::Constant, [0.114, 0.404, 0.804, 1.0]);     // #1D67CD

        // Default
        theme.set_color(TokenStyle::Default, theme.foreground);

        theme
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dark_theme() {
        let theme = Theme::dark();
        assert_eq!(theme.name, "Dark");

        // Verify keywords have a distinct color
        let keyword_color = theme.color(TokenStyle::Keyword);
        let default_color = theme.color(TokenStyle::Default);
        assert_ne!(keyword_color, default_color);
    }

    #[test]
    fn test_light_theme() {
        let theme = Theme::light();
        assert_eq!(theme.name, "Light");
    }

    #[test]
    fn test_fallback_color() {
        let theme = Theme::new("Test");
        // Unknown style should return foreground
        let color = theme.color(TokenStyle::Keyword);
        assert_eq!(color, theme.foreground);
    }
}
