//! End-to-end steering checks using public sensing/growth and tree APIs.
use glam::Vec3;
use std::{
    f32::consts::{FRAC_PI_2, PI},
    sync::Arc,
};
use trees_core::{
    Bud, BudFate, Habitat, OpenAir, SamplingConfig, Slot, Tree, TreeSpecies, grow_step,
};

const SAMPLING: SamplingConfig = SamplingConfig {
    cell_size: 0.75,
    jitter: 0.25,
};

fn species(main: f32, lateral: f32, beta: f32) -> TreeSpecies {
    let mut s = TreeSpecies::default();
    s.space.occupancy_radius = 0.25;
    s.space.perception_distance = 3.0;
    s.space.perception_angle = FRAC_PI_2;
    s.tropism.main_set_point = main;
    s.tropism.lateral_set_point = lateral;
    s.tropism.straightness = 0.0;
    s.tropism.gravitropism = beta;
    s.tropism.environment = 1.0 - beta;
    s
}

fn preview(s: TreeSpecies) -> Tree {
    let mut t = Tree::new(Arc::new(s), 42, Vec3::ZERO).unwrap();
    for _ in 0..6 {
        grow_step(&mut t, &OpenAir, SAMPLING).unwrap();
        t.update_radii();
    }
    t
}

fn geometry(t: &Tree) -> Vec<(Vec3, Vec3, Vec3, f32, u16)> {
    t.iter()
        .map(|(_, n)| (n.position, n.heading, n.lateral, n.radius, n.order))
        .collect()
}

#[test]
fn representative_previews_replay_with_finite_geometry_and_valid_frames() {
    for s in [
        species(0.0, FRAC_PI_2, 0.0),
        species(0.0, 0.0, 1.0),
        species(0.0, FRAC_PI_2, 1.0),
        species(PI, PI, 1.0),
    ] {
        let t = preview(s.clone());
        assert!(
            t.len() > 4,
            "fixture must exercise branching over multiple steps"
        );
        assert_eq!(geometry(&t), geometry(&preview(s)));
        assert_eq!(t.step(), 6);
        for (_, n) in t.iter() {
            assert!(n.position.is_finite() && n.radius.is_finite());
            assert!(n.heading.is_finite() && n.lateral.is_finite());
            assert!((n.heading.length() - 1.0).abs() < 1e-5);
            assert!((n.lateral.length() - 1.0).abs() < 1e-5);
            assert!(n.heading.dot(n.lateral).abs() < 1e-5);
            if let Some(parent) = n.parent {
                let segment = n.position - t.node(parent).position;
                assert!((segment - n.heading * n.length).length() < 1e-5);
                assert_eq!(n.length, t.species().architecture.internode_length);
            }
        }
    }
}

#[test]
fn main_up_and_laterals_horizontal_include_first_and_continuing_internodes() {
    let t = preview(species(0.0, FRAC_PI_2, 1.0));
    let mut first_lateral = 0;
    let mut continuing_lateral = 0;
    let mut main = 0;
    for (_, n) in t.iter() {
        let Some(parent) = n.parent else { continue };
        if n.order == 0 {
            main += 1;
            assert!((n.heading - Vec3::Y).length() < 1e-6);
        } else {
            assert!(n.heading.y.abs() < 1e-6);
            if t.node(parent).order < n.order {
                first_lateral += 1;
            } else {
                continuing_lateral += 1;
            }
        }
    }
    assert!(main > 1 && first_lateral > 0 && continuing_lateral > 0);
}

#[test]
fn set_points_change_geometry_only_when_gravitropism_has_weight() {
    let environment = geometry(&preview(species(0.0, 0.0, 0.0)));
    assert_eq!(environment, geometry(&preview(species(PI, PI, 0.0))));
    let upright = geometry(&preview(species(0.0, 0.0, 1.0)));
    let horizontal = geometry(&preview(species(0.0, FRAC_PI_2, 1.0)));
    let down_tree = preview(species(PI, PI, 1.0));
    let downward = geometry(&down_tree);
    assert_ne!(environment, upright);
    assert_ne!(upright, horizontal);
    assert_ne!(horizontal, downward);
    for (_, n) in down_tree.iter().filter(|(_, n)| n.parent.is_some()) {
        assert_eq!(n.heading, -Vec3::Y);
    }
}

#[test]
fn enabling_tropism_steers_new_wood_without_moving_existing_wood() {
    let mut t = preview(species(0.0, 0.0, 0.0));
    let old: Vec<_> = t
        .iter()
        .map(|(id, n)| (id, n.position, n.heading, n.lateral, n.length, n.birth_step))
        .collect();
    let changed = Arc::new(species(PI, PI, 1.0));
    let ids: Vec<_> = t.iter().map(|(id, _)| id).collect();
    for id in ids {
        t.node_mut(id).species = changed.clone();
    }
    let added = grow_step(&mut t, &OpenAir, SAMPLING).unwrap();
    assert!(!added.is_empty());
    for id in added {
        assert_eq!(t.node(id).heading, -Vec3::Y);
        assert_eq!(t.node(id).birth_step, 6);
    }
    for (id, position, heading, lateral, length, birth_step) in old {
        let n = t.node(id);
        assert_eq!(
            (n.position, n.heading, n.lateral, n.length, n.birth_step),
            (position, heading, lateral, length, birth_step)
        );
    }
}

#[test]
fn zero_environment_weight_still_requires_markers_and_cancellation_stays_dormant() {
    struct Solid;
    impl Habitat for Solid {
        fn occupied(&self, _: Vec3) -> bool {
            true
        }
    }
    let mut t = Tree::new(Arc::new(species(0.0, FRAC_PI_2, 1.0)), 42, Vec3::ZERO).unwrap();
    assert!(grow_step(&mut t, &Solid, SAMPLING).unwrap().is_empty());
    assert!(
        t.node(t.root())
            .buds()
            .all(|(_, b)| b.fate == BudFate::Dormant)
    );
    assert_eq!(t.step(), 1);

    let mut s = species(PI, PI, 0.5);
    s.tropism.straightness = 0.5;
    s.tropism.environment = 0.0;
    let mut t = Tree::new(Arc::new(s), 42, Vec3::ZERO).unwrap();
    let root = t.root();
    t.node_mut(root).main = Slot::Bud(Bud::new(Vec3::Y));
    t.node_mut(root).laterals = [Slot::Empty, Slot::Empty];
    assert!(grow_step(&mut t, &OpenAir, SAMPLING).unwrap().is_empty());
    let b = t.node(root).main.bud().unwrap();
    assert!(b.marker_count > 0);
    assert!(b.environment_direction().is_some());
    assert_eq!(b.fate, BudFate::Dormant);
    assert_eq!(t.len(), 1);
    assert_eq!(t.step(), 1);
}
