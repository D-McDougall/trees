//! Species-specific growth parameters.
//!
//! A [`TreeSpecies`] is built and validated once, then wrapped in an `Arc` and
//! shared by every node of a tree. Nothing ever gets `&mut` access to it after
//! that, which is how the design's "parameters are fixed for the tree's
//! lifetime" rule is enforced by the type system rather than by convention.
//!
//! Parameters are grouped by the growth stage that reads them, so each stage
//! of the simulator can take just the group it needs. Symbols in the docs
//! follow `paper-summaries.md` (λ, κ, ρ, θ, …).

use std::fmt;

/// How many lateral buds each node carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BranchingPattern {
    /// One lateral bud per node, rotated by the divergence angle each node
    /// (most broadleaf trees, e.g. oak, birch).
    Alternate,
    /// Two lateral buds facing each other per node (e.g. maple, ash).
    Opposite,
}

impl BranchingPattern {
    /// Number of lateral child slots a node of this species uses.
    pub fn lateral_count(self) -> usize {
        match self {
            BranchingPattern::Alternate => 1,
            BranchingPattern::Opposite => 2,
        }
    }
}

/// Local branching geometry: the architectural part of the model.
#[derive(Clone, Debug, PartialEq)]
pub struct Architecture {
    /// One or two lateral buds per node.
    pub branching: BranchingPattern,
    /// Divergence (phyllotaxis) angle φ between successive nodes, in radians.
    /// The golden angle (≈137.5°) is the usual default.
    pub divergence_angle: f32,
    /// Angle ψ between a lateral bud and its parent's heading, in radians.
    pub branching_angle: f32,
    /// Angle δ by which the continuing (main) bud deflects away from the
    /// lateral, in radians. Non-zero values give zig-zag, gnarled axes.
    pub deflection_angle: f32,
    /// Base internode length, in world units. Also sets the light-grid cell
    /// size and the default occupancy radius.
    pub internode_length: f32,
}

/// How a branch point splits vigor between the continuing axis and laterals.
#[derive(Clone, Debug, PartialEq)]
pub enum VigorModel {
    /// Extended Borchert–Honda: v_main ∝ λ·Q_main, v_lat ∝ (1−λ)·Q_lat.
    /// The default, and the one Longay's unified model uses.
    BorchertHonda,
    /// Pałubicki's priority model: branches along an axis are ranked by light
    /// and weighted by a piecewise-linear function from `w_max` down to
    /// `w_min` over the top `top_fraction` of ranks. Better for animating a
    /// tree that loses apical control with age.
    Priority {
        /// Weight of the best-lit branch on an axis.
        w_max: f32,
        /// Weight of every branch ranked below `top_fraction`.
        w_min: f32,
        /// Fraction of ranks, in (0, 1], over which the weight falls.
        top_fraction: f32,
    },
}

/// How light becomes vigor and vigor becomes shoots.
#[derive(Clone, Debug, PartialEq)]
pub struct Vigor {
    /// How vigor splits at each branch point.
    pub model: VigorModel,
    /// Apical control λ in [0, 1]. Above 0.5 favours the main axis (clear
    /// trunk, excurrent); below 0.5 favours laterals (spreading, decurrent).
    /// Very sensitive near 0.5.
    pub apical_control: f32,
    /// Light exponent κ in Q = b·L^κ. 0 means pure space competition; above 1
    /// sharpens differences so fewer buds activate.
    pub light_exponent: f32,
    /// Scale α turning the root's gathered light into its vigor: v_root = α·Q_root.
    pub root_vigor_scale: f32,
    /// Minimum vigor for a bud with markers to activate.
    pub activation_threshold: f32,
    /// n_max: most internodes a single shoot may add in one step. Shoot
    /// length is n = ⌈v/v_max · n_max⌉, so growth is normalized per step.
    pub max_shoot_length: u32,
}

