//! Display physical lengths in feet; one world unit represents one metre.

pub fn radius_label(radius: f32) -> String {
    format!("{:.4} ft", crate::editor::to_feet(radius))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radius_labels_use_feet() {
        assert_eq!(radius_label(0.3048), "1.0000 ft");
        assert_eq!(radius_label(0.1016), "0.3333 ft");
    }
}
