# Stored foliage: placement contract

The core stores leaves in `Tree::leaves()` alongside the branch graph. Leaves
are not branch children or buds. They are deterministic records included in
`Tree::clone()` snapshots, and are available without a renderer or image asset.

The core implements initial attachment placement and an isolated Longay petiole
rod solver. It does not yet render textured leaves, load PNGs or persist settings.
`Leaf::heading`, `left` and `normal` remain the initial blade frame. Call
`solve_petiole` for final petiole samples and a reoriented blade endpoint/frame.
The temporary viewer still uses the initial frame; solver-driven geometry and
petiole-response UI controls are the next integration steps.

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
   uses its own radius at both ends. Foliage and the tube mesh share
   `Tree::base_radius` so they cannot drift to different taper rules.
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
| `blade_length` | 0.1016 (4 inches) | Non-negative finite distance from blade base to tip |
| `blade_width` | 0.1016 (4 inches) | Non-negative finite distance across blade |
| `petiole_length` | 0.08 | Non-negative finite rod arc length |
| `petiole_response.target` | `(0, 1, 0)` | Finite nonzero normalizable world-space preferred blade-normal direction; normalized by solver |
| `petiole_response.twist` | 2.0 | Non-negative finite eH, twist susceptibility about H |
| `petiole_response.bend` | 2.0 | Non-negative finite eL, bend susceptibility about L |
| `petiole_response.turn` | 2.0 | Non-negative finite eU, turn susceptibility about U |
| `appearance` | `generic-broadleaf` | Opaque, engine-neutral key resolved by the viewer later |

A zero blade dimension suppresses leaves. Zero petiole length is permitted.
Validation applies even when foliage is disabled. Image bytes and renderer handles
never belong in the species record. A future conifer implementation can introduce
new foliage parameters and templates; the existing conifer preset explicitly has
foliage disabled until needle support exists. Its growth architecture is unchanged.
Response susceptibilities have units of radians per world unit for a unit
target, and are illustrative defaults rather than measured elastic moduli. A
zero susceptibility disables that component; all three zero give a straight rod.
Target magnitude has no effect: target normalization is an explicit project
convention that separates response strength from target direction.

## Longay petiole-response solver

`trees-core/src/petiole.rs` implements Appendix C.1.2, printed pp. 143–144.
For the right-handed unit frame H (tangent), L (blade-left) and U (blade-normal),
and normalized target direction T, use Longay's component rates exactly:

```text
ΩH = -eH * dot(L, T)
ΩL =  eL * dot(H, T)
ΩU =  eU * dot(L, cross(T, cross(H, T)))
Ω  = ΩH * H + ΩL * L + ΩU * U
```

These are rates per arc length, not angles per iteration. A rod of length S
is integrated from its bark attachment toward its blade with `ds = S / N`.
The target is fixed in world space for a solve. This is Longay's response
model, not Hart's iteration-count-based reorientation or a light simulation.
It is not a global optimization guaranteeing the normal will reach T.
In particular, exactly aligned and exactly opposite normals are zero-torque
equilibria; no arbitrary flip is introduced at the opposite normal.

The numerical method is a project choice consistent with the infinitesimal
rates, rather than a finite rotation-order prescription from the paper:

1. Validate response, rod length, resolution and initial attachment/frame.
   Reject non-finite attachment coordinates, invalid frames, negative or
   non-finite length, and non-normalizable target vectors. Resolution must
   be in `1..=4096`, including for zero-length rods.
2. Accept initial frame rounding within `1e-4`, then orthonormalize in f64.
3. At each segment, compute Ω at the current frame. Rotate a trial frame by
   `Ω * ds/2` using a quaternion to obtain a midpoint estimate.
4. Advance the position by `midpoint.H * ds`, and rotate the original frame
   by `Ω(midpoint) * ds`. This explicit midpoint step is second order for
   smooth solutions and avoids choosing a privileged order for finite H/L/U
   rotations. Quaternion rotations preserve frame orthogonality.
5. Emit the endpoint position and frame, and repeat proximally to distally.
   Internal positions and rotations use f64; returned samples use f32 and
   non-finite output is rejected explicitly.

