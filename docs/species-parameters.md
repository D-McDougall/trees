# Species parameter reference

All five PDFs and the TXT document in `docs/` were reviewed for this reference.
The two Longay PDF files are consecutive parts of one thesis. Citations below use
printed section/page numbering, not PDF-viewer page indices. Explanations are
paraphrases. Source-specific formulas are distinguished from this project's
sampling, foliage and preview conventions.

## Reading the model

An internode is a stem segment between nodes; a shoot is the sequence produced
by one bud during a growth increment. Branches emerge from competition between
potential buds, so no one parameter directly prescribes the final silhouette.
Physical quantities remain stored in metres; UI lengths use ft with exactly
1 ft = 0.3048 m. Angles are stored in radians and shown in °. Perception angle is
a half-angle. Model coefficients and the gravimorphism ellipse are dimensionless.

The current preview uses occupancy-aware space colonization with at most six
steps and 256 nodes. Each successful bud produces one constant-length internode
in the mean unit direction of its won markers. Vigor, shadow propagation,
direction blending, gravimorphism and automatic shedding are pending. The
corresponding help describes the intended literature model, not active behavior.

Smelik et al. (2014), *A Survey on Procedural Modeling for Virtual Worlds*, §3
and §§10.2–10.3, motivates explaining coupled parameters and preserving designer
intent; it is a survey, not the primary source for the growth equations.
Makowski et al. (2019), *Synthetic Silviculture: Multi-scale Modeling of Plant
Ecosystems*, §§4–6, adds module-scale growth and ecosystem plasticity. Its climate,
seeding, physiological-age and module parameters are not implemented species
controls in this application; they must not be conflated with the bud model here.
Hart's §§9–10 and Longay's Chapter 4, §5.3.2 and Appendix C supply the main detailed
comparisons with Pałubicki et al. (2009), §§4–5.

## Parameter help

These sections are the text used by the viewer's label tooltips. Keep their
headings synchronized with the corresponding editor labels.

### Seed

Status: Viewer control.

Selects a reproducible realization of the tree rather than a biological species trait. Keep this integer fixed while comparing parameters so that changes in the marker distribution do not obscure their effects. Changing it changes the jittered free-space samples and initial foliage phases, so the same species can produce different individuals. With the same seed and parameters, this implementation repeats the same result. The papers motivate stochastic variation; the particular seeded grid is a project choice.

Source: Pałubicki et al. (2009), Self-organizing tree models for image synthesis, §§4.1, 5; project sampling convention.

### Point spacing

Status: Simulated.

Width of a sampling cell, displayed in ft. The sampler supplies one marker per cubic cell before occupancy and perception filtering; it does not enforce a minimum pairwise separation equal to this value. Smaller cells provide more samples of free space and cost more work; larger cells can miss small openings or leave a bud without markers. Tune this together with occupancy radius and perception distance. This preview limits perception distance to four sampling cells to bound the synchronous computation; spacing is not internode length.

Source: Pałubicki et al. (2009), Self-organizing tree models for image synthesis, §4.1; project sampling convention.

### Point jitter

Status: Simulated.

Displaces each marker from its cell center by a seeded random offset in each coordinate. This number is a dimensionless fraction of cell width, not a distance in ft. Zero gives a regular grid, which can reveal alignment artifacts; increasing it breaks that regularity while keeping one marker per cell. Values stay below one half so samples remain inside their own cells. Keep seed and spacing fixed to compare jitter settings. The restricted jitter grid is an implementation choice for sampling the free space described in the literature.

Source: Pałubicki et al. (2009), Self-organizing tree models for image synthesis, §4.1; project sampling convention.

### Preset

Status: Not connected.

A preset is a reusable bundle of species settings, not a single growth coefficient. The literature uses saved parameter sets to recover an architectural character and then varies environmental guidance or edits to create individuals. In this viewer the displayed name identifies the current species, but this menu has no selectable presets yet. It does not load settings or switch to the core library’s conifer configuration. Use the individual controls below to change the current species.

Source: Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, §5.1 and Appendix E.

### Branching

Status: Simulated.

Chooses how many lateral buds occur at each node. Alternate places one; Opposite places two facing one another. A bud is a potential branch origin, not a guarantee that a branch will appear: it must win free-space markers to grow. Use alternate for a single sequence of lateral opportunities and opposite for paired opportunities such as those seen in maples. Divergence rotates successive nodes around the axis, while branching angle tilts the buds away from it. Leaf arrangement is edited independently.

