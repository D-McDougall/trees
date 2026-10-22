# Species-dependent foliage: review and implementation plan

Status: initial core foliage schema and stored placement implemented; rendering remains planned. Updated 2026-10-22.

The first implementation spans the original units 1–3 at the user’s request: species fields plus stored tree attachments and initial frames. See [the implemented contract](foliage.md). Unit 4 (petiole reorientation) and subsequent rendering/UI/storage units remain pending. The core owns stored leaves rather than generating them only in `trees-mesh`, following the user’s explicit data-model requirement.

A temporary viewer debug mode now draws fixed four-inch disks at initial blade centers, with a visibility toggle, placement count and foliage parameter widgets. This provides placement inspection ahead of the final leaf geometry/material and upload units; petiole reorientation remains pending.

## Findings from the literature

The supplied references do not define a single interchangeable leaf algorithm.

* **Longay (2014), Appendix C.1, printed pp. 141–145** is the most direct match for the existing design. C.1.1 explicitly decouples leaf placement from buds and places leaves on branches below a user-defined width threshold, allowing density changes without regenerating internodes. C.1.2 starts with phyllotactic orientations and reorients leaves through petiole bending and twisting. It models an inextensible rod with frame H, L, U and target tropism T. The rotation rates are ΩH = −eH L·T, ΩL = eL H·T, and ΩU = eU L·(T×(H×T)). Integrate the rates along petiole segments, multiplying by segment length; the final frame and endpoint position the blade. Figures C.2–C.3 provide visual references. The chapter does not specify a complete sampling/counting rule for independent leaf density; that rule must be documented as our implementation choice.
* **Hart (2020), §10.2, printed pp. 106–110**, instead attaches leaves beneath buds on internodes below a radius threshold. Lateral leaves start in the direction an internode from that bud would grow. Hart also adds leaves at main buds, explicitly acknowledging this as a non-biological fullness adjustment. Its orientation procedure differs from Longay's rod model: three small iterative rotations, with independently adjustable elasticities and typically 10–20 iterations. These methods should not be silently mixed or described as equivalent.
* **Pałubicki et al. (2009), §3**, establishes the node/internode/metamer and axillary-bud relationships used by the core model. It supplies the growth architecture rather than a detailed implementation of Longay's independent foliage placement.
* **Makowski et al. (2019), rendering discussion immediately before §7.1**, uses textured quads for interactive foliage and procedural Houdini foliage for offline renders. This supports cards as a rendering representation, not an alternative leaf attachment algorithm.
* **Smelik et al. (2014)** provides procedural-modeling context; it is not the implementation specification for this feature.

Sources are the supplied files in `docs`, including Hart's text transcription. Longay's placement text, frame illustration, and torque equations were also checked on rendered PDF pages.

## Baseline review before implementation

* `trees-core` has immutable `Arc<TreeSpecies>` records, validated alternate/opposite branching, divergence and branching angles, node frames, seed, birth steps, radii and stump flags. It has no foliage parameters or texture identity.
* `trees-mesh` produces engine-independent wireframe and straight tube buffers. Its documentation anticipates leaf instances, but no leaf API exists. Keep bark and foliage separate: they need different materials and update schedules.
* The Bevy viewer uploads one bark mesh in `show_tree`. Width pins produce a new `ShownTree` with recomputed radii. Foliage eligibility must use that displayed geometry and be recomputed after pin changes.
* The egui panel clones `TreeParams`; any change regrows the stand-in tree. Texture replacement should update a material without regrowing branches. Foliage-only tuning should similarly rebuild foliage without changing branch topology.
* The preset selector is currently an empty mockup even though library broadleaf/conifer presets exist. Species-specific appearance needs a working selection/association policy.
* Native and browser builds exist, with GitHub Pages deployment. The design's older desktop-only scope statement is stale. Uploads need separate native/browser entry points feeding a common decoder.
* No actual growth simulator, asset persistence, or project save/load system exists. Foliage does not need to wait for the simulator. Simulated leaf lighting, seasons, wind, fruit, LOD and new growth algorithms are separate work.

## Accepted baseline

Follow Longay C.1, matching `design.md`: independently spaced leaves on eligible thin branches, species-controlled phyllotaxis and petiole response, then textured leaf cards. Exclude stumps and zero-length root geometry. Define the threshold in radius units and state its relation to the paper's “width.” Define equality, local taper sampling, and branch-surface attachment explicitly.

Use distance-based sampling along each main-child axis, carrying residual distance and phyllotactic phase across internodes; lateral axes start their own sequence. This avoids density depending accidentally on how a branch is subdivided. It is an engineering specification beyond the paper, to be recorded as such. Use stable axis/path and sample identities for seeded variation. Changing a texture must never move leaves.

Store validated botanical foliage parameters in the species record. Associate species with an engine-neutral appearance/asset identifier, with actual decoded images and Bevy handles owned by the viewer. Start with one full-image texture per species, transparent background, blade base at bottom center and tip pointing up. Use alpha-masked, double-sided lit cards and a separate petiole representation. Use a generic broadleaf as the default and the only foliage form required for this milestone. The user will supply prepared leaf images. Provide a fallback so the default renders before a custom image is loaded. Keep attachment, organ geometry and appearance separate so future conifer parameters and templates can be added without rewriting broadleaf placement; do not claim that a broadleaf texture on the existing conifer preset models needles. Defaults are illustrative unless calibrated against named species.

## Small implementation units

Each row is intended as a separately reviewable change with its own documentation and validation. Dependencies are shown in the second column.