The polyline has N equal-length chords before f32 rounding, with total length
S. Its chord directions approximate the smooth midpoint tangents; sample frames
represent boundary tangents rather than exact chord directions. Increasing N
refines the path and final frame without multiplying the biological response.
The default `PetioleOptions { segments: 32 }` is independent of species data.
Strong responses or long rods may need finer resolution: there is no adaptive
error estimator, and finite output alone does not certify convergence. At large
world coordinates, f32 output rounding also limits resolvable segment lengths.

The public API returns `Result<Petiole, PetioleError>`. `samples()` always includes
the initial attachment and, for positive length, N subsequent samples. A
zero-length rod returns only the initial position/frame, with no duplicate or
zero-length segments. `tip()` returns the final sample: its position is the
blade's bottom-center pivot, and its H/L/U frame orients and scales the blade.
Continue to apply width and length from that base rather than from its center.
`Leaf::blade_base` remains an explicitly **unbent** convenience method; final
geometry should instead use the solver's endpoint.

```rust
use std::sync::Arc;
use glam::Vec3;
use trees_core::{Tree, TreeSpecies, PetioleOptions, solve_petiole};

let mut species = TreeSpecies::default();
species.foliage.petiole_response.twist = 3.0;
species.foliage.petiole_response.bend = 1.0;
let tree = Tree::new(Arc::new(species), 42, Vec3::ZERO)?;
let rod = solve_petiole(
    &tree.leaves()[0], &tree.species().foliage, PetioleOptions::default(),
)?;
let blade_base = rod.tip().position;
let blade_normal = rod.tip().normal;
```

The solver is pure: it does not change stored leaf attachments, buds, branches,
seeds or the species record. Rod samples are derived on demand and can be rebuilt
from an immutable tree snapshot. Solver errors distinguish invalid response,
length, segment count, initial frame and non-finite output. Only response and
petiole length are read from the foliage record; blade size and appearance do
not change the solved rod.

Tests cover the three signed torque components; single-axis analytic solutions
for twist, bending and the bent centerline; zero response and zero length;
length preservation and orthonormal frames; aligned, opposite and near-parallel
cases; deterministic solves and target normalization; invalid input/output
handling; real stored leaf fixtures and stationary anchors; and convergence
against a high-resolution reference as segment counts increase.

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
`grow` rejects zero/non-finite or unnormalizable directions, negative/non-finite
lengths and non-finite resulting positions before consuming a bud. Zero length
remains valid and emits no attachments.
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
cast shadows. One unit-circle perimeter is reused for every disk in a rebuild.
Hiding the disks retains their mesh; a changed tree replaces it.

Each disk defaults to 0.1016 world units across (four inches, interpreting one
unit as one metre). Width and length independently scale the debug disk into an
ellipse, matching the blade dimensions. Its bottom-center anchor stays at the
petiole endpoint, and its tip extends along the leaf heading by `blade_length`.
The center moves by half the length; the anchor never moves when either blade
dimension changes. The plane follows the stored normal. UV bottom center
`(0.5, 1)` maps to that same anchor, ready for the prepared PNG convention.
Positive blade-left maps to image-left (`u=0`), avoiding a mirrored texture.
Zero width or length suppresses placements. These shapes are temporary debugging
aids, not final textured foliage.

The **Foliage** section edits leaves enabled, alternate/opposite arrangement,
spacing, maximum branch radius, leaf divergence, attachment angle, petiole length,
blade length and blade width. Angles are displayed in degrees and stored in
radians. Placement is independent of the Architecture section's branch controls.
The appearance key is not exposed because texture selection is still pending.
The viewer currently deterministically regrows on species edits, retaining width
pins; foliage-only edits reproduce the same branch skeleton. Debug visibility
alone never triggers regrowth. Like the other existing viewer settings, these
controls are session-local until the persistence unit is implemented.

Viewer tests check default diameter, independent axis scaling, bottom-center anchoring,
UV orientation, centers, planes and triangle winding, debug visibility and mesh replacement, and that foliage edits preserve branch geometry.

For a quick smoke check, run `cargo test --workspace`,
`cargo clippy --workspace --all-targets -- -D warnings`, and
`cargo fmt --all --check`. Native inspection: run `cargo run -p trees-viewer`,
verify the green placements, toggle Debug leaf disks, then vary leaf width and
length independently. The petiole endpoint should remain the bottom-center
anchor, and the branch skeleton should stay the same. Width pins should update
which tapered regions qualify for leaves. The headless viewer tests cover toggle
and mesh-update behavior without requiring a display.
