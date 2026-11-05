//! The small round swatch shown next to a category's name.

use gpui_kit::{App, Hsla, IntoElement, RenderOnce, Rgba, Styled, Window, div};

use crate::ui::format::categories::UNCATEGORIZED_COLOR;

/// A `#rrggbb` (or `#rgb`, `#rrggbbaa`) color, or `None` when it does not parse.
pub fn hex_color(hex: &str) -> Option<Hsla> {
    Rgba::try_from(hex).ok().map(Hsla::from)
}

/// A category color, falling back to the uncategorized gray when it does not parse.
pub fn category_color(hex: &str) -> Hsla {
    hex_color(hex)
        .or_else(|| hex_color(UNCATEGORIZED_COLOR))
        .unwrap_or_default()
}

#[derive(IntoElement)]
pub struct CategoryDot {
    color: Hsla,
}

impl CategoryDot {
    pub fn new(hex: &str) -> Self {
        Self {
            color: category_color(hex),
        }
    }
}

impl RenderOnce for CategoryDot {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div().size_2().flex_shrink_0().rounded_full().bg(self.color)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_colors_parse() {
        let red = hex_color("#ff0000").unwrap();
        assert_eq!(Rgba::from(red), Rgba::try_from("#f00").unwrap());
        assert!(hex_color("red").is_none());
        assert!(hex_color("").is_none());
    }

    #[test]
    fn a_bad_color_falls_back_to_gray() {
        assert_eq!(
            category_color("nope"),
            hex_color(UNCATEGORIZED_COLOR).unwrap()
        );
    }
}