Source: Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, §4.1; Hart (2020), Interactive Visualization and Modeling of Plant Structures in Virtual Reality, §9.3.

### Divergence φ

Status: Simulated.

Rotation around a branch axis between the lateral-bud frames at consecutive nodes, displayed in °. About 137.5° gives the familiar golden-angle spiral; 180° places alternate buds on alternating sides, and 90° rotates successive opposite pairs into crossed planes. Use this to set the local phyllotactic pattern before tuning competition. Actual visible branches may skip positions because not every bud develops. This angle does not bend the branch itself and does not change the independently controlled leaf divergence.

Source: Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, §4.1; Hart (2020), Interactive Visualization and Modeling of Plant Structures in Virtual Reality, §§9.1–9.3.

### Branching ψ

Status: Simulated.

Initial angle between a lateral bud and the heading of its supporting internode, in °. Small values point potential branches along their parent; larger values spread their perception cones outward. Here marker sensing uses that initial bud direction and then grows toward the average unit direction of won markers, so the final branch angle need not exactly equal the setting. Tune together with perception half-angle and divergence. The effect of branch angle on the whole crown emerges through competition rather than directly prescribing a silhouette.

Source: Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, §4.1, Fig. 4.2; Hart (2020), Interactive Visualization and Modeling of Plant Structures in Virtual Reality, §9.3.

### Deflection δ

Status: Simulated.

Angle in ° by which the continuing main bud points away from the lateral side. Zero keeps its initial heading aligned with its parent; a larger value can encourage zigzag or gnarled axes. In the single-lateral construction, matching the deflection and branching angles creates symmetric Y-shaped bud directions. Unlike Hart’s opposite-branching variant, this implementation also deflects the main bud when opposite lateral pairs are selected. Marker-guided growth can further change the realized angles; this is a bud-orientation rule, not mechanical bending of existing wood.

Source: Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, §4.4.4; Hart (2020), Interactive Visualization and Modeling of Plant Structures in Virtual Reality, §9.3.

### Internode length

Status: Simulated.

Length of each newly added stem segment between nodes, displayed in ft. Increasing it makes growth advance farther per step and spaces branching opportunities farther apart; decreasing it gives finer structural detail and can require more steps to reach the same size. The current solver adds one constant-length internode per successful bud per step. Hart also investigates lengths varying with vigor or branch order, but those variants are not used here. Editing this value does not automatically rescale sampling, foliage dimensions, or other species lengths.

Source: Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, §4.1; Hart (2020), Interactive Visualization and Modeling of Plant Structures in Virtual Reality, §9.7.

### Leaves enabled

Status: Simulated.

Turns foliage placement on or off for this species. Disable it to inspect branch architecture and thickness without foliage, or to represent a leafless tree. Re-enabling it restores deterministic attachments from the current seed and foliage settings. Leaves attach at distance stations on sufficiently thin branches; they are not one-to-one copies of branch buds. In the current viewer foliage is inspected with the debug leaf disks rather than a finished textured canopy, and its visibility does not supply simulated light or vigor feedback.

Source: Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, Appendix C.1.

### Arrangement

Status: Simulated.

Chooses one leaf per attachment station (Alternate) or a facing pair (Opposite). This setting is intentionally independent of branching: the literature separates foliage density from the branch skeleton to support artistic control without regrowing a finer network of internodes. Use opposite pairs for paired foliage and alternate for a single rotating sequence. Leaf spacing sets the distance between stations; leaf divergence rotates successive stations. The initial frame is then eligible for petiole reorientation in the core rod solver, which the debug disks do not visualize.

Source: Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, §§4.1, C.1.1; project foliage convention.

### Leaf spacing

Status: Simulated.

Distance in ft between foliage attachment stations measured along each continuous branch axis. Smaller spacing adds a denser canopy without shortening internodes or adding branch buds; larger spacing reveals more wood and can suggest a smaller or more open tree. Opposite arrangement produces two leaves at each eligible station. Stations failing the maximum-radius cutoff are skipped, so the actual count also depends on thickness. The project places stations at half-spacing offsets and continues the station sequence through consecutive main-child internodes.

Source: Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, Appendix C.1.1; project distance-station convention.