/// Direction blending and gravitropism.
#[derive(Clone, Debug, PartialEq)]
pub struct Tropism {
    /// Gravitropic set-point angle θ for lateral axes, in radians from
    /// vertical-up: 0 grows up, π/2 grows horizontally, π weeps downward.
    pub lateral_set_point: f32,
    /// Set-point for the main (order 0) axis. `0.0` with plagiotropic
    /// laterals gives conifer forms.
    pub main_set_point: f32,
    // The three weights below are the barycentric (α, β, γ) of
    // H' = αH + βT + γE. They must be non-negative and sum to 1; this is
    // what the UI's triangle widget edits.
    /// Weight α of the current heading H (straight growth).
    pub straightness: f32,
    /// Weight β of the set-point direction T (gravitropism).
    pub gravitropism: f32,
    /// Weight γ of the direction E toward won markers (space colonization).
    pub environment: f32,
}

/// Bud propensity by position on a parent branch (Longay):
/// an ellipse in the plane normal to the parent axis.
#[derive(Clone, Debug, PartialEq)]
pub struct Gravimorphism {
    /// Vertical semi-axis s_v.
    pub vertical_axis: f32,
    /// Horizontal semi-axis s_p.
    pub horizontal_axis: f32,
    /// Vertical offset e_v of the ellipse centre. Positive favours buds on
    /// top of a branch (epitonic), negative underneath (hypotonic).
    pub vertical_offset: f32,
}

/// Space colonization: markers stand for free space that buds compete for.
#[derive(Clone, Debug, PartialEq)]
pub struct SpaceColonization {
    /// Occupancy radius ρ: markers inside it are removed. Sets crown density.
    pub occupancy_radius: f32,
    /// Half-angle θ of each bud's perception cone, in radians.
    pub perception_angle: f32,
    /// Reach r of the perception cone, in world units.
    pub perception_distance: f32,
    /// Markers generated per unit vigor in each bud's cone when growing
    /// autonomously (no brushed markers). 0 disables autonomous growth.
    pub auto_markers_per_vigor: f32,
}

/// Shadow propagation on the light grid (Hart's distance-based variant):
/// each node adds `strength · falloff^-dist` to cells below it.
#[derive(Clone, Debug, PartialEq)]
pub struct Shadow {
    /// Full, unshaded light exposure C.
    pub full_light: f32,
    /// Shadow added by a node to the cell directly below it (a).
    pub strength: f32,
    /// Falloff base β > 1; larger values keep shadow tight to the node.
    pub falloff: f32,
    /// How many cells below a node its shadow pyramid reaches (q_max).
    pub depth: u32,
}

/// Branch shedding (self-pruning).
#[derive(Clone, Debug, PartialEq)]
pub struct Shedding {
    /// Whether branches are shed at all.
    pub enabled: bool,
    /// A branch is shed when its gathered light per internode falls below this.
    pub light_per_internode_threshold: f32,
}

/// Branch radius via the pipe model: rⁿ = Σ r_iⁿ.
#[derive(Clone, Debug, PartialEq)]
pub struct Thickness {
    /// Pipe exponent n, between 2 and 3.
    pub pipe_exponent: f32,
    /// Radius of a terminal internode.
    pub tip_radius: f32,
}

/// Longay (2014 C.1.2) petiole response, independent of numerical resolution.
///
/// The solver normalizes `target`, so its magnitude does not change response.
/// Susceptibilities are non-negative rotation rates per world unit of petiole
/// length. Defaults are illustrative, not calibrated mechanical properties.
#[derive(Clone, Debug, PartialEq)]
pub struct PetioleResponse {
    /// Preferred blade-normal direction T: finite, nonzero and normalizable.
    pub target: glam::Vec3,
    /// eH: susceptibility to twisting about heading H.
    pub twist: f32,
    /// eL: susceptibility to bending about blade-left L.
    pub bend: f32,
    /// eU: susceptibility to turning about blade-normal U.
    pub turn: f32,
}

impl Default for PetioleResponse {
    fn default() -> Self {
        Self {
            target: glam::Vec3::Y,
            twist: 2.0,
            bend: 2.0,
            turn: 2.0,
        }
    }
}

