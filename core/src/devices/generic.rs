//! What every modern Razer mouse has in common, for models without an own module.
//!
//! Mice of the current generation (transaction id 0x1F or 0x3F) use the same commands for buttons,
//! macros and on-board profiles as the Naga Pro. Whether a model answers them is checked at
//! connection with read-only requests (`RazerMouse::probe`); only then are those features turned on,
//! marked as experimental.
//!
//! Button ids that are the same on every Razer mouse get names and factory bindings here; the
//! others are found by scanning and shown as "Button 0x.." with what they do now.

use crate::bindings::Action;

pub const BUTTONS: &[(u8, &str)] = &[
    (0x01, "Left button"),
    (0x02, "Right button"),
    (0x03, "Middle button (wheel)"),
    (0x04, "Side back"),
    (0x05, "Side forward"),
    (0x09, "Scroll up"),
    (0x0A, "Scroll down"),
    (0x34, "Wheel tilt left"),
    (0x35, "Wheel tilt right"),
];

/// Factory bindings that are the same on every mouse; others are unknown (no "Reset to default").
pub fn default_action(id: u8) -> Option<Action> {
    Some(match id {
        0x01..=0x05 | 0x09 | 0x0A => Action::mouse(id),
        0x34 => Action::mouse(0x68),
        0x35 => Action::mouse(0x69),
        _ => return None,
    })
}

/// Naga models keep their 12 thumb keys at 0x40..0x4B.
pub fn family_button_name(model: &str, id: u8) -> Option<String> {
    (model.contains("Naga") && (0x40..=0x4B).contains(&id)).then(|| format!("Side {}", id - 0x3F))
}

/// Does a database model use the current protocol generation (buttons/macros/profiles may answer)?
/// 0x1F — 2020 and newer (Naga Pro, Basilisk V3…), 0x3F — 2019–2020 (Viper Ultimate, DeathAdder V2…).
pub fn may_have_onboard(transaction_id: u8) -> bool {
    matches!(transaction_id, 0x1F | 0x3F)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_defaults() {
        assert_eq!(family_button_name("Razer Naga X", 0x40).as_deref(), Some("Side 1"));
        assert_eq!(family_button_name("Razer Naga X", 0x4B).as_deref(), Some("Side 12"));
        assert_eq!(family_button_name("Razer Basilisk V3", 0x40), None);
        assert_eq!(default_action(0x01), Some(Action::mouse(1)));
        assert_eq!(default_action(0x0B), None);
    }
}