### Maximum branch radius

Status: Simulated.

Foliage attaches only where the interpolated supporting branch radius is strictly below this cutoff, shown in ft. This is a radius, not a diameter. Lower it to keep leaves near slender outer twigs and expose the trunk; raise it to let foliage extend onto thicker wood. Zero suppresses all attachments. Since eligibility depends on pipe-model thickness and width pins, changing those can change leaf counts even with the same leaf spacing. The criterion is a modeling approximation to foliage on young shoots, not a chronological age measurement.

Source: Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, Appendix C.1.1.

### Leaf divergence

Status: Simulated.

Rotation in ° between successive foliage attachment stations around the supporting axis. About 137.5° creates a spiral; 180° alternates sides; for opposite pairs, 90° gives crossed successive pairs. This sets the initial phyllotactic orientation independently of branch-bud divergence, allowing canopy tuning without changing the branch skeleton. The seed supplies a reproducible phase for each axis, so the first leaf need not face a fixed compass direction. A radius-rejected station still advances the sequence rather than resetting the angular pattern.

Source: Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, Appendix C.1.2; project foliage convention.

### Attachment angle

Status: Simulated.

Initial angle in ° between the petiole heading and its supporting branch heading. Zero lies along the branch; 90° points radially outward; angles above 90° point back toward the branch base. Use it to establish the unbent orientation of the foliage before any petiole response. It does not directly specify the final blade-normal inclination relative to gravity. This viewer’s debug disks show initial frames; the separate core petiole solver can bend and twist those frames toward a preferred normal direction.

Source: Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, Appendix C.1.2; Hart (2020), Interactive Visualization and Modeling of Plant Structures in Virtual Reality, §10.2.

### Petiole length

Status: Simulated in initial debug geometry; rod response available in core.

Length of the stalk connecting bark to blade, displayed in ft. Longer stalks move blades farther from their attachments and, in the core rod model, provide more arc length over which bending, twisting and turning can accumulate. Zero places the blade base directly at the attachment and leaves no rod length for reorientation. The debug view currently draws a straight stalk in the initial heading, not the solved curved rod. Changing length does not move the attachment stations or alter branch growth.

Source: Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, Appendix C.1.2; Hart (2020), Interactive Visualization and Modeling of Plant Structures in Virtual Reality, §10.2.2.

### Blade length

Status: Simulated in debug geometry.

Blade dimension from base to tip, in ft, excluding the petiole. Larger blades make the same branch structure read as coarser or smaller in scale; smaller blades can make it appear more extensive, particularly with denser spacing. The default dimension is 1/3 ft. Change blade width separately to adjust aspect ratio. A zero length suppresses leaf placement. The current debug view uses an elliptical disk to inspect dimensions; it does not render the final blade silhouette or compute photosynthesis or leaf-driven shadows.

Source: Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, Appendix C.1.1; Hart (2020), Interactive Visualization and Modeling of Plant Structures in Virtual Reality, §10.2.

### Blade width

Status: Simulated in debug geometry.

Transverse blade dimension in ft, independent of its base-to-tip length. Increase width for a broader leaf and stronger apparent canopy coverage, or reduce it for a narrow blade. Tune it with spacing and length: foliage appearance depends on all three, so increasing leaf size is not equivalent to increasing the number of leaves. A zero width suppresses placement. The default width is 1/3 ft. The debug disk represents the two blade dimensions rather than a detailed leaf outline, and blade dimensions do not currently shade or inhibit tree growth.

Source: Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, Appendix C.1.1; Hart (2020), Interactive Visualization and Modeling of Plant Structures in Virtual Reality, §10.2.

### Model

Status: Not simulated.

Chooses the intended resource-allocation rule. Extended Borchert–Honda splits vigor locally at each branching point according to weighted light contributions. The priority model instead ranks branches along an entire axis by mean light and allocates vigor using rank weights, allowing different architectural biases. These are abstractions of growth regulation, not literal predictions of hormone or water transport. In the present preview both models are pending: buds grow through marker competition alone. The choice and the controls below therefore cannot yet change growth.

Source: Pałubicki et al. (2009), Self-organizing tree models for image synthesis, §4.2; Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, §§4.2.2–4.3.

### w_min

Status: Not simulated.