impl PetioleResponse {
    /// Validates response even when foliage is disabled. A zero susceptibility
    /// disables that rotation; all three zero produce a straight petiole.
    pub fn validate(&self) -> Result<(), SpeciesError> {
        if !self.target.is_finite() || self.target.try_normalize().is_none() {
            return Err(SpeciesError::OutOfRange {
                field: "foliage.petiole_response.target",
                value: self.target.length(),
            });
        }
        for (field, value) in [
            ("foliage.petiole_response.twist", self.twist),
            ("foliage.petiole_response.bend", self.bend),
            ("foliage.petiole_response.turn", self.turn),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(SpeciesError::OutOfRange { field, value });
            }
        }
        Ok(())
    }
}

/// Species-specific foliage, independent of branch buds (Longay 2014 C.1.1).
///
/// Distances are world units and angles are radians. Defaults are illustrative
/// broadleaf settings, not measurements of a named species. Petiole response
/// is applied by the engine-neutral rod solver, separately from placement.
#[derive(Clone, Debug, PartialEq)]
pub struct Foliage {
    /// Whether this species produces leaves.
    pub enabled: bool,
    /// Distance between attachment stations along an axis; strictly positive.
    pub spacing: f32,
    /// Leaves attach only where the interpolated branch radius is strictly below this.
    /// Zero disables attachments. This is a radius, not a diameter.
    pub max_radius: f32,
    /// One leaf or an opposite pair at each station, independent of branch pattern.
    pub arrangement: BranchingPattern,
    /// Rotation between attachment stations, independent of branch divergence.
    pub divergence_angle: f32,
    /// Initial petiole angle from the supporting axis, in [0, π].
    pub attachment_angle: f32,
    /// Blade length from base to tip; zero suppresses leaves.
    pub blade_length: f32,
    /// Blade width; zero suppresses leaves.
    pub blade_width: f32,
    /// Petiole rod arc length, non-negative.
    pub petiole_length: f32,
    /// Target orientation and bending/twisting rates for the rod solver.
    pub petiole_response: PetioleResponse,
    /// Engine-neutral appearance key. Image bytes and renderer handles live in the viewer.
    pub appearance: String,
}

impl Default for Foliage {
    fn default() -> Self {
        Self {
            enabled: true,
            spacing: 0.25,
            max_radius: 0.06,
            arrangement: BranchingPattern::Alternate,
            divergence_angle: GOLDEN_ANGLE,
            attachment_angle: 60f32.to_radians(),
            blade_length: 4.0 * 0.0254,
            blade_width: 4.0 * 0.0254,
            petiole_length: 0.08,
            petiole_response: PetioleResponse::default(),
            appearance: "generic-broadleaf".into(),
        }
    }
}

/// Everything that makes one kind of tree grow differently from another.
///
/// Immutable once a tree is created. Build one by starting from a preset
/// ([`TreeSpecies::default`], [`TreeSpecies::conifer`]) and changing fields,
/// then call [`TreeSpecies::validate`] before sharing it.
#[derive(Clone, Debug, PartialEq)]
pub struct TreeSpecies {
    /// Display name, for the UI and error messages.
    pub name: String,
    /// Phyllotaxis and branch angles.
    pub architecture: Architecture,
    /// Light-to-vigor allocation and shoot growth.
    pub vigor: Vigor,
    /// Shoot direction blending.
    pub tropism: Tropism,
    /// Bud propensity around a parent branch.
    pub gravimorphism: Gravimorphism,
    /// Space colonization markers and perception.
    pub space: SpaceColonization,
    /// Shadow propagation on the light grid.
    pub shadow: Shadow,
    /// Self-pruning of shaded branches.
    pub shedding: Shedding,
    /// Pipe-model branch radii.
    pub thickness: Thickness,
    /// Leaf placement and initial blade dimensions.
    pub foliage: Foliage,
}

/// The golden angle, 360°·(1 − 1/φ), in radians.
pub const GOLDEN_ANGLE: f32 = 2.399_963;

