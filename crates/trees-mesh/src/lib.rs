//! Geometry builder: converts a snapshot of a [`trees_core::Tree`] into
//! engine-neutral branch meshes, leaf instances and levels of detail.
//!
//! See "Geometry generation" in the design document. Output is plain vertex
//! and index buffers ([`TreeMesh`]) so any engine can upload them.
//!
//! Only [`MeshStyle::Wireframe`] is implemented so far: one vertex per node,
//! one line per internode.

use glam::Vec3;
use trees_core::Tree;

/// How a tree is turned into geometry.
///
/// Placeholder until the configuration system is designed; for now it only
/// picks the style.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshOptions {
    pub style: MeshStyle,
}

/// The kind of geometry to build. More styles (swept cylinders, foliage)
/// will be added, so match on it with a wildcard arm.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum MeshStyle {
    /// The skeleton: each internode drawn as a line from its base to its tip.
    #[default]
    Wireframe,
}

/// How [`TreeMesh::indices`] group into primitives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Topology {
    /// Every two indices form a line segment.
    LineList,
}

/// Engine-neutral geometry in world space.
#[derive(Clone, Debug, PartialEq)]
pub struct TreeMesh {
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub topology: Topology,
}

/// Builds a mesh of `tree` in the style chosen by `options`.
pub fn build_mesh(tree: &Tree, options: &MeshOptions) -> TreeMesh {
    match options.style {
        MeshStyle::Wireframe => wireframe(tree),
    }
}

/// One vertex at each node's tip plus one at the root's base, and one line
/// per internode joining its base (the parent's vertex) to its tip.
fn wireframe(tree: &Tree) -> TreeMesh {
    let mut mesh = TreeMesh {
        positions: Vec::with_capacity(tree.len() + 1),
        indices: Vec::with_capacity(tree.len() * 2),
        topology: Topology::LineList,
    };
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use trees_core::{SlotIndex, TreeSpecies};

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
        let a = t.grow(root, SlotIndex::Main, Vec3::Y, 1.0).unwrap();
        let b = t.grow(root, SlotIndex::Lateral(0), Vec3::X, 1.0).unwrap();
        let c = t.grow(a, SlotIndex::Main, Vec3::Y, 1.0).unwrap();
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
        let a = t.grow(root, SlotIndex::Main, Vec3::Y, 1.0).unwrap();
        t.grow(a, SlotIndex::Main, Vec3::Y, 1.0).unwrap();
        let b = t.grow(root, SlotIndex::Lateral(0), Vec3::X, 1.0).unwrap();
        let d = t.grow(b, SlotIndex::Main, Vec3::X, 1.0).unwrap();
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
}