Lowest relative allocation weight in the priority model. Branches below the top ranked fraction receive this weight; the highest ranked branch starts at w_max. Lowering w_min relative to w_max concentrates resources in favored axes, potentially emphasizing a trunk and reducing subordinate growth. Bringing the two weights together reduces rank-based preference. These weights are dimensionless and are normalized during allocation, so their contrast matters more than a common scale. This model is not yet evaluated by the preview.

Source: Pałubicki et al. (2009), Self-organizing tree models for image synthesis, §4.2, Fig. 8c.

### w_max

Status: Not simulated.

Weight at the favored end of the priority ranking. Increase it relative to w_min to strengthen preferential allocation to highly ranked branches; approach w_min for a flatter allocation profile. The priority method orders branches using light per bud and can give the terminal bud priority when apical control is present. It operates over a whole axis, unlike the local Borchert–Honda split. This is a relative dimensionless weight, not a length or an absolute resource supply, and priority allocation is not simulated yet.

Source: Pałubicki et al. (2009), Self-organizing tree models for image synthesis, §4.2, Fig. 8c.

### Top fraction

Status: Not simulated.

Fraction of the priority list over which weights fall from w_max to w_min. A small fraction concentrates the weight advantage near the beginning of the ranking, while a larger fraction spreads the gradient over more ranks. The remaining branches use w_min. For example, 0.5 means half the ranks, not half the tree’s physical height or a count of internodes. Compare it together with the weight contrast when designing a strong leader versus a more distributed crown. Priority allocation is pending in this preview.

Source: Pałubicki et al. (2009), Self-organizing tree models for image synthesis, §4.2, Fig. 8c.

### Apical control λ

Status: Not simulated.

Bias between the continuing main axis and each lateral contribution in the extended Borchert–Honda model. Main demand is weighted by λ and lateral demand by 1−λ before normalization. At 0.5 allocation follows light without a main/lateral bias; above 0.5 favors a leader and an excurrent form; below 0.5 favors spreading, decurrent forms. Small changes near 0.5 can accumulate strongly through repeated branching, so adjust gradually. The model is a growth-regulation abstraction, and the current marker-only preview does not apply this parameter.

Source: Pałubicki et al. (2009), Self-organizing tree models for image synthesis, §4.2; Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, §4.2.2, Eq. 4.1.

### Light exponent κ

Status: Not simulated.

Controls the nonlinear conversion from a bud’s light exposure L to its allocation signal Q=b·L^κ, with b supplied by gravimorphism. With normalized light between zero and one, values above one emphasize better-lit buds and reduce the relative signal from shaded buds; values between zero and one flatten those differences. Zero removes sensitivity to light. Increasing this selectivity can avoid creating surplus branches and reduce reliance on later shedding. It is dimensionless and should be tuned with the light model and activation threshold. None of these light-to-vigor effects are simulated yet.

Source: Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, §4.3.2, Eqs. 4.2, 4.7.

### Root vigor scale α

Status: Not simulated.

Converts the light accumulated at the tree base into a resource signal: v_root=α·Q_root. Larger values increase available vigor and can let more buds pass an absolute activation threshold; smaller values can suppress growth. Longay normalizes shoot length relative to the most vigorous enabled bud, so a common scale does not independently multiply all shoot lengths in that model. Tune alongside activation threshold rather than treating α as physical tree size. This α is unrelated to the straightness weight using the same Greek symbol. Vigor allocation is pending.

Source: Pałubicki et al. (2009), Self-organizing tree models for image synthesis, §4.2; Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, §4.3.2.

### Activation threshold

Status: Not simulated.

Minimum vigor required before a bud that has won free-space markers is allowed to grow. Raising the threshold makes activation more selective, reducing the number of new shoots; lowering it admits weaker buds and can fill the crown more densely. Light, apical control, gravimorphism and root vigor scale all affect whether the threshold is reached. It is a dimensionless model threshold, not an illumination value or a length. In the present preview a nonzero marker direction activates a bud without consulting vigor, so this control has no effect yet.

Source: Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, §4.3.2; Hart (2020), Interactive Visualization and Modeling of Plant Structures in Virtual Reality, §9.5.1.

### Max shoot length (internodes)

Status: Not simulated.