impl Default for TreeSpecies {
    /// A generic spreading broadleaf tree, using the defaults the papers
    /// suggest (golden-angle phyllotaxis, ρ ≈ 2 internodes, θ ≈ 90° cone).
    fn default() -> Self {
        let internode = 1.0;
        TreeSpecies {
            name: "Generic broadleaf".to_owned(),
            architecture: Architecture {
                branching: BranchingPattern::Alternate,
                divergence_angle: GOLDEN_ANGLE,
                branching_angle: 45f32.to_radians(),
                deflection_angle: 5f32.to_radians(),
                internode_length: internode,
            },
            vigor: Vigor {
                model: VigorModel::BorchertHonda,
                apical_control: 0.52,
                light_exponent: 1.5,
                root_vigor_scale: 2.0,
                activation_threshold: 0.1,
                max_shoot_length: 3,
            },
            tropism: Tropism {
                lateral_set_point: 60f32.to_radians(),
                main_set_point: 0.0,
                straightness: 0.5,
                gravitropism: 0.2,
                environment: 0.3,
            },
            gravimorphism: Gravimorphism {
                vertical_axis: 1.0,
                horizontal_axis: 1.0,
                vertical_offset: 0.0,
            },
            space: SpaceColonization {
                occupancy_radius: 2.0 * internode,
                perception_angle: 45f32.to_radians(),
                perception_distance: 5.0 * internode,
                auto_markers_per_vigor: 8.0,
            },
            shadow: Shadow {
                full_light: 1.0,
                strength: 0.2,
                falloff: 1.5,
                depth: 6,
            },
            shedding: Shedding {
                enabled: true,
                light_per_internode_threshold: 0.05,
            },
            foliage: Foliage::default(),
            thickness: Thickness {
                pipe_exponent: 2.5,
                tip_radius: 0.02,
            },
        }
    }
}

impl TreeSpecies {
    /// An excurrent conifer: strong apical control, an upright main axis and
    /// near-horizontal laterals.
    #[allow(clippy::field_reassign_with_default)]
    pub fn conifer() -> Self {
        let mut s = TreeSpecies::default();
        s.name = "Generic conifer".to_owned();
        // Needle arrangements and templates are a later feature.
        s.foliage.enabled = false;
        s.architecture.branching_angle = 70f32.to_radians();
        s.architecture.deflection_angle = 0.0;
        s.vigor.apical_control = 0.58;
        s.vigor.light_exponent = 2.0;
        s.tropism.main_set_point = 0.0;
        s.tropism.lateral_set_point = 95f32.to_radians();
        s.tropism.straightness = 0.6;
        s.tropism.gravitropism = 0.3;
        s.tropism.environment = 0.1;
        s.thickness.pipe_exponent = 2.2;
        s
    }

    /// Checks ranges and invariants that the simulator relies on.
    ///
    /// Every `f32` must be finite. Angles are in radians; directions measured
    /// from an axis (ψ, δ, θ, the set-points) lie in [0, π].
    pub fn validate(&self) -> Result<(), SpeciesError> {
        use std::f32::consts::PI;
        let check = |field: &'static str, value: f32, ok: bool| {
            if ok && value.is_finite() {
                Ok(())
            } else {
                Err(SpeciesError::OutOfRange { field, value })
            }
        };
        let positive = |field, value: f32| check(field, value, value > 0.0);
        let non_negative = |field, value: f32| check(field, value, value >= 0.0);
        let angle = |field, value: f32| check(field, value, (0.0..=PI).contains(&value));

        let a = &self.architecture;
        check("architecture.divergence_angle", a.divergence_angle, true)?;
        angle("architecture.branching_angle", a.branching_angle)?;
        angle("architecture.deflection_angle", a.deflection_angle)?;
        positive("architecture.internode_length", a.internode_length)?;

