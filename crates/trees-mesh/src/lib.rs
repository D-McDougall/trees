//! Geometry builder: converts a snapshot of a [`trees_core::Tree`] into
//! engine-neutral branch meshes, leaf instances and levels of detail.
//!
//! See "Geometry generation" in the design document. Output is plain vertex
//! and index buffers ([`TreeMesh`]) so any engine can upload them.
//!
//! Two styles are implemented: [`MeshStyle::Wireframe`] (one line per
//! internode) and [`MeshStyle::Tubes`] (generalized cylinders sized by each
//! node's radius, swept along straight segments).

use glam::{Quat, Vec3};
use trees_core::{NodeId, Tree, TreeNode};

/// How a tree is turned into geometry.
///
/// Placeholder until the configuration system is designed; for now it only
/// picks the style.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshOptions {
    pub style: MeshStyle,
}

/// The kind of geometry to build. More styles (curved tubes, foliage)
/// will be added, so match on it with a wildcard arm.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[non_exhaustive]
pub enum MeshStyle {
    /// The skeleton: each internode drawn as a line from its base to its tip.
    #[default]
    Wireframe,
    /// Solid branches: a ring of each node's radius at every node, joined by
    /// straight segments, with every branch tip capped.
    Tubes(TubeOptions),
}

/// Settings for [`MeshStyle::Tubes`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TubeOptions {
    /// Longay 2014 Eq. C.2: a branch whose base has radius r gets
    /// C = 3·(r / r_tip)^(1/n) sides, so the thinnest twigs are triangles.
    /// n = 1 keeps the arc each side covers constant; larger n gives thick
    /// branches fewer sides. Must be positive.
    pub sides_exponent: f32,
}

impl Default for TubeOptions {
    fn default() -> Self {
        TubeOptions {
            sides_exponent: 2.0,
        }
    }
}

/// How [`TreeMesh::indices`] group into primitives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Topology {
    /// Every two indices form a line segment.
    LineList,
    /// Every three indices form a triangle, counter-clockwise seen from its
    /// front (outside) face.
    TriangleList,
}

/// Engine-neutral geometry in world space.
#[derive(Clone, Debug, PartialEq)]
pub struct TreeMesh {
    pub positions: Vec<[f32; 3]>,
    /// Unit normal per vertex, or empty when the style has none (wireframe).
    pub normals: Vec<[f32; 3]>,
    /// Texture coordinates per vertex, or empty when the style has none.
    /// For tubes, u runs 0 to 1 around the branch and v is the arc length
    /// from the start of the branch, in world units.
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    pub topology: Topology,
}

impl TreeMesh {
    fn new(topology: Topology) -> Self {
        TreeMesh {
            positions: Vec::new(),
            normals: Vec::new(),
            uvs: Vec::new(),
            indices: Vec::new(),
            topology,
        }
    }

    /// Appends a vertex with a normal and texture coordinate; returns its index.
    fn push_vertex(&mut self, position: Vec3, normal: Vec3, uv: [f32; 2]) -> u32 {
        self.positions.push(position.to_array());
        self.normals.push(normal.to_array());
        self.uvs.push(uv);
        (self.positions.len() - 1) as u32
    }
}

/// Builds a mesh of `tree` in the style chosen by `options`.
pub fn build_mesh(tree: &Tree, options: &MeshOptions) -> TreeMesh {
    match options.style {
        MeshStyle::Wireframe => wireframe(tree),
        MeshStyle::Tubes(tube) => tubes(tree, &tube),
    }
}

/// One vertex at each node's tip plus one at the root's base, and one line
/// per internode joining its base (the parent's vertex) to its tip.
fn wireframe(tree: &Tree) -> TreeMesh {
    let mut mesh = TreeMesh::new(Topology::LineList);
    mesh.positions.reserve(tree.len() + 1);
    mesh.indices.reserve(tree.len() * 2);
    // Vertex of each node, indexed by `NodeId::index()`. Ids of pruned nodes
    // stay `None`; `Tree::iter` visits parents first, so lookups always hit.
    let mut vertex_of: Vec<Option<u32>> = Vec::new();
    let push = |mesh: &mut TreeMesh, p: Vec3| {
        mesh.positions.push(p.to_array());
        (mesh.positions.len() - 1) as u32
    };

    for (id, node) in tree.iter() {
        let base = match node.parent {
            Some(parent) => vertex_of[parent.index()].expect("parent visited before child"),
            None => push(&mut mesh, node.base()),
        };
        let tip = push(&mut mesh, node.position);
        mesh.indices.extend([base, tip]);
        if vertex_of.len() <= id.index() {
            vertex_of.resize(id.index() + 1, None);
        }
        vertex_of[id.index()] = Some(tip);
    }
    mesh
}