Upper limit on the number of internodes produced by one bud in one growth step. Longay uses n=ceil((v/v_max)·n_max), so the most vigorous enabled bud produces up to this count and other enabled buds produce proportionally shorter shoots. Larger counts can produce long, wispy or pendulous branches when combined with downward tropism; smaller counts give compact increments. This is a count, not ft, and physical shoot length also depends on internode length. The current preview always adds one internode per successful bud, so this limit is pending.

Source: Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, §4.3.2, Eq. 4.3; §4.4.2.

### Lateral set-point θ

Status: Not simulated.

Preferred inclination of axes of order greater than zero, measured in ° from vertical up. Here 0° means upward, 90° means horizontal (plagiotropic), and 180° means downward. A local target direction is chosen in the vertical plane of each heading so branches can spread radially rather than all following one compass direction. The gravitropism weight determines how strongly growth follows this target; the angle alone is not the turning strength. This model concerns new growth rather than bending existing branches under weight. The preview does not apply tropism yet.

Source: Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, §4.4.2, Fig. 4.16.

### Main set-point

Status: Not simulated.

Preferred inclination in ° from vertical up for the main, order-zero axis. A 0° main set-point paired with roughly horizontal lateral set-points can create the upright leader and spreading tiers characteristic of conifer-like forms. Use separate values to distinguish trunk behavior from branches, rather than forcing one global direction on the entire tree. As with the lateral set-point, its influence depends on the gravitropism blend weight and competition. This is a new-growth direction target, not an instruction to rotate existing trunk geometry, and is pending in the preview.

Source: Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, §4.4.2, Fig. 4.19.

### Direction weights

Status: Preview only; not simulated.

Blends three unit directions: straightness α preserves the current bud heading, gravitropism β points toward the appropriate set-point inclination, and environment γ points toward free-space markers. The intended direction is normalize(αH+βT+γE). Move toward a corner to strengthen that influence, toward an edge to remove the opposite influence, or toward the center for a balanced mix. The three dimensionless weights sum to 100%, so increasing one reduces the others. High straightness preserves branch paths; high environment adapts them to openings. The picker edits stored values, but the current preview uses only marker direction.

Source: Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, §§4.4.1–4.4.2, 5.1.1; Hart (2020), Interactive Visualization and Modeling of Plant Structures in Virtual Reality, §11.4.4.

### Vertical axis

Status: Not simulated.

Vertical semi-axis s_v of a control ellipse perpendicular to the supporting branch. Distance to the ellipse in a bud’s radial direction sets its relative growth propensity. With zero offset, increasing s_v relative to the horizontal semi-axis favors buds on both upper and lower sides; reducing it favors side-facing buds. A unit circle centered on the branch is neutral. These are dimensionless propensity coordinates, not world lengths in ft. The effect is strongest on horizontal branches and fades on vertical ones. Gravimorphism is not yet simulated.

Source: Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, §4.4.3, Figs. 4.20–4.22.

### Horizontal axis

Status: Not simulated.

Horizontal semi-axis s_p of the gravimorphism ellipse, in the plane perpendicular to the branch. With a centered ellipse, increasing it relative to the vertical semi-axis favors lateral positions on the sides of horizontal branches (amphitony). Decreasing it accentuates upper/lower positions. The radial distance d is blended into the propensity as b=cos²τ+d·sin²τ, where τ is the parent’s inclination from vertical. This is a relative bud-activation bias rather than bending or branch dimensions. Use together with vertical axis and offset; the preview does not evaluate it yet.

Source: Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, §4.4.3, Figs. 4.20–4.22.

### Vertical offset

Status: Not simulated.

Shifts the gravimorphism control ellipse relative to the branch axis in its most-vertical perpendicular direction. A positive offset favors upper-side buds (epitony); a negative offset favors underside buds (hypotony). Zero removes this upper-versus-lower preference but an unequal ellipse can still favor sides or top/bottom together. Use modest offsets relative to the vertical semi-axis: large offsets can put the branch axis outside the ellipse, and the pending solver has no defined policy for that case yet. This is a dimensionless propensity parameter, not a displacement in ft; no gravimorphic bias is currently applied.

Source: Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, §§2.1, 4.4.3.

### Occupancy radius ρ

Status: Simulated.

Radius in ft around each live node within which markers are considered occupied and excluded from competition. Increasing it reserves more space around existing growth, usually reducing crowding and leaving a more open branching structure; decreasing it permits closer competing shoots. This is not the geometric wood radius and does not detect intersections of entire branch segments. Tune relative to internode length, sample spacing and perception distance. It must remain smaller than perception distance so buds have a nonempty region of free space to sense. Stumps also exclude nearby markers here.