        let v = &self.vigor;
        if let VigorModel::Priority {
            w_max,
            w_min,
            top_fraction,
        } = v.model
        {
            non_negative("vigor.model.w_min", w_min)?;
            check("vigor.model.w_max", w_max, w_max >= w_min)?;
            check(
                "vigor.model.top_fraction",
                top_fraction,
                top_fraction > 0.0 && top_fraction <= 1.0,
            )?;
        }
        let lambda = v.apical_control;
        check(
            "vigor.apical_control",
            lambda,
            (0.0..=1.0).contains(&lambda),
        )?;
        non_negative("vigor.light_exponent", v.light_exponent)?;
        positive("vigor.root_vigor_scale", v.root_vigor_scale)?;
        non_negative("vigor.activation_threshold", v.activation_threshold)?;
        if v.max_shoot_length == 0 {
            return Err(SpeciesError::OutOfRange {
                field: "vigor.max_shoot_length",
                value: 0.0,
            });
        }

        let t = &self.tropism;
        angle("tropism.lateral_set_point", t.lateral_set_point)?;
        angle("tropism.main_set_point", t.main_set_point)?;
        let sum = t.straightness + t.gravitropism + t.environment;
        if [t.straightness, t.gravitropism, t.environment]
            .iter()
            .any(|w| !w.is_finite() || *w < 0.0)
            || (sum - 1.0).abs() > 1e-4
        {
            return Err(SpeciesError::TropismWeights { sum });
        }

        let g = &self.gravimorphism;
        positive("gravimorphism.vertical_axis", g.vertical_axis)?;
        positive("gravimorphism.horizontal_axis", g.horizontal_axis)?;
        check("gravimorphism.vertical_offset", g.vertical_offset, true)?;

        let s = &self.space;
        positive("space.occupancy_radius", s.occupancy_radius)?;
        // A half-angle of π would make the cone the whole sphere.
        check(
            "space.perception_angle",
            s.perception_angle,
            s.perception_angle > 0.0 && s.perception_angle < PI,
        )?;
        check(
            "space.perception_distance",
            s.perception_distance,
            s.perception_distance > s.occupancy_radius,
        )?;
        non_negative("space.auto_markers_per_vigor", s.auto_markers_per_vigor)?;

        let sh = &self.shadow;
        positive("shadow.full_light", sh.full_light)?;
        non_negative("shadow.strength", sh.strength)?;
        check("shadow.falloff", sh.falloff, sh.falloff > 1.0)?;

        non_negative(
            "shedding.light_per_internode_threshold",
            self.shedding.light_per_internode_threshold,
        )?;

        let n = self.thickness.pipe_exponent;
        check("thickness.pipe_exponent", n, (2.0..=3.0).contains(&n))?;
        positive("thickness.tip_radius", self.thickness.tip_radius)?;
        let f = &self.foliage;
        positive("foliage.spacing", f.spacing)?;
        non_negative("foliage.max_radius", f.max_radius)?;
        check("foliage.divergence_angle", f.divergence_angle, true)?;
        angle("foliage.attachment_angle", f.attachment_angle)?;
        non_negative("foliage.blade_length", f.blade_length)?;
        non_negative("foliage.blade_width", f.blade_width)?;
        non_negative("foliage.petiole_length", f.petiole_length)?;
        f.petiole_response.validate()?;
        Ok(())
    }
}

/// Why a [`TreeSpecies`] failed validation.
#[derive(Clone, Debug, PartialEq)]
pub enum SpeciesError {
    /// A parameter is outside its documented range, or is not finite.
    OutOfRange {
        /// Path of the parameter, e.g. `"vigor.apical_control"`.
        field: &'static str,
        /// The rejected value.
        value: f32,
    },
    /// The barycentric direction weights must be non-negative and sum to 1.
    TropismWeights {
        /// The sum of the three weights as given.
        sum: f32,
    },
}

impl fmt::Display for SpeciesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SpeciesError::OutOfRange { field, value } => write!(f, "{field} out of range: {value}"),
            SpeciesError::TropismWeights { sum } => {
                write!(
                    f,
                    "tropism weights must be non-negative and sum to 1 (sum is {sum})"
                )
            }
        }
    }
}

