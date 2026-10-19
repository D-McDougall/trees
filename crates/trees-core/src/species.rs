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
        w_max: f32,
        w_min: f32,
        top_fraction: f32,
    },
}

/// How light becomes vigor and vigor becomes shoots.
#[derive(Clone, Debug, PartialEq)]
pub struct Vigor {
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
    /// Barycentric weights (α, β, γ) of H' = αH + βT + γE: keep the current
    /// heading, turn toward the set-point, head toward won markers. Must sum
    /// to 1; this is what the UI's triangle widget edits.
    pub straightness: f32,
    pub gravitropism: f32,
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

/// Everything that makes one kind of tree grow differently from another.
///
/// Immutable once a tree is created. Build one by starting from a preset
/// ([`TreeSpecies::default`], [`TreeSpecies::conifer`]) and changing fields,
/// then call [`TreeSpecies::validate`] before sharing it.
#[derive(Clone, Debug, PartialEq)]
pub struct TreeSpecies {
    pub name: String,
    pub architecture: Architecture,
    pub vigor: Vigor,
    pub tropism: Tropism,
    pub gravimorphism: Gravimorphism,
    pub space: SpaceColonization,
    pub shadow: Shadow,
    pub shedding: Shedding,
    pub thickness: Thickness,
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
    pub fn validate(&self) -> Result<(), SpeciesError> {
        use SpeciesError::OutOfRange;
        let check = |field: &'static str, value: f32, ok: bool| {
            if ok && value.is_finite() {
                Ok(())
            } else {
                Err(OutOfRange { field, value })
            }
        };
        let a = &self.architecture;
        check(
            "architecture.internode_length",
            a.internode_length,
            a.internode_length > 0.0,
        )?;
        let v = &self.vigor;
        check(
            "vigor.apical_control",
            v.apical_control,
            (0.0..=1.0).contains(&v.apical_control),
        )?;
        check(
            "vigor.light_exponent",
            v.light_exponent,
            v.light_exponent >= 0.0,
        )?;
        if v.max_shoot_length == 0 {
            return Err(OutOfRange {
                field: "vigor.max_shoot_length",
                value: 0.0,
            });
        }
        let t = &self.tropism;
        let sum = t.straightness + t.gravitropism + t.environment;
        if [t.straightness, t.gravitropism, t.environment]
            .iter()
            .any(|w| *w < 0.0)
            || (sum - 1.0).abs() > 1e-4
        {
            return Err(SpeciesError::TropismWeights { sum });
        }
        let s = &self.space;
        check(
            "space.occupancy_radius",
            s.occupancy_radius,
            s.occupancy_radius > 0.0,
        )?;
        check(
            "space.perception_distance",
            s.perception_distance,
            s.perception_distance > s.occupancy_radius,
        )?;
        check(
            "shadow.falloff",
            self.shadow.falloff,
            self.shadow.falloff > 1.0,
        )?;
        let n = self.thickness.pipe_exponent;
        check("thickness.pipe_exponent", n, (2.0..=3.0).contains(&n))?;
        check(
            "thickness.tip_radius",
            self.thickness.tip_radius,
            self.thickness.tip_radius > 0.0,
        )?;
        Ok(())
    }
}

/// Why a [`TreeSpecies`] failed validation.
#[derive(Clone, Debug, PartialEq)]
pub enum SpeciesError {
    OutOfRange {
        field: &'static str,
        value: f32,
    },
    /// The barycentric direction weights must be non-negative and sum to 1.
    TropismWeights {
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