Source: Pałubicki et al. (2009), Self-organizing tree models for image synthesis, §4.1; Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, §§4.2.1, 4.3.3.

### Perception angle θ

Status: Simulated.

Half-angle in ° from a bud’s forward direction to the boundary of its perception cone. A 45° setting spans 90° across the cone; 90° sees the forward hemisphere; larger values include some markers behind the bud. Narrow angles reinforce existing architecture but may leave buds unable to see an opening. Wider angles improve access to surrounding space but can allow more abrupt direction changes. A marker must also be within perception distance and outside all occupied regions. Eligible buds compete for markers, with the closest winning; this is not a camera field of view.

Source: Pałubicki et al. (2009), Self-organizing tree models for image synthesis, §4.1; Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, §4.2.1.

### Perception distance r

Status: Simulated.

Maximum distance in ft from a bud to markers it may claim, measured from the bud itself rather than from the edge of its occupancy sphere. Larger reach lets a bud sense more distant openings but increases computation and competition with neighbors; shorter reach makes decisions local and may stall sparsely sampled buds. Keep it greater than occupancy radius. The viewer caps it at four times point spacing to bound work. It changes what a bud senses, not the length of the internode grown toward the resulting average direction.

Source: Pałubicki et al. (2009), Self-organizing tree models for image synthesis, §4.1; Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, §4.2.1.

### Markers per vigor

Status: Not simulated.

Intended density multiplier for autonomous generation of attraction points in bud perception volumes, relative to vigor. Longay generates at least one point per bud and proportionally more for vigorous buds to support reliable autonomous growth, then clears unused points between iterations. In this project the stored coefficient is reserved for that future generator, with zero intended to disable it. The present preview instead queries a reproducible spatial grid controlled by point spacing and jitter, so this coefficient does not alter marker counts or branch growth yet. It is a dimensionless coefficient, not ft.

Source: Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, §4.3.1.

### Full light C

Status: Not simulated.

Reference exposure for an unshaded bud in the approximate light grid. Shadow contributions reduce this available light, and the resulting exposure would feed light sensitivity and resource allocation. Increasing the reference relative to shadow strength makes a given amount of shading less restrictive; decreasing it makes the same canopy shadow more consequential. This is an arbitrary model-light scale, not physical illumination units. Compare it together with shadow strength, falloff, depth and the light exponent. The current preview does not compute the light grid or use C to activate buds.

Source: Pałubicki et al. (2009), Self-organizing tree models for image synthesis, §4.1; Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, §§4.2.2–4.3.

### Strength a

Status: Not simulated.

Amplitude of the shadow contributed by each node to the grid below it. In the intended distance-based variant, a contribution is a·β^(−distance in grid cells), so a sets its scale rather than its spatial reach. Larger strength would make overlapping shadows inhibit lower growth more strongly; zero removes this source of shade. Tune it relative to full light C and falloff instead of treating it as rendered shadow opacity. The approximation does not measure actual leaf area or branch silhouette. Shadow propagation is pending in the preview.

Source: Pałubicki et al. (2009), Self-organizing tree models for image synthesis, §4.1; Hart (2020), Interactive Visualization and Modeling of Plant Structures in Virtual Reality, §9.5.2.

### Falloff β

Status: Not simulated.

Base of the exponential attenuation of shadow with distance, constrained above one. This project’s intended convention is a·β^(−distance), so increasing β makes shadows fade faster and remain close to their sources; bringing β toward one makes them persist farther. Hart writes the equivalent decay using a base below one, so the numerical direction is reversed from that notation. Depth separately truncates the shadow pyramid. Use slower decay for stronger long-range canopy suppression and faster decay for local shading. The light grid is not simulated yet.

Source: Pałubicki et al. (2009), Self-organizing tree models for image synthesis, §4.1; Hart (2020), Interactive Visualization and Modeling of Plant Structures in Virtual Reality, §9.5.2.

### Depth q_max

Status: Not simulated.

Number of grid-cell layers reached below a shadow-casting node. It is a count of cells, not a distance in ft or a count of growth steps. Greater depth includes more distant lower buds and a wider shadow pyramid, increasing both long-range interactions and grid work. Smaller depth confines shading to nearby growth; zero gives no downward layers. The eventual physical reach depends on light-grid cell size, while falloff controls the strength within that reach. The current preview has no active light grid, so depth has no effect yet.

