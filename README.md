# trees

A procedural generator for 3D tree models, written in Rust.

Most procedural trees repeat a fixed recursive rule, such as an L-system or a
fractal, and need many hand-tuned parameters before the branching looks
natural. This project grows trees the way Pałubicki et al. (2009) describe
instead: buds compete for light and free space, internal signals decide how
much vigor each one gets, and the form of the crown emerges from that
competition. A handful of species parameters (apical control, light
sensitivity, tropism, phyllotaxis) is enough to get plausible branch density,
and the same model lets a user steer growth with brushes rather than numbers.

The generator runs natively and in the browser.

## Status

The project is at an early stage. The data model and occupancy-aware marker
growth are in place; light and vigor simulation are still planned.

| Crate | What works today | Planned |
| --- | --- | --- |
| [`trees-core`](crates/trees-core) | The tree graph (an arena of internodes whose empty child slots are buds), species parameters with validation and two presets, growing, pruning and shedding internodes, tip-to-base folds, pipe-model branch radii with width pins, stored species-dependent leaf attachments and initial frames, the `Habitat` trait, and deterministic occupancy-aware space colonization | A light grid, vigor allocation and automatic shedding |
| [`trees-mesh`](crates/trees-mesh) | Wireframe output: one line per internode | Swept generalized cylinders, leaf instances, levels of detail |
| [`trees-viewer`](crates/trees-viewer) | A Bevy app that grows a stand-in test tree and shows its tube mesh on a ground disc, with an orbit camera and an egui side panel that edits the seed and species growth parameters, regrowing the tree on each change. Right-click a branch for a menu that pins its width, re-solving the pipe model live; pins are marked in the view and listed in the panel | Brush, prune, bend and undo tools |

## Quick start

You need Rust 1.97.1 or newer.

Run the viewer natively:

```sh
cargo run -p trees-viewer
```