impl std::error::Error for SpeciesError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn golden_angle_is_two_pi_over_phi_squared() {
        let phi = (1.0 + 5f64.sqrt()) / 2.0;
        let golden = std::f64::consts::TAU * (1.0 - 1.0 / phi);
        assert!((GOLDEN_ANGLE as f64 - golden).abs() < 1e-6);
        assert!((GOLDEN_ANGLE.to_degrees() - 137.508).abs() < 1e-3);
    }

    #[test]
    fn presets_are_valid_and_follow_the_papers() {
        for s in [TreeSpecies::default(), TreeSpecies::conifer()] {
            assert_eq!(s.validate(), Ok(()), "{}", s.name);
            let l = s.architecture.internode_length;
            // Pałubicki 2009: ρ ≈ 2 internodes, r ≈ 4–6 internodes, θ ≈ 90° cone.
            assert!((s.space.occupancy_radius / l - 2.0).abs() < 1e-6);
            assert!((4.0..=6.0).contains(&(s.space.perception_distance / l)));
            assert!((s.space.perception_angle.to_degrees() * 2.0 - 90.0).abs() < 1e-3);
        }
        // Conifers are excurrent (λ > 0.5) and more so than the broadleaf.
        let (b, c) = (TreeSpecies::default(), TreeSpecies::conifer());
        assert!(c.vigor.apical_control > 0.5);
        assert!(c.vigor.apical_control > b.vigor.apical_control);
        assert!(c.tropism.lateral_set_point > b.tropism.lateral_set_point);
    }

    #[test]
    fn validation_enforces_parameter_ranges() {
        use std::f32::consts::PI;
        let bad: &[fn(&mut TreeSpecies)] = &[
            |s| s.foliage.spacing = 0.0,
            |s| s.foliage.spacing = -1.0,
            |s| s.foliage.spacing = f32::NAN,
            |s| s.foliage.spacing = f32::INFINITY,
            |s| s.foliage.max_radius = -0.1,
            |s| s.foliage.max_radius = f32::INFINITY,
            |s| s.foliage.divergence_angle = f32::NAN,
            |s| s.foliage.attachment_angle = PI + 0.1,
            |s| s.foliage.attachment_angle = -0.1,
            |s| s.foliage.blade_length = -0.1,
            |s| s.foliage.blade_length = f32::NAN,
            |s| s.foliage.blade_width = -0.1,
            |s| s.foliage.blade_width = f32::INFINITY,
            |s| s.foliage.petiole_length = -0.1,
            |s| s.foliage.petiole_length = f32::NAN,
            |s| s.architecture.divergence_angle = f32::INFINITY,
            |s| s.architecture.branching_angle = -0.1,
            |s| s.architecture.branching_angle = PI + 0.1,
            |s| s.architecture.deflection_angle = f32::NAN,
            |s| s.architecture.internode_length = 0.0,
            |s| s.vigor.apical_control = 1.01,
            |s| s.vigor.apical_control = -0.01,
            |s| s.vigor.light_exponent = -1.0,
            |s| s.vigor.max_shoot_length = 0,
            |s| s.thickness.pipe_exponent = 1.9,
            |s| s.thickness.pipe_exponent = 3.1,
            |s| s.shadow.falloff = 1.0,
            |s| s.space.perception_distance = s.space.occupancy_radius,
            |s| s.architecture.internode_length = f32::NAN,
            |s| s.vigor.root_vigor_scale = 0.0,
            |s| s.vigor.activation_threshold = -0.1,
            |s| {
                s.vigor.model = VigorModel::Priority {
                    w_max: 0.5,
                    w_min: 1.0,
                    top_fraction: 0.5,
                }
            },
            |s| {
                s.vigor.model = VigorModel::Priority {
                    w_max: 1.0,
                    w_min: -0.1,
                    top_fraction: 0.5,
                }
            },
            |s| {
                s.vigor.model = VigorModel::Priority {
                    w_max: 1.0,
                    w_min: 0.0,
                    top_fraction: 0.0,
                }
            },
            |s| s.tropism.lateral_set_point = PI + 0.1,
            |s| s.tropism.main_set_point = -0.1,
            |s| s.gravimorphism.vertical_axis = 0.0,
            |s| s.gravimorphism.horizontal_axis = -1.0,
            |s| s.gravimorphism.vertical_offset = f32::NAN,
            |s| s.space.perception_angle = 0.0,
            |s| s.space.perception_angle = PI,
            |s| s.space.auto_markers_per_vigor = -1.0,
            |s| s.shadow.full_light = 0.0,
            |s| s.shadow.strength = -0.1,
            |s| s.shedding.light_per_internode_threshold = -0.1,
        ];
        for (i, edit) in bad.iter().enumerate() {
            let mut s = TreeSpecies::default();
            edit(&mut s);
            assert!(s.validate().is_err(), "edit {i} should be rejected");
        }
        // The documented range ends are allowed.
        let mut s = TreeSpecies::default();
        s.vigor.apical_control = 0.0;
        s.thickness.pipe_exponent = 3.0;
        s.architecture.branching_angle = PI;
        s.tropism.lateral_set_point = PI;
        s.space.auto_markers_per_vigor = 0.0;
        s.vigor.model = VigorModel::Priority {
            w_max: 1.0,
            w_min: 1.0,
            top_fraction: 1.0,
        };
        assert_eq!(s.validate(), Ok(()));
    }

    #[test]
    fn foliage_range_endpoints_and_disabled_validation() {
        let mut s = TreeSpecies::default();
        s.foliage.max_radius = 0.0;
        s.foliage.blade_width = 0.0;
        s.foliage.blade_length = 0.0;
        s.foliage.petiole_length = 0.0;
        s.foliage.attachment_angle = std::f32::consts::PI;
        s.foliage.divergence_angle = -1.0;
        assert_eq!(s.validate(), Ok(()));
        s.foliage.enabled = false;
        s.foliage.spacing = 0.0;
        assert!(matches!(
            s.validate(),
            Err(SpeciesError::OutOfRange {
                field: "foliage.spacing",
                ..
            })
        ));
    }

    #[test]
    fn validates_species_petiole_response_even_when_foliage_is_disabled() {
        let bad: &[fn(&mut PetioleResponse)] = &[
            |r| r.target = glam::Vec3::ZERO,
            |r| r.target.x = f32::NAN,
            |r| r.target.y = f32::INFINITY,
            |r| r.twist = -1.0,
            |r| r.twist = f32::NAN,
            |r| r.bend = -1.0,
            |r| r.bend = f32::INFINITY,
            |r| r.turn = -1.0,
            |r| r.turn = f32::NAN,
        ];
        for edit in bad {
            let mut s = TreeSpecies::default();
            s.foliage.enabled = false;
            edit(&mut s.foliage.petiole_response);
            assert!(s.validate().is_err());
        }
        let mut s = TreeSpecies::default();
        s.foliage.petiole_response = PetioleResponse {
            target: glam::Vec3::Y * 8.0,
            twist: 0.0,
            bend: 0.0,
            turn: 0.0,
        };
        assert_eq!(s.validate(), Ok(()));
    }

    #[test]
    fn tropism_weights_must_be_barycentric() {
        let mut s = TreeSpecies::default();
        (
            s.tropism.straightness,
            s.tropism.gravitropism,
            s.tropism.environment,
        ) = (1.0, 0.0, 0.0);
        assert_eq!(s.validate(), Ok(()));
        // Sums to 1 but has a negative weight.
        (
            s.tropism.straightness,
            s.tropism.gravitropism,
            s.tropism.environment,
        ) = (1.2, -0.2, 0.0);
        assert!(matches!(
            s.validate(),
            Err(SpeciesError::TropismWeights { .. })
        ));
        // NaN compares false both ways, so it must not slip through.
        (
            s.tropism.straightness,
            s.tropism.gravitropism,
            s.tropism.environment,
        ) = (f32::NAN, 0.0, 0.0);
        assert!(matches!(
            s.validate(),
            Err(SpeciesError::TropismWeights { .. })
        ));
    }
}
