# Gravitropism

Gravitropism steers newly generated internodes toward a species-defined
inclination. It does not bend existing wood. Geometry and lengths remain in
metres, angles are stored in radians, and direction weights are dimensionless.
The viewer edits lengths in feet (1 ft = 0.3048 m) and angles in degrees.

## Sources and model

Longay, *Interactive Procedural Modelling of Trees and Landscapes* (2014),
[§4.4.1–4.4.2, printed pp. 39–44](<2014 Interactive Procedural Modelling of Trees and Landscapes pg1-85.pdf>),
introduces straightness in Equation (4.4), extends it with local gravitropism
in Equation (4.5), and illustrates the three influences and barycentric weights
in Figure 4.16. Figures 4.17–4.19 show horizontal growth, downward growth, and
separate main/lateral behavior. The normalized construction and nearest-target
choice are also described in
[Hart (2020), §9.4.1, printed pp. 79–81](<2020 Interactive Visualization and Modeling of Plant Structures in Virtual Reality.pdf.txt>).

For unit bud heading H, local tropism target T, and environmental direction E:

```text
D = normalize(αH + βT + γE)
α + β + γ = 1; α, β, γ >= 0
```

E is the normalized sum of unit vectors toward won attraction markers, so each
marker has equal weight. The implementation retains E and the marker count as
sensing data; D is used only for the growth request.

Inclination θ is measured from vertical up (+Y): 0° is up, 90° is horizontal,
and 180° is down. For a heading with a usable horizontal projection:

```text
R = normalize((H.x, 0, H.z))
T = cos(θ) Y + sin(θ) R
```

This preserves heading azimuth and selects the nearer target in its vertical
plane. Exactly 0 and π return +Y and -Y directly, avoiding trigonometric residue.
Only an order-zero main-slot continuation uses `main_set_point`. Every other
prospective child uses `lateral_set_point`, including the first lateral on the
trunk and main-slot continuations of lateral axes.

## Implementation choices

`crates/trees-core/src/tropism.rs` is pure direction arithmetic;
`crates/trees-core/src/growth.rs` applies it through `grow_step` after sensing.

Longay chooses a random vertical plane for almost-vertical headings. Here a
horizontal projection of length at most 1e-6 on the unit heading triggers a
stateless seeded azimuth. A fixed wrapping integer hash combines the tree seed,
node ID, and main/lateral slot; its upper 24 bits map into [0, 2π). This makes
symmetry breaking reproducible without consuming a traversal-dependent random
stream. Different graph IDs or bud slots can select different planes. Replay
assertions apply to identical seeds, parameters, and edit histories on the same
build; they do not promise bitwise equality across platforms.

The vertical threshold is independent of blend cancellation. An exactly zero
blend stays zero and is passed to `Tree::grow`, which adds no internode; the bud
becomes dormant and the growth step still advances. No substitute heading is
used. Representable nonzero blends are normalized without a cancellation
epsilon, scaling first to avoid underflow. Zero-weight directions are ignored.

Marker support remains necessary even when γ = 0. Buds with no usable marker
direction stay dormant. Competition reads a snapshot before any new nodes are
added; new buds compete on the next step. Steering preserves species internode
length and the existing habitat endpoint check. Direct calls to `Tree::grow`
continue to use the caller's direction.

Existing node positions, headings, frames, lengths, and birth steps do not
change when subsequent growth uses different tropism settings. In contrast,
viewer parameter edits regenerate the whole preview from its seed, so its old
preview is replaced. Radius updates may thicken old wood independently of
steering.

## Integrated validation

`crates/trees-core/tests/gravitropism.rs` exercises the public growth API across
six steps. It checks deterministic replay, unit/perpendicular frames, finite
positions and radii, internode displacement/length, main versus lateral
selection, angle sensitivity with β > 0, angle independence with β = 0,
preservation of existing geometry, marker gating, and exact cancellation.
The axis checks require both first lateral internodes and continuations to be
present, rather than passing vacuously on an unbranched tree.

The viewer's `tropism_edits_regenerate_the_preview_reproducibly` test checks
that editing angles and weights regenerates the ECS preview, matches a fresh
build, remains unchanged on an idle update, and restores the original geometry
when parameters are restored. Existing editor tests cover idle parameter
preservation and the weight simplex.

Run:

```sh
cargo test -p trees-core -p trees-viewer
cargo fmt --all -- --check
```

### Observed preview behavior

Orthographic XY and ZY skeleton projections were visually inspected for seed
42, six growth steps, and `OpenAir`, using the integration-test fixture:
0.75 m sampling cells, jitter 0.25, 0.25 m occupancy radius, 3 m perception
distance, 90° perception half-angle, and otherwise default architecture.
All four configurations use α = 0. These are deliberately strong steering
examples, rather than tuned species presets.

| Configuration | Weights and set-points | Observed form | Internodes |
| --- | --- | --- | --- |
| Environment only | γ = 1 | Irregular spreading branches responding to markers | 127 |
| Upright | β = 1; main/lateral 0° | All new segments vertical up, with overlapping axes | 27 |
| Horizontal laterals | β = 1; main 0°, lateral 90° | Upright leader with radially spreading horizontal tiers | 122 |
| Downward | β = 1; main/lateral 180° | All new segments vertical down, with overlapping axes | 27 |

These counts record the inspected fixture; they are not silhouette or node-count
requirements. Pure up/down steering can place multiple branches on the same
line. `OpenAir` allows growth below the origin; the viewer's ground habitat can
reject those endpoints. Inspection covered generated skeleton projections,
not a live GPU-rendered viewer session. Automated checks also exercise the
actual viewer preview generation path. Competition, habitat, bud headings, and
mixed weights affect practical crown shape; a set-point does not prescribe an
exact silhouette or enable unsupported buds to grow.