/// The child that continues `node`'s axis: always the one in its main slot,
/// whatever the radii. The apical continuation is fixed by the tree, not
/// inferred from thickness, so a lateral never takes over: when the main
/// child is shed the axis runs into its stump and ends there, and when it
/// is pruned the axis ends at `node`. Either way the laterals keep the
/// shape they had.
fn continuation(node: &TreeNode) -> Option<NodeId> {
    node.main.child()
}

/// Splits the tree into axes (design document, geometry pass 1): each axis
/// is a chain of nodes joined through their main slots, from base to tip,
/// and every node is on exactly one axis. Every lateral starts a new axis.
fn axes(tree: &Tree) -> Vec<Vec<NodeId>> {
    let mut axes = Vec::new();
    for (id, node) in tree.iter() {
        let continues_parent = node
            .parent
            .is_some_and(|p| continuation(tree.node(p)) == Some(id));
        if continues_parent {
            continue;
        }
        let mut axis = vec![id];
        let mut cur = node;
        while let Some(next) = continuation(cur) {
            axis.push(next);
            cur = tree.node(next);
        }
        axes.push(axis);
    }
    axes
}

/// Longay 2014 Eq. C.2: sides of a branch whose base has `radius`, where
/// `tip_radius` gets 3 sides. Not capped above, as in the paper.
fn side_count(radius: f32, tip_radius: f32, sides_exponent: f32) -> u32 {
    let sides = 3.0 * (radius / tip_radius).powf(sides_exponent.recip());
    (sides.round() as u32).max(3)
}