Source: Pałubicki et al. (2009), Self-organizing tree models for image synthesis, §§4.1, 6; Hart (2020), Interactive Visualization and Modeling of Plant Structures in Virtual Reality, §9.5.2.

### Enabled

Status: Not simulated.

Enables the intended self-pruning of unproductive branches. The literature compares the resource benefit of retaining a branch with its maintenance burden and removes branches below a chosen threshold, helping form a clear lower bole. Disable shedding when preserving deliberately modeled branches is more important, and instead use selective bud activation to limit density. This checkbox is specifically for shedding, not leaf visibility or all tree growth. Automatic shedding is not currently called by the preview, so neither state changes its branches yet.

Source: Pałubicki et al. (2009), Self-organizing tree models for image synthesis, §4.4; Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, §4.3.2.

### Light per internode

Status: Not simulated.

Profitability cutoff for automatic branch shedding: compare gathered branch light divided by the number of supported internodes with this threshold. Raising it would reject more poorly illuminated branch systems and open the crown; lowering it would retain them. The literature also discusses a vigor-per-size variant; this project’s parameter specifies gathered light per internode. It is a model ratio, not ft, and is separate from the threshold that activates individual buds. Shedding can remove deliberately grown branches, but no automatic shedding or light evaluation is implemented in the preview yet.

Source: Pałubicki et al. (2009), Self-organizing tree models for image synthesis, §4.4; Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, §4.3.2.

### Tip radius

Status: Simulated.

Radius in ft assigned to a terminal internode, providing the boundary value for the pipe-model calculation r^n=Σr_i^n. Increasing it thickens tips and, through accumulation toward the base, usually thickens the trunk and supporting branches as well. Decreasing it gives finer twigs. This is a radius, not diameter, and does not control the free-space occupancy sphere. Width pins can override local thickness, while stumps retain their recorded radii. Since foliage uses a radius cutoff, thicker tips can also reduce the number of eligible leaf stations.

Source: Pałubicki et al. (2009), Self-organizing tree models for image synthesis, §4.5; Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, §5.3.2; Hart (2020), Interactive Visualization and Modeling of Plant Structures in Virtual Reality, §9.6.

### Pipe exponent n

Status: Simulated.

Exponent relating parent and child radii: r_parent=(Σr_child^n)^(1/n). At n=2 the rule conserves cross-sectional area, corresponding to the classical pipe-model approximation; values between two and three allow other branching proportions. For fixed tip radii and topology, increasing n makes supporting branches relatively thinner; decreasing it makes them thicker. This is dimensionless. Width pins partition the tree into regions with solved local exponents, so the species value may be overridden below a pin. The calculation describes structural thickness, not marker occupancy or a complete biomechanical model.

Source: Pałubicki et al. (2009), Self-organizing tree models for image synthesis, §4.5; Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, §5.3.2; Hart (2020), Interactive Visualization and Modeling of Plant Structures in Virtual Reality, §9.6.

### Debug sampling markers

Status: Viewer control.

Shows raw samples from the seeded spatial grid near up to sixteen live bud nodes, capped at four thousand markers. Use it to understand sample spacing, jitter and why perception settings may leave a bud without nearby candidates. These are raw candidates: some will be rejected by occupancy, habitat constraints or perception cones, or claimed by another bud. Turning this display on does not change sampling or regrow the tree. It visualizes the free-space representation used by space colonization, not actual leaves or light measurements.

Source: Pałubicki et al. (2009), Self-organizing tree models for image synthesis, §4.1; project diagnostic view.

### Debug leaf disks

Status: Viewer control.

Displays foliage attachments, straight petioles and elliptical leaf disks using their initial frames. Use it to inspect spacing, paired versus alternate arrangements, branch-radius eligibility and blade dimensions before finished leaf rendering is available. Disks are a diagnostic approximation rather than the final blade silhouette. They do not display the bending and twisting produced by the core petiole rod solver, and do not shade buds or influence growth. Toggling the view changes only the diagnostic display, not the tree’s species or seed.

Source: Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, Appendix C.1; project diagnostic view.

### Width radius

Status: Simulated.