| Unit | Depends on | Implementation and documentation | Validation |
| --- | --- | --- | --- |
| 1. Algorithm contract | Accepted Longay placement | Update design with chosen source, radius semantics, sampling/phase, frame handedness, units, determinism, and explicit deviations from the paper. Add tiny synthetic-tree fixtures. | Hand-work expected attachments and initial frames for straight, bent, alternate and opposite axes. |
| 2. Species foliage schema | 1 | Add density/spacing, radius cutoff, blade dimensions, petiole length, target orientation and three elasticities; validate finite values and bounds. Define disabled foliage. Add appearance IDs and preset defaults without engine types. | Valid/invalid inputs, zero-size/density behavior, preset validation; existing core tests. Document every field and unit. |
| 3. Attachment sampling | 2 | Add an engine-neutral foliage output API, separate from bark buffers. Sample eligible axes, preserve spacing/phase, attach at the surface, exclude stumps, derive stable identities. Document sampling choices and radius-pin behavior. | Exact simple-tree counts/positions, cutoff boundaries, bent/lateral axes, zero lengths, deterministic rebuilds, equivalent subdivisions, prune/shed fixtures and pin changes. |
| 4. Petiole orientation | 3 | Implement Longay's framed rod integration as an isolated function returning petiole points and blade frame. Keep solver resolution separate from biological parameters. Document equations and sign conventions. | Zero elasticity gives a straight rod; targeted nonzero torque cases, orthonormal/finite frames, rod length, vertical/parallel degeneracies, convergence with finer segmentation. |
| 5. Leaf/petiole geometry | 4 | Add a reusable UV-mapped blade template and compact leaf instances; generate petiole geometry from solver output. Define blade origin, normal, dimensions and UV convention. | Blade base equals petiole endpoint, dimensions/orientation and UV tests, valid geometry, no NaNs or degenerate petioles at zero length. |
| 6. Viewer rendering | 5 | Add shared leaf mesh/material rendering, bundled default textures, alpha masking, correct two-sided normals and shadows. Rebuild from `ShownTree` after growth or width pins; release replaced assets. Prefer shared rendering assets, measure before custom GPU instancing. | Native visual checks from above/below and grazing angles, transparent silhouettes, shadow shape, pin eligibility, repeated rebuild resource use; browser smoke build/run. Record leaf counts and frame timing on a fixed seed. |
| 7. Species and foliage controls | 2, 6 | Wire presets and appearance association; add foliage controls, visibility toggle and reset. Split geometry/material invalidation from branch regrowth. Document switching presets and custom overrides. | Preset switches update geometry and texture together; texture/visibility changes preserve branch topology and leaf transforms; foliage tuning preserves width pins. |
| 8. Common image ingestion | 6 | Decode transparent PNG bytes into viewer-owned assets with orientation/alpha handling, dimensions/byte limits and clear errors. Preserve last valid texture on failure. Document accepted input and image preparation. | Transparent PNG, opaque PNG, rejected JPEG, malformed/unsupported/oversized input, aspect ratio behavior, successful replacement and repeated asset cleanup. |
| 9. Native file selection | 7, 8 | Add asynchronous file selection, preview, replace and reset for the active species. Keep filesystem details out of core. | Cancel, invalid selection, repeat upload, switch species, reset; UI remains responsive and keeps the previous image on failure. |
| 10. Browser upload | 7, 8 | Add browser file selection using bytes, no server upload required. Preserve per-species association and ignore stale asynchronous results after a species change. Document persistent storage behavior. | Served WASM app: select/cancel/replace/reset, errors, species switch during decoding, transparent edges and aspect ratio; native compilation remains intact. |
| 11. Persistent appearance settings | 7–10 | Store imported image bytes, species association and foliage settings in a versioned local record. Native: use an application data directory and atomic replacement. Browser: use IndexedDB behind a storage adapter. Copy bytes rather than retaining a selected-file path. Restore on startup; reset removes the override. Report storage failures while retaining the working in-memory image. | Restart/reload restores the same image, settings and species association; moving the source file does not break restoration; corrupt records, quota/unavailable storage, failed writes and reset have defined fallback behavior. |
| 12. Integration and delivery | 1–11 | Update README/design and image preparation guide; record source-to-code mapping, deviations, visual fixtures, supported platforms and measured limits. Add focused regression coverage for interactions. | `cargo test --workspace`, formatting/lint checks appropriate to the repository, native launch and Trunk release build; manually verify both upload flows and deterministic seed/pin examples. |

Implementation order can proceed through units 1–6 before upload work. Image ingestion can be developed after the material contract is fixed. Persistence is now part of the milestone. Calibrated conifer needle arrangements remain future work. Storage choices above are proposed implementation details; browser persistence is local to the site/browser profile and can be cleared or evicted, so it cannot promise permanent retention or cross-device sync.

## User decisions and remaining clarification

1. **Placement:** Longay C.1, independent of branch/bud placement.
2. **Species:** generic broadleaf default now; conifers later, with extensible parameters and geometry.
3. **Images:** user-supplied cropped images with transparency; later expose the same prepared-image workflow to users. No automatic segmentation or cropping is required.
4. **Persistence:** preserve custom textures and their species association across restarts where supported. Persist foliage settings too so restored textures retain their intended size and placement; full project/save-history persistence remains outside this feature.

**Image format confirmed:** cropped PNG files with alpha transparency. Use the same format for supplied assets and future user imports. The initial workflow uses one blade per image, base at bottom center and tip pointing up; show this convention in the upload guidance. No JPEG support, separate alpha masks, background removal or crop editor is required.

All four planning questions are resolved. Native/browser storage mechanisms and sampling details remain documented engineering choices to validate during implementation.

Assume both native and browser support because both are already shipped. No atlas or multi-leaf image support is required. No performance budget is supplied: measure the existing seven-step tree first and report results before deciding whether custom GPU instancing is necessary.