/// One generalized cylinder per axis from [`axes`]. Each axis has a ring at
/// its base (radius of its first node) and one at every node's tip (that
/// node's radius), all with the side count of its base. Rings sit
/// perpendicular to the average of the headings meeting there and are
/// oriented by parallel transport so the tube does not twist. Every axis tip
/// is closed with a flat cap; axis bases are left open, since they start
/// inside the parent branch (or the ground, for the trunk).
fn tubes(tree: &Tree, options: &TubeOptions) -> TreeMesh {
    let mut mesh = TreeMesh::new(Topology::TriangleList);
    for axis in axes(tree) {
        let nodes: Vec<&TreeNode> = axis.iter().map(|&id| tree.node(id)).collect();
        let first = nodes[0];
        let sides = side_count(
            first.radius,
            first.species.thickness.tip_radius,
            options.sides_exponent,
        );

        // Ring k sits at the base of nodes[k] (k = 0) or the tip of
        // nodes[k - 1]; its tangent is the bisector of the headings there.
        let mut rings: Vec<(Vec3, Vec3, f32)> = vec![(first.base(), first.heading, first.radius)];
        for (k, node) in nodes.iter().enumerate() {
            let tangent = match nodes.get(k + 1) {
                Some(next) => (node.heading + next.heading)
                    .try_normalize()
                    .unwrap_or(node.heading),
                None => node.heading,
            };
            rings.push((node.position, tangent, node.radius));
        }

        let mut normal = first.lateral;
        let mut prev_tangent = first.heading;
        let mut arc_length = 0.0;
        let mut prev_ring: Option<u32> = None;
        for (k, &(center, tangent, radius)) in rings.iter().enumerate() {
            // Parallel transport: carry the reference normal by the rotation
            // between successive tangents, then re-orthonormalize.
            normal = Quat::from_rotation_arc(prev_tangent, tangent) * normal;
            normal = (normal - tangent * normal.dot(tangent))
                .try_normalize()
                .unwrap_or_else(|| tangent.any_orthonormal_vector());
            prev_tangent = tangent;
            let binormal = tangent.cross(normal);
            if k > 0 {
                arc_length += nodes[k - 1].length;
            }

            // sides + 1 vertices: the seam is duplicated so u can reach 1.
            let start = mesh.positions.len() as u32;
            for j in 0..=sides {
                let u = j as f32 / sides as f32;
                let (sin, cos) = (u * std::f32::consts::TAU).sin_cos();
                let radial = cos * normal + sin * binormal;
                mesh.push_vertex(center + radius * radial, radial, [u, arc_length]);
            }
            if let Some(below) = prev_ring {
                for j in 0..sides {
                    let (a0, a1) = (below + j, below + j + 1);
                    let (b0, b1) = (start + j, start + j + 1);
                    mesh.indices.extend([a0, a1, b1, a0, b1, b0]);
                }
            }
            prev_ring = Some(start);
        }

        // Flat cap: its own vertices, so it shades flat against the tube.
        let &(center, tangent, _) = rings.last().expect("an axis has rings");
        let ring = prev_ring.expect("an axis has rings");
        let hub = mesh.push_vertex(center, tangent, [0.5, arc_length]);
        let rim = mesh.positions.len() as u32;
        for j in 0..sides {
            let p = Vec3::from_array(mesh.positions[(ring + j) as usize]);
            mesh.push_vertex(p, tangent, [0.5, arc_length]);
        }
        for j in 0..sides {
            mesh.indices.extend([hub, rim + j, rim + (j + 1) % sides]);
        }
    }
    mesh
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use trees_core::{OpenAir, SlotIndex, TreeSpecies};

    use super::*;

    fn seedling() -> Tree {
        Tree::new(Arc::new(TreeSpecies::default()), 42, Vec3::ZERO).unwrap()
    }

    fn lines(mesh: &TreeMesh) -> Vec<(Vec3, Vec3)> {
        mesh.indices
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&[a, b]| {
                let p = |i: u32| Vec3::from_array(mesh.positions[i as usize]);
                (p(a), p(b))
            })
            .collect()
    }

    #[test]
    fn seedling_is_one_line_from_origin() {
        let t = seedling();
        let mesh = build_mesh(&t, &MeshOptions::default());
        assert_eq!(mesh.topology, Topology::LineList);
        assert_eq!(mesh.positions.len(), 2);
        assert_eq!(lines(&mesh), vec![(Vec3::ZERO, t.node(t.root()).position)]);
    }

    #[test]
    fn each_internode_joins_parent_tip_to_its_tip() {
        let mut t = seedling();
        let root = t.root();
        let a = t
            .grow(&OpenAir, root, SlotIndex::Main, Vec3::Y, 1.0)
            .unwrap();
        let b = t
            .grow(&OpenAir, root, SlotIndex::Lateral(0), Vec3::X, 1.0)
            .unwrap();
        let c = t.grow(&OpenAir, a, SlotIndex::Main, Vec3::Y, 1.0).unwrap();
        let mesh = build_mesh(&t, &MeshOptions::default());

        // Shared vertices: one per node plus the root's base.
        assert_eq!(mesh.positions.len(), t.len() + 1);
        let pos = |id| t.node(id).position;
        assert_eq!(
            lines(&mesh),
            vec![
                (Vec3::ZERO, pos(root)),
                (pos(root), pos(a)),
                (pos(root), pos(b)),
                (pos(a), pos(c)),
            ]
        );
    }

    #[test]
    fn pruned_nodes_leave_no_geometry() {
        let mut t = seedling();
        let root = t.root();
        let a = t
            .grow(&OpenAir, root, SlotIndex::Main, Vec3::Y, 1.0)
            .unwrap();
        t.grow(&OpenAir, a, SlotIndex::Main, Vec3::Y, 1.0).unwrap();
        let b = t
            .grow(&OpenAir, root, SlotIndex::Lateral(0), Vec3::X, 1.0)
            .unwrap();
        let d = t.grow(&OpenAir, b, SlotIndex::Main, Vec3::X, 1.0).unwrap();
        t.prune(a);

        let mesh = build_mesh(&t, &MeshOptions::default());
        assert_eq!(mesh.positions.len(), 4);
        assert_eq!(mesh.indices.len(), 6);
        assert!(
            mesh.indices
                .iter()
                .all(|&i| (i as usize) < mesh.positions.len())
        );
        assert_eq!(
            lines(&mesh).last(),
            Some(&(t.node(b).position, t.node(d).position))
        );
    }

    fn tube_mesh(tree: &Tree) -> TreeMesh {
        let style = MeshStyle::Tubes(TubeOptions::default());
        build_mesh(tree, &MeshOptions { style })
    }

    fn v(mesh: &TreeMesh, i: u32) -> Vec3 {
        Vec3::from_array(mesh.positions[i as usize])
    }

    /// A trunk of three internodes with a lateral on each of the lower two,
    /// bent in several directions and tapering, so every code path runs.
    fn sapling() -> (Tree, [NodeId; 5]) {
        let mut t = seedling();
        let root = t.root();
        let a = t
            .grow(
                &OpenAir,
                root,
                SlotIndex::Main,
                Vec3::new(0.2, 1.0, 0.0),
                1.0,
            )
            .unwrap();
        let b = t
            .grow(&OpenAir, a, SlotIndex::Main, Vec3::new(-0.3, 1.0, 0.4), 0.8)
            .unwrap();
        let l1 = t
            .grow(
                &OpenAir,
                root,
                SlotIndex::Lateral(0),
                Vec3::new(1.0, 0.5, 0.0),
                0.7,
            )
            .unwrap();
        let l2 = t
            .grow(
                &OpenAir,
                a,
                SlotIndex::Lateral(0),
                Vec3::new(-0.5, 0.3, -1.0),
                0.6,
            )
            .unwrap();
        let tip = TreeSpecies::default().thickness.tip_radius;
        for (id, r) in [(root, 9.0), (a, 4.0), (b, 1.0), (l1, 2.0), (l2, 1.0)] {
            t.node_mut(id).radius = r * tip;
        }
        (t, [root, a, b, l1, l2])
    }

    #[test]
    fn side_count_follows_longay_eq_c2() {
        assert_eq!(side_count(0.02, 0.02, 2.0), 3);
        assert_eq!(side_count(0.32, 0.02, 2.0), 12);
        assert_eq!(side_count(0.32, 0.02, 1.0), 48);
        // Thinner than a tip still makes a closed tube.
        assert_eq!(side_count(0.01, 0.02, 2.0), 3);
        // No upper limit.
        assert_eq!(side_count(2.0, 0.02, 1.0), 300);
    }

    #[test]
    fn seedling_tube_is_a_capped_triangular_prism() {
        let mesh = tube_mesh(&seedling());
        assert_eq!(mesh.topology, Topology::TriangleList);
        // Two rings of 3 sides + seam, then a hub and 3 rim vertices.
        assert_eq!(mesh.positions.len(), 2 * 4 + 1 + 3);
        assert_eq!(mesh.normals.len(), mesh.positions.len());
        assert_eq!(mesh.uvs.len(), mesh.positions.len());
        // 3 quads of 2 triangles, then 3 cap triangles.
        assert_eq!(mesh.indices.len(), 3 * 6 + 3 * 3);
    }

    #[test]
    fn rings_have_node_radius_and_are_perpendicular_to_straight_axis() {
        let mut t = seedling();
        let root = t.root();
        let up = t.node(root).heading;
        let a = t.grow(&OpenAir, root, SlotIndex::Main, up, 1.0).unwrap();
        let b = t.grow(&OpenAir, a, SlotIndex::Main, up, 1.0).unwrap();
        for (id, r) in [(root, 0.08), (a, 0.05), (b, 0.02)] {
            t.node_mut(id).radius = r;
        }
        let mesh = tube_mesh(&t);
        let sides = side_count(0.08, t.species().thickness.tip_radius, 2.0);
        let rings = [
            (t.node(root).base(), 0.08),
            (t.node(root).position, 0.08),
            (t.node(a).position, 0.05),
            (t.node(b).position, 0.02),
        ];
        for (k, (center, radius)) in rings.into_iter().enumerate() {
            let start = k as u32 * (sides + 1);
            for i in start..=start + sides {
                let offset = v(&mesh, i) - center;
                assert!((offset.length() - radius).abs() < 1e-5);
                assert!(offset.dot(up).abs() < 1e-5);
                let n = Vec3::from_array(mesh.normals[i as usize]);
                assert!((n - offset / radius).length() < 1e-4);
            }
        }
    }

    /// The vertices of each axis's tube, in [`axes`] order: its rings of
    /// `sides + 1` vertices, then the cap's hub and rim.
    fn tube_vertices(tree: &Tree, mesh: &TreeMesh) -> Vec<Vec<[f32; 3]>> {
        let mut start = 0;
        axes(tree)
            .into_iter()
            .map(|axis| {
                let first = tree.node(axis[0]);
                let sides = side_count(first.radius, first.species.thickness.tip_radius, 2.0);
                let count = (axis.len() + 1) * (sides as usize + 1) + 1 + sides as usize;
                start += count;
                mesh.positions[start - count..start].to_vec()
            })
            .collect()
    }

    #[test]
    fn a_thicker_lateral_still_branches_off() {
        let (mut t, [root, a, b, l1, l2]) = sapling();
        t.node_mut(l1).radius = t.node(root).radius;
        t.node_mut(l2).radius = t.node(a).radius * 2.0;
        assert_eq!(axes(&t), vec![vec![root, a, b], vec![l1], vec![l2]]);
        let mesh = tube_mesh(&t);
        let tubes = tube_vertices(&t, &mesh);
        assert_eq!(
            tubes.iter().map(Vec::len).sum::<usize>(),
            mesh.positions.len()
        );
    }

    #[test]
    fn shedding_the_main_child_leaves_laterals_unchanged() {
        let (mut t, [root, a, _, l1, _]) = sapling();
        // Thicker than the main child, so a thickest-path rule would reroute.
        t.node_mut(l1).radius = t.node(a).radius * 2.0;
        let before = tube_vertices(&t, &tube_mesh(&t))[1].clone();
        t.shed(a);
        assert_eq!(axes(&t), vec![vec![root, a], vec![l1]]);
        let mesh = tube_mesh(&t);
        assert_eq!(tube_vertices(&t, &mesh)[1], before);
        // The trunk runs into the stump and is capped there.
        let stump = t.node(a);
        assert!((0..mesh.positions.len() as u32).any(|i| {
            (v(&mesh, i) - stump.position).length() < 1e-6
                && (Vec3::from_array(mesh.normals[i as usize]) - stump.heading).length() < 1e-6
        }));
    }

    #[test]
    fn pruning_the_main_child_ends_the_axis() {
        let (mut t, [root, a, _, l1, _]) = sapling();
        let before = tube_vertices(&t, &tube_mesh(&t))[1].clone();
        t.prune(a);
        assert_eq!(axes(&t), vec![vec![root], vec![l1]]);
        assert_eq!(tube_vertices(&t, &tube_mesh(&t))[1], before);
    }

    #[test]
    fn every_branch_tip_is_capped() {
        let (t, _) = sapling();
        let mesh = tube_mesh(&t);
        let tips: Vec<_> = t.iter().filter(|(_, n)| n.is_terminal()).collect();
        assert_eq!(tips.len(), axes(&t).len());
        for (_, node) in tips {
            let hub = (0..mesh.positions.len() as u32).find(|&i| {
                (v(&mesh, i) - node.position).length() < 1e-6
                    && (Vec3::from_array(mesh.normals[i as usize]) - node.heading).length() < 1e-6
            });
            assert!(hub.is_some(), "no cap at a tip");
        }
    }

    #[test]
    fn triangles_face_outward_and_normals_are_unit() {
        let (t, _) = sapling();
        let mesh = tube_mesh(&t);
        assert!(
            mesh.normals
                .iter()
                .all(|n| (Vec3::from_array(*n).length() - 1.0).abs() < 1e-4)
        );
        for &[a, b, c] in mesh.indices.as_chunks::<3>().0 {
            let face = (v(&mesh, b) - v(&mesh, a)).cross(v(&mesh, c) - v(&mesh, a));
            let n = |i: u32| Vec3::from_array(mesh.normals[i as usize]);
            assert!(face.dot(n(a) + n(b) + n(c)) > 0.0);
        }
    }

    #[test]
    fn pruned_nodes_leave_no_tube() {
        let (mut t, [_, a, _, l1, _]) = sapling();
        let before = tube_mesh(&t);
        t.prune(a);
        let after = tube_mesh(&t);
        assert_eq!(axes(&t), vec![vec![t.root()], vec![l1]]);
        assert!(after.positions.len() < before.positions.len());
        assert!(
            after
                .indices
                .iter()
                .all(|&i| (i as usize) < after.positions.len())
        );
    }
}
