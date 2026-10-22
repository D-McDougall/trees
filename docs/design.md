# Tree Generator Design Document

Oct 19, 2026 · David A McDougall

## Purpose and goals

The generator grows trees by simulating competition between buds for light and space, rather than by repeating a fixed recursive pattern. This self-organizing approach (Pałubicki 2009, refined by Longay 2014 and Hart 2020) yields realistic branch density with a handful of meaningful parameters, and it lets artists steer growth with brushes instead of tuning numbers.

**Goals**

- Generate a single, fully branched tree whose form emerges from a small set of species parameters (apical control, light sensitivity, tropism, phyllotaxis).
- Let a user steer growth interactively: paint where branches should go, prune, bend, set thickness, undo.
- Produce geometry for a real-time 3D rendering engine: smooth branches, clean junctions, instanced leaves, levels of detail.

**Non-goals for the first version**

- Botanical accuracy beyond visual plausibility; no quantitative validation against measured trees.
- Self-weight bending, wind animation, roots, and seasonal phenology.
- Changing species parameters at run time; a tree's parameters are fixed when it is created.
- VR and tablet interfaces; the first version is desktop only, and these are future work.
- Forests and ecosystems: the generator grows one tree at a time, internode by internode.

## System architecture

The tree graph is the single source of truth: the growth simulator and user edits change it, and everything visible is derived from it. Keeping geometry derived, never edited directly, means any change (a new growth step, a pruned branch, an undo) is handled by rebuilding the affected meshes rather than patching them.

The system has five subsystems:

1. **Species library** — named parameter sets that define how a kind of tree grows (apical control, light sensitivity, tropism, phyllotaxis, shoot length, pipe exponent).
2. **Environment** — the shared world the tree grows into: attraction markers that stand for free space, a voxel light grid that stores shadow, obstacles and terrain.
3. **Growth simulator** — advances the tree one season per step: senses the environment at each bud, allocates vigor, grows shoots, sheds weak branches and updates thickness.
4. **Interaction layer** — turns user gestures (brush, lasso, prune, bend, thickness pins) into changes to markers, constraints or the tree graph, and records them in an edit history.
5. **Geometry builder** — converts a snapshot of the tree graph into smooth branch meshes, leaves and levels of detail.

&#91;embedded content: System architecture · 5 subsystems around the tree graph\]

Arrows show data flow: inputs at the top feed the simulator, which writes the tree graph; the geometry builder only ever reads a snapshot of it.

Three cross-cutting decisions hold these together:

- **Simulation and rendering run on separate threads.** The renderer reads an immutable snapshot of the tree graph, so a slow growth step never drops frames (Hart 2020).
- **Traversals are written as generic folds.** Every whole-tree computation is either a tip-to-base pass (gathering light, computing radii) or a base-to-tip pass (distributing vigor). Writing these once as higher-order functions keeps the algorithms independent of node layout.
- **All randomness is seeded and position-hashed.** The same species, seed and edit history always produce the same tree, which makes undo, saving and network sharing cheap: store the recipe, not the mesh.

## Core data structures

Eight structures carry the whole system; the tree graph and the two spatial structures (marker index and light grid) do almost all the work during growth.

| Structure | What it holds | Why it is shaped this way |
| --- | --- | --- |
| **Tree graph** | A rooted tree of nodes. Each node is one internode: world-space position, heading frame, length, radius, branch order, age, accumulated light and vigor. Children are the continuing axis plus one or two laterals (alternate or opposite branching). | World-space positions avoid re-walking turtle transforms; an empty child slot *is* a bud, so buds need no separate storage (Hart 2020). |
| **Bud** (implicit) | The empty child slot plus its direction, its fate (dormant, active, aborted) and the markers it won this step. | Bud fate is the only decision growth makes; keeping it on the node keeps the graph compact. |
| **Species parameters** | An immutable record: apical control λ, light exponent κ, max shoot length, tropism angle and blend weights, phyllotaxis and branching angles, perception and occupancy radii, shadow constants, pipe exponent, shedding threshold. | One record per species makes trees reproducible and lets users save and compare presets. |
| **Marker set + spatial index** | Attraction points that represent unclaimed space, stored in a uniform grid or k-d tree. | Assigning markers to buds is the main bottleneck when done by brute force; a spatial index turns it from buds × markers into a local query. |
| **Light grid** | Sparse voxel grid of shadow values, stored as a hash map from coarse-cell key to a dense block allocated on demand. | Dense locality for fast shadow writes, without a fixed world size (Hart 2020). |
| **Constraint set** | User pins that the simulator must respect: width at a node (an absolute radius, dropped when the node is cut or shed), bend handles, protected (unsheddable) branches, growth regions. | Separates user intent from simulated state so regrowth can re-apply it. |
| **Edit history** | An ordered log of growth steps and user operations, with periodic snapshots of the tree graph. | Gives full undo and redo and reproducible saves. |
| **Render snapshot** | A frozen copy of the tree graph plus derived skeleton curves, mesh buffers, leaf instances and LOD levels. | Lets the renderer read without locking the simulator. |

