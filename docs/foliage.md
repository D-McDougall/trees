# Stored foliage: placement contract

The core stores leaves in `Tree::leaves()` alongside the branch graph. Leaves
are not branch children or buds. They are deterministic records included in
`Tree::clone()` snapshots, and are available without a renderer or image asset.

This change implements the species schema and **initial attachment placement**.
It does not yet render textured leaves, load PNGs, persist settings, or solve petiole
bending/twisting. `Leaf::heading`, `left` and `normal` describe the initial
blade frame, not a final orientation obtained by the later rod solver.

## Source and implementation choices

Longay (2014), *Interactive Procedural Modelling of Trees and Landscapes*,
Appendix C.1.1, printed p. 141, places leaves independently of buds on branches
below a width threshold. C.1.2 starts their orientations from phyllotaxis before
applying petiole bending and twisting. The supplied source is
`docs/2014 Interactive Procedural Modelling of Trees and Landscapes pg86-170.pdf`.

Longay does not give a complete distance-sampling rule. The following are
project conventions, not claims of exact equations from the thesis:

1. An axis follows main-child slots; every lateral child starts another axis.
   Stumps end an axis. No lateral takes over when a main child disappears.
2. With spacing `s`, station `i` lies at arc distance `(i + 0.5) * s` from
   the axis base. Distance and station phase continue through every internode,
   including thick ones. A lateral restarts the distance sequence.
3. Internodes own half-open distance intervals `[base, tip)`. A station exactly
   at a junction belongs to the distal internode; no duplicate terminal leaf is
   added. Zero-length internodes produce no attachments. The current root is
   a real, nonzero internode, and may carry leaves.
4. Radius is linearly interpolated at each station. A main continuation uses
   its parent's radius at its base and its own radius at its tip; an axis start
   uses its own radius at both ends. This matches the existing tube mesh.
   A leaf exists only if this local **radius is strictly less than**
   `foliage.max_radius`. Thus the cutoff diameter is twice that value.
5. Attachments are offset radially from the centerline onto the bark surface.
   Opposite arrangements produce two attachments at opposite radial positions
   at the same distance; alternate arrangements produce one.
6. Each axis starts from a canonical perpendicular to its heading, rotated by
   a phase hashed from the tree seed and structural slot path. Transport this
   frame along the axis using the shortest rotation between successive headings.
   At an exactly reversed heading, glam's deterministic perpendicular-axis
   rotation chooses the otherwise ambiguous half-turn. Apply foliage divergence
   per station, including stations excluded by the cutoff. Branch-bud divergence,
   branching angle, arrangement and occupancy do not set foliage spacing or phase.
7. Initial petiole heading H is `cos(a)*axis_heading + sin(a)*radial`, where `a`
   is `foliage.attachment_angle`. L is `axis_heading × radial` and U is `H × L`.
   The frame is orthonormal and right handed. At a 90-degree attachment angle,
   H points outward and U follows the supporting axis. The unbent blade base is
   the attachment plus H times `foliage.petiole_length`.

No marker allocation, light competition, vigor or branch growth equations are
changed. There is still no growth simulator: the viewer's stand-in growth routine
uses the same `Tree::grow` API and now produces stored leaf data automatically.

## Species fields

`TreeSpecies::foliage` is a validated `Foliage` record, immutable for the tree's
lifetime under the existing species ownership model. The generic broadleaf is
still the default. Values are illustrative world-unit settings, not calibrated
botanical measurements.

| Field | Default | Contract |
| --- | --- | --- |
| `enabled` | true | False produces no leaves |
| `spacing` | 0.25 | Positive finite distance between stations |
| `max_radius` | 0.06 | Non-negative finite strict radius cutoff; zero produces no leaves |
| `arrangement` | Alternate | One leaf or an opposite pair, independent of branch buds |
| `divergence_angle` | golden angle | Finite radians per station; may be negative |
| `attachment_angle` | 60 degrees | Finite radians in `[0, π]` |
| `blade_length` | 0.3 | Non-negative finite distance from blade base to tip |
| `blade_width` | 0.15 | Non-negative finite distance across blade |
| `petiole_length` | 0.08 | Non-negative finite straight initial length |
| `appearance` | `generic-broadleaf` | Opaque, engine-neutral key resolved by the viewer later |