Run it in the browser with [Trunk](https://trunkrs.dev):

```sh
rustup target add wasm32-unknown-unknown
cargo install --locked trunk
cd crates/trees-viewer
trunk serve --release
```

Then open <http://127.0.0.1:8080>. Use `--release`: a debug build of Bevy for
the web is several hundred megabytes of wasm.

Run the tests:

```sh
cargo test --workspace
```

## Workspace layout

```
crates/
  trees-core/    tree graph, species parameters, Habitat trait (depends only on glam)
  trees-mesh/    engine-neutral geometry builder (vertex and index buffers)
  trees-viewer/  Bevy + egui viewer, native and wasm
docs/
  design.md      design document
```

Data flows one way. The growth simulator and user edits change the tree graph
in `trees-core`. `trees-mesh` reads a snapshot of the graph and builds plain
buffers. The viewer uploads those buffers to Bevy. Neither library crate
depends on a game engine, so the geometry can be used anywhere.

## Using the libraries

The crates are not published to crates.io. Depend on them by path or Git URL.

```rust
use std::sync::Arc;

use glam::Vec3;
use trees_core::{SlotIndex, Tree, TreeSpecies};
use trees_mesh::{MeshOptions, build_mesh};

let species = Arc::new(TreeSpecies::conifer());
let mut tree = Tree::new(species, 42, Vec3::ZERO)?;

// The world, as growth sees it: anything below the ground plane is solid.
// A closure works; a game would implement `Habitat` on its own scene.
let ground = |p: Vec3| p.y < 0.0;

let root = tree.root();
tree.grow(&ground, root, SlotIndex::Main, Vec3::Y, 0.5);
tree.grow(&ground, root, SlotIndex::Lateral(0), Vec3::new(1.0, 0.3, 0.0), 0.3);
tree.advance_step();

// Plain positions and indices, ready for any engine.
let mesh = build_mesh(&tree, &MeshOptions::default());
```

`Tree::grow` returns `None` when the bud is missing or the new tip would land
in occupied space; the bud then stays in place and can grow later.

## Design

The tree graph is the single source of truth: everything visible is derived
from it, so a growth step, a pruned branch or an undo is handled by rebuilding
meshes, never by patching them. Simulation and rendering run on separate
threads, and the renderer reads an immutable snapshot. Every randomized choice
is seeded and hashed by position, so the same species, seed and edit history
always produce the same tree.

Foliage placement follows Longay’s thin-branch rule independently of buds. The core
stores attachments and initial leaf frames; temporary debug disks (four inches by default, scaled from the blade base) visualize them in the viewer. The Foliage panel
edits placement parameters; textured leaf geometry and uploads remain planned. See the [foliage contract](docs/foliage.md) for parameters and lifecycle.

The full design, including the data structures, the growth loop, the
interaction tools and the geometry passes, is in
[docs/design.md](docs/design.md).

## References

The growth model and interaction design build on the following work.

1. Wojciech Pałubicki, Kipp Horel, Steven Longay, Adam Runions, Brendan Lane,
   Radomír Měch and Przemyslaw Prusinkiewicz. 2009. Self-organizing tree
   models for image synthesis. *ACM Transactions on Graphics* 28, 3
   (Proceedings of SIGGRAPH 2009), Article 58, 10 pages.
   <https://doi.org/10.1145/1531326.1531364>
2. Steven Longay. 2014. *Interactive Procedural Modelling of Trees and
   Landscapes.* PhD thesis, Department of Computer Science, University of
   Calgary. <https://doi.org/10.11575/PRISM/26605>
3. Jeremy Adam Hart. 2020. *Interactive Visualization and Modeling of Plant
   Structures in Virtual Reality.* MSc thesis, Department of Computer Science,
   University of Calgary. <http://hdl.handle.net/1880/116303>
4. Miłosz Makowski, Torsten Hädrich, Jan Scheffczyk, Dominik L. Michels, Sören
   Pirk and Wojtek Pałubicki. 2019. Synthetic Silviculture: Multi-scale
   Modeling of Plant Ecosystems. *ACM Transactions on Graphics* 38, 4
   (Proceedings of SIGGRAPH 2019), Article 131, 14 pages.
   <https://doi.org/10.1145/3306346.3323039>
5. Ruben M. Smelik, Tim Tutenel, Rafael Bidarra and Bedrich Benes. 2014. A
   Survey on Procedural Modelling for Virtual Worlds. *Computer Graphics
   Forum* 33, 6, 31–50. <https://doi.org/10.1111/cgf.12276>

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.

## Occupancy-aware growth API

The viewer can call `trees_core::grow_step(&mut tree, &habitat,
SamplingConfig { cell_size: 0.5, jitter: 0.25 })?` to grow one species-length
internode per bud that wins a preferred direction. It advances the season once.
For sensing alone, call `trees_core::sense` with the same arguments; it returns
`Vec<BudGrowth>` and refreshes bud marker counts, sums, and active/dormant fates
without advancing the season. `BudGrowth::preferred_direction` and
`Bud::environment_direction()` expose the normalized sum of unit marker vectors.

Both APIs use the tree seed, a world-origin `RestrictedJitter` grid, and the
species perception distance and half-angle. Queries enclose each perception
sector in an AABB, then apply exact distance and angle tests. Habitat occupancy
and closed spheres at all live node tips remove markers. Closest eligible buds
win, with node ID then main/lateral slot order breaking exact ties. Aborted buds
are excluded; buds without a direction remain dormant. Every competition pass
finishes before any branches grow, so new nodes compete only next season.
Habitat implementations should keep their occupancy view consistent during a call.

Markers determine direction only; `architecture.internode_length` determines
branch length. This stage does not allocate light or vigor or blend tropisms.
Sampling errors leave the tree unchanged. Radius/foliage refreshes beyond those
performed by `Tree::grow` can be requested with `Tree::update_radii` after a step.