## Growth simulation

Each growth step stands for one season and runs the same eight stages. A bud grows only if it wins space (at least one marker) **and** receives enough vigor from the tree's light budget — Longay's unified model. Space decides *where* growth is possible; light decides *how much* of it happens.

1. **Clear occupied space.** Remove markers inside any node's occupancy sphere. Space the tree already fills is no longer available.
2. **Sense space (space colonization).** Assign each remaining marker to one bud whose perception cone contains it, preferring buds that would grow long shoots. Each bud's markers give it a preferred direction and permission to grow. *Why:* this spreads branches evenly and stops them crowding each other without any explicit density parameter.
3. **Cast shadow.** Each node adds a small pyramid of shadow to the light grid below it. A bud's light exposure is full light minus the shadow at its cell. *Why:* cheap, local, and enough to make inner and lower branches starve, which carves out a realistic crown.
4. **Gather light (tip to base).** Each bud's light is shaped by the species' light exponent and its position on the parent branch (gravimorphism), then summed toward the root. Every branch now knows how much light it collects.
5. **Distribute vigor (base to tip).** The root's vigor is split at every branch point between the continuing axis and the laterals, weighted by their light and by apical control λ (extended Borchert–Honda model). *Why:* λ is the single knob that moves a tree from a clear trunk (conifer-like) to a spreading crown (oak-like).
6. **Grow shoots.** Each active bud adds a shoot whose length scales with its share of the maximum vigor. Shoot direction blends three pulls: keep going straight, follow the species' preferred angle to gravity, and head toward the bud's markers. New lateral buds are placed by the phyllotaxis and branching angles.
7. **Shed.** Remove branches whose light per internode falls below a threshold. *Why:* shedding forms bare lower trunks; it is optional because it can delete branches a user painted.
8. **Update thickness (tip to base).** Radii follow the pipe model: a parent's cross-section relates to the sum of its children's, with an exponent between 2 and 3. Radii never shrink when branches are shed: a shed branch leaves a stump that keeps its radius, and stumps are permanent, so they are the tree's memory of its shed branches (Pałubicki 2009 §4.5) and trunks keep the scars of their past.

An alternative vigor model is worth keeping behind the same interface: the **priority model** (Pałubicki 2009), which ranks branches along an axis and is better for animating a tree that loses apical control with age.

## Environment and light

The environment answers two questions for every bud each step: is there free space nearby, and how much light reaches it? Markers answer the first; the light grid answers the second. Keeping them separate lets each be tuned or replaced independently.

- **Markers as space.** Markers come from three sources: an artist's brush or lasso envelope, a species' default crown envelope, or autonomous generation inside each bud's perception cone (more markers for more vigorous buds). Autonomous markers let a tree grow with no input; painted markers let a user shape it.
- **Light as accumulated shadow.** Shadow uses a distance-based falloff rather than a strict pyramid, so the spread is smooth at the corners (Hart 2020). Cells are taller than they are wide so shadow concentrates below a branch. Because every node casts the same pattern, it is precomputed once as a stencil.
- **Obstacles and terrain.** Walls, buildings and the ground are written into the same grids: they delete markers and add shadow. Trees then grow around obstacles with no special-case logic.
- **Light as an artistic proxy.** The finished shadow grid doubles as an ambient-occlusion term at render time, and per-branch vigor can tint leaves (weaker branches toward yellow).

## Artist control

Control works by changing the simulator's inputs, not by overriding its outputs. A brush adds markers; a pin adds a constraint; the simulator then grows a plausible tree that honours them. This keeps edited trees as natural-looking as unedited ones.