Pins a selected branch radius in ft and recomputes the pipe-model exponent in the region below the constraint. Use it to create a heavier mature-looking trunk or emphasize a particular limb without adding branches. The slider and numeric entry edit the same radius; changing either creates a pin. Nested pins split the tree into independent constraint regions. If the requested radius cannot accommodate its supported boundary radii, the branch is widened and a warning is shown. This changes thickness and foliage eligibility rather than spatial growth parameters.

Source: Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, §5.3.2, Eqs. 5.1–5.3.

### Pinned

Status: Simulated.

Keeps a selected branch’s requested radius as a width constraint. Checking this records the current radius; dragging or typing a radius also pins it. Uncheck to remove that constraint and return the region to thickness determined by the remaining pins and the species pipe exponent. Pins use structural branch paths so they can survive regrowing the tree with new parameters. A pin whose branch no longer exists remains listed but inactive. It controls thickness, not a positional anchor for bending, and an infeasibly thin request is widened.

Source: Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, §5.3.2; project path-based pin persistence.

### Unpin

Status: Simulated.

Removes this radius constraint from the selected branch path. The tree then recomputes thickness using the remaining width constraints and the species pipe exponent, so radii in the affected region can change and foliage eligibility can change with them. Use this to undo a local thickness override without changing the branch skeleton, species settings or random seed. An inactive pin can also be removed from the list even when its branch is absent from the current tree.

Source: Longay (2014), Interactive Procedural Modelling of Trees and Landscapes, §5.3.2; project pin controls.

## Species settings without editor widgets

`name` identifies a parameter bundle and is not a biological coefficient.
`foliage.appearance` is an engine-neutral appearance key (default
`generic-broadleaf`); it selects no texture in the current debug view and does
not influence growth or leaf placement.

`foliage.petiole_response` implements Longay Appendix C.1.2, pp. 143–144. The
nonzero `target` vector is normalized and specifies a preferred blade-normal
direction; increasing its magnitude does not increase force. `twist` (eH),
`bend` (eL) and `turn` (eU) are nonnegative susceptibility rates per metre of
petiole arc length, not dimensionless direction weights or angles. The equations
are ΩH = −eH L·T, ΩL = eL H·T, and ΩU = eU L·(T×(H×T)). Here H is the rod tangent,
L blade-left and U the blade normal. Zero disables the associated rotation;
all three zero yield a straight petiole. Longer petioles accumulate more
response. The defaults are illustrative and are not measured material constants.
The core solver integrates these rates using framed rods and fixed arc-length
steps; its numerical segment count is a solver option, not a species trait.
Neither response settings nor segment count have editor widgets yet. The viewer
shows initial frames rather than solved rods. See [foliage.md](foliage.md) for
solver conventions and validation.

## Source files reviewed

- [Pałubicki et al., 2009](2009%20Self-organizing%20tree%20models%20for%20image%20synthesis.pdf): bud architecture, space/light competition, allocation, shedding, pipe model.
- [Longay, 2014, first part](2014%20Interactive%20Procedural%20Modelling%20of%20Trees%20and%20Landscapes%20pg1-85.pdf): unified growth and coupled architectural controls; width constraints.
- [Longay, 2014, second part](2014%20Interactive%20Procedural%20Modelling%20of%20Trees%20and%20Landscapes%20pg86-170.pdf): landscape context, foliage placement and petiole orientation.
- [Smelik et al., 2014](2014%20A%20Survey%20on%20Procedural%20Modeling%20for%20Virtual%20Worlds.pdf): procedural modeling and user-control context.
- [Makowski et al., 2019](2019%20Synthetic%20Silviculture%20Multi-scale%20Modeling%20of%20Plant%20Ecosystems.pdf): distinction between module, plant and ecosystem models.
- [Hart, 2020](2020%20Interactive%20Visualization%20and%20Modeling%20of%20Plant%20Structures%20in%20Virtual%20Reality.pdf.txt): Tree Wand growth, appearance and parameter controls; Part I concerns vascular visualization rather than this growth model.

Longay §2.1 and §4.4.3 define epitony as upper-side preference, hypotony as
underside preference and amphitony as side preference. The captions in Fig. 5.6
swap some terms, so this reference follows the definitions and ellipse model.
Hart uses a decay base below one, while this project's shadow parameters use its
reciprocal above one. These differences matter when transferring parameter
values between sources.