A zero blade dimension suppresses leaves. Zero petiole length is permitted.
Validation applies even when foliage is disabled. Image bytes and renderer handles
never belong in the species record. A future conifer implementation can introduce
new foliage parameters and templates; the existing conifer preset explicitly has
foliage disabled until needle support exists. Its growth architecture is unchanged.
Petiole elasticity and target-orientation fields will accompany their solver in
the later orientation unit rather than expose parameters with no effect now.

## Ownership, identity and update lifecycle

`Leaf` stores its supporting `NodeId`, local distance, world-space attachment,
initial frame and `LeafId { axis, station, side }`. Dimensions, petiole length and
appearance come from the owning tree's species; they are not duplicated per leaf.
An id is tree-local, not a globally unique texture or persistent project id.
Ids remain stable across rebuilds, width edits and edits to unrelated axes.
Station numbers do not compact when a thick region rejects leaves. The supporting
node can change under equivalent subdivision while positions, frames and station
identity remain equivalent (up to floating-point error) along a straight axis.

`Tree::new`, successful `grow`, `prune`, `shed`, and `update_radii` rebuild the
stored leaf collection. Failed growth and refused edits do not change it. A
radius pin takes effect on foliage when `update_radii` applies it, matching the
existing pin lifecycle. Growth/pruning refresh against the currently stored
radii; callers must still run `update_radii` to solve new branch thicknesses.
`advance_step` only changes the season counter and does not age/drop leaves.

The existing public mutable-node accessors bypass these automatic passes. After
direct edits, call `update_radii` to synchronize radii and foliage, or
`update_leaves` if radii are already correct. Neither pass modifies buds.

A rebuild visits the skeleton and emits leaves, with an additional path walk for
each axis start. Successful individual growth calls currently rebuild the whole
collection, favoring a coherent public API over incremental complexity. This can
be costly for large batches; a future growth simulator should introduce a batched
edit/finalize API if profiling warrants it. Sampling cost and memory scale with
axis length divided by spacing; callers should use sensible world scales.

## Validation

Core tests use synthetic seedlings and straight, subdivided, bent and lateral
axes. They verify exact distances and surface offsets, frame handedness, independent
alternate/opposite phyllotaxis, divergence, seed/rebuild determinism, strict radius
boundaries and taper, clone independence, prune/shed cleanup, width-pin updates,
zero dimensions/length and disabled conifer foliage. Species tests reject invalid
ranges and non-finite values. Existing branch graph and mesh tests remain relevant
regression checks. Run `cargo test --workspace`.

## Temporary viewer debug mode

The viewer starts with **Debug leaf disks** enabled. The checkbox above the
species sections hides/shows the disks without changing species or regrowing the
tree. A counter shows the number of stored leaf placements. Disks refresh from
`ShownTree`, so radius pins and species/seed changes are reflected automatically.
They share one batched mesh and an unlit, two-sided green material, and do not
cast shadows. Hiding the disks retains their mesh; a changed tree replaces it.

Each disk is 0.1016 world units across (four inches, interpreting one unit as one
metre for this marker). Its center is the initial petiole endpoint plus half the
blade length along the leaf heading; its plane follows the stored blade normal.
This marks the center of the eventual blade rather than its bark attachment.
Disk size stays fixed when blade dimensions change. Blade length moves its
center; blade width only suppresses placements when zero, until real leaf
geometry is added. These disks are temporary debugging aids, not final foliage.

The **Foliage** section edits leaves enabled, alternate/opposite arrangement,
spacing, maximum branch radius, leaf divergence, attachment angle, petiole length,
blade length and blade width. Angles are displayed in degrees and stored in
radians. Placement is independent of the Architecture section's branch controls.
The appearance key is not exposed because texture selection is still pending.
The viewer currently deterministically regrows on species edits, retaining width
pins; foliage-only edits reproduce the same branch skeleton. Debug visibility
alone never triggers regrowth. Like the other existing viewer settings, these
controls are session-local until the persistence unit is implemented.

Viewer tests check disk diameter, centers, planes and triangle winding, debug
visibility and mesh replacement, and that foliage edits preserve branch geometry.