| Tool | What the user does | What changes underneath |
| --- | --- | --- |
| Space brush | Sprays where branches should go; a small radius sketches a single limb | Adds markers (Poisson-spaced at the occupancy radius) |
| Lasso envelope | Draws a crown silhouette | Inflates the 2D outline to a 3D volume and fills it with markers |
| Directional stroke | Brushes in a direction | Sets a local tropism, so branches flow with the stroke |
| Selective growth | Picks a branch to grow | Restricts active buds to that subtree; all buds still claim space |
| Prune | Cuts a branch | Cut: leaves a stump that keeps its thickness; stumps are never removed |
| Bend | Drags a handle on a branch | Solves an elastic deformation where thick branches resist more than thin ones |
| Thickness pins | Sets trunk or branch width at a node | The pinned node keeps exactly that radius as the tree grows: the subtree below each pin solves numerically for its own pipe exponent (Longay 2014 §5.3.2), and the pin counts as a tip of that radius above it. A pin no wider than the thickest branch or stump it carries cannot be solved; it widens its node but never pinches it |
| Parameter panel | Picks a species preset and tunes its sliders, including a triangle widget for the straight / gravity / space blend, before growth starts | Sets the species record, fixed for the tree's lifetime |
| Undo / redo | Steps backward or forward through every growth step and edit | Restores the nearest snapshot and replays the full edit history to that point |

The known tension is that painting into an already shaded region often fails: low light starves the new branch, and shedding removes it. The design mitigates this with a sharper light exponent (fewer buds activate, so shedding is rarely needed), normalized shoot length (growth is guaranteed even in shade), and protecting user-painted branches from shedding.

## Geometry generation

Geometry is built in four passes from a render snapshot, each pass independent per branch so the whole build parallelizes, on the CPU or in GPU tessellation shaders.

1. **Split into axes.** Each axis follows a node's main (apical) child and every lateral starts a new axis. The apical continuation is fixed by the tree, never inferred from thickness: a lateral that outgrows its parent still branches off, and when the main child is shed or pruned the axis ends at its stump or at the fork instead of a lateral taking over. *Why:* re-routing an axis through a lateral would change that branch's shape the moment its neighbor is cut. The cost is visible cracks where a lateral is thicker than the continuing axis, which the junction meshes below are meant to fix.
2. **Smooth the skeleton.** Each axis becomes a chain of quadratic rational Bézier curves through internode midpoints, smooth along every root-to-tip path. One weight controls how sharp the corners are.
3. **Sweep generalized cylinders.** A cross-section ring is swept along each curve using a parallel-transport frame (no twisting). Because each segment is planar, the frame at any point has a closed form, so every vertex can be computed independently. Radius is planned to blend smoothly between internodes; for now rings at each node are joined by straight segments. Ring resolution scales with radius, so trunks get many sides and twigs few. Texture coordinates use a cheap arc-length approximation.
4. **Place foliage.** Leaves attach to branches below a width threshold, independently of buds. Each leaf is turned toward its preferred orientation by modelling the petiole as a rod that twists and bends. Leaves are instances of a few card or mesh templates, not unique geometry. Fruit and flowers use the same mechanism with a light-weighted probability.

**Level of detail.** Distant trees drop thin branches below a screen-size threshold (replacing them with leaf clusters), reduce ring resolution, and finally become impostors. The thickness-ordered tree graph makes this a simple radius cut.

**Known weakness.** Overlapping cylinders look wrong at thick forks. The planned fix is a dedicated junction mesh used only where both branches are thick, keeping the cheap path for the many thin forks.

## Risks and scope decisions

| Risk | Consequence | Mitigation |
| --- | --- | --- |
| Marker assignment cost | Interactivity collapses around 5k internodes with brute force (Hart 2020) | Spatial index from day one; incremental updates per step |
| Shedding fights the user | Painted branches vanish in shade | Light exponent above 1, normalized shoot length, protected branches |
| Parameter sensitivity | λ near 0.5 swings tree form sharply | Species presets and narrow slider ranges |
| Thick-fork junctions | Visible overlapping cylinders on hero trees, worst where a lateral outgrows the continuing axis | Junction meshes |

**Scope decisions**

- **Runtime:** a real-time 3D rendering engine. This favours instanced leaf cards, GPU-friendly mesh buffers and aggressive LOD.
- **Platform:** desktop only for now; VR and tablet interfaces are future work.
- **Edits:** every growth step and edit is recorded in a full history for undo and redo. Changing parameters at run time is out of scope.
- **Bending:** self-weight bending will not be implemented; interactive bending by the user remains.

## References

All in the Trees project; detail in `claude/paper-summaries.md`.

- Pałubicki et al. (2009). *Self-organizing tree models for image synthesis.* ACM TOG 28(3).
- Longay (2014). *Interactive Procedural Modelling of Trees and Landscapes.* PhD thesis, University of Calgary.
- Hart (2020). *Interactive Visualization and Modeling of Plant Structures in Virtual Reality.* MSc thesis, University of Calgary.
- Smelik et al. (2014). *A Survey on Procedural Modeling for Virtual Worlds.* Computer Graphics Forum 33(6).
