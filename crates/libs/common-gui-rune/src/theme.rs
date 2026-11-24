//! What a window draws with: the design system's two kits and the appearance the window was
//! told, when it was.

use crate::Ds;
use crate::appearance::set_appearance;
use rune_kit::Kit;

/// The kits and the system appearance a window watch read (true: dark). Until a read lands, the
/// appearance Rune reports stands.
#[derive(Default)]
pub struct Theme {
    ds: Ds,
    system_dark: Option<bool>,
}

impl Theme {
    /// Both kits, with no appearance read yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the window draws dark: the appearance it was told, else `system_dark`, what
    /// Rune reports (`Cx::system_dark`).
    pub fn dark(&self, system_dark: bool) -> bool {
        self.system_dark.unwrap_or(system_dark)
    }

    /// The kit for [`Theme::dark`].
    pub fn kit(&self, system_dark: bool) -> &Kit {
        self.ds.kit(self.dark(system_dark))
    }

    /// The appearance the window was told, if any.
    pub fn told(&self) -> Option<bool> {
        self.system_dark
    }

    /// Stores the appearance the window draws in (true: dark); true when that changed it.
    pub fn set(&mut self, dark: bool) -> bool {
        set_appearance(&mut self.system_dark, dark)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[test]
    fn set_reports_a_change_only_when_the_value_changes() {
        let mut theme = Theme::new();

        let first = theme.set(true);
        let again = theme.set(true);
        let flipped = theme.set(false);

        assert_eq!((first, again, flipped), (true, false, true));
        assert_eq!(theme.told(), Some(false));
    }

    #[rstest]
    #[case::dark(true)]
    #[case::light(false)]
    fn with_nothing_told_the_kit_follows_what_rune_reports(#[case] system_dark: bool) {
        let theme = Theme::new();

        assert_eq!(theme.kit(system_dark).dark, system_dark);
        assert_eq!(theme.dark(system_dark), system_dark);
    }

    #[rstest]
    #[case::told_dark(true)]
    #[case::told_light(false)]
    fn a_told_appearance_wins_over_what_rune_reports(#[case] told: bool) {
        let mut theme = Theme::new();
        theme.set(told);

        assert_eq!(theme.kit(!told).dark, told);
        assert_eq!(theme.dark(!told), told);
    }
}
