//! Display lengths in customary units; one world unit represents one meter.
#[derive(Clone, Copy)]
pub enum LengthUnit {
    Inches,
    Feet,
}

impl LengthUnit {
    fn world_per_unit(self) -> f32 {
        match self {
            Self::Inches => 0.0254,
            Self::Feet => 0.3048,
        }
    }

    pub fn from_world(self, value: f32) -> f32 {
        value / self.world_per_unit()
    }

    pub fn to_world(self, value: f32) -> f32 {
        value * self.world_per_unit()
    }

    pub fn suffix(self) -> &'static str {
        match self {
            Self::Inches => " in",
            Self::Feet => " ft",
        }
    }
}

pub fn radius_label(radius: f32) -> String {
    format!("{:.4} in", LengthUnit::Inches.from_world(radius))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn customary_lengths_match_world_scale() {
        assert!((LengthUnit::Inches.from_world(0.1016) - 4.0).abs() < 1e-6);
        assert!((LengthUnit::Feet.from_world(0.3048) - 1.0).abs() < 1e-6);
        for unit in [LengthUnit::Inches, LengthUnit::Feet] {
            assert!((unit.to_world(unit.from_world(1.5)) - 1.5).abs() < 1e-6);
        }
    }
}
