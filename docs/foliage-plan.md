# Species-dependent foliage: review and implementation plan

Status: placement, petiole-response schema/solver and debug visualization complete; remaining work below. Updated 2026-10-22.

**Next focus: unit 5, production blade and petiole geometry using the rod
solver's endpoint and final frame.** Units 2 and 4 are now complete in core.
The debug viewer still displays initial frames; solver-driven rendering and
petiole-response widgets remain part of the pending integration work.

The implementation contract is in [foliage.md](foliage.md). Original unit numbers
are retained so earlier discussions still resolve. Completed work is struck out
below and removed from the remaining-work table. Partially completed units list
only their outstanding scope.

## Completed work

- ~~1. Algorithm contract: source mapping, radius cutoff and taper, distance sampling, axis phase, frame handedness, determinism, and synthetic-tree fixtures.~~
- ~~3. Attachment sampling: tree-owned leaf records, stable identities, surface attachments independent of buds, and refreshes after growth, pruning, shedding and radius updates.~~
- ~~2. Species schema: validated placement/dimensions, petiole length and response (target and three susceptibilities), appearance key and broadleaf defaults; conifer foliage disabled pending needle support.~~
- ~~4. Longay rod solver: isolated midpoint integration, path and final frame output, validation/errors, analytic and convergence tests, and documented equations and numerical conventions.~~
- ~~5–6, debug portion: batched two-sided debug disks, four-inch defaults, independent width/length scaling, bottom-center anchoring and UV orientation, refreshed from the displayed tree.~~
- ~~7, completed portion: foliage parameter widgets, debug visibility toggle and placement count; debug visibility changes do not regrow the tree.~~
- ~~12, completed portion: placement/debug documentation, regression and headless smoke tests, native visual inspection, and cleanup of shared taper calculations and arrangement widgets.~~

Production textured foliage, preset selection, uploads and persistence are not
implemented. Foliage parameter edits still deterministically regrow the stand-in
tree; they reproduce the same branch skeleton but do not yet avoid that work.

## Findings from the literature

The supplied references do not define a single interchangeable leaf algorithm.

* **Longay (2014), Appendix C.1, printed pp. 141–145** is the most direct match for the existing design. C.1.1 explicitly decouples leaf placement from buds and places leaves on branches below a user-defined width threshold, allowing density changes without regenerating internodes. C.1.2 starts with phyllotactic orientations and reorients leaves through petiole bending and twisting. It models an inextensible rod with frame H, L, U and target tropism T. The rotation rates are ΩH = −eH L·T, ΩL = eL H·T, and ΩU = eU L·(T×(H×T)). Integrate the rates along petiole segments, multiplying by segment length; the final frame and endpoint position the blade. Figures C.2–C.3 provide visual references. The chapter does not specify a complete sampling/counting rule for independent leaf density; that rule must be documented as our implementation choice.
* **Hart (2020), §10.2, printed pp. 106–110**, instead attaches leaves beneath buds on internodes below a radius threshold. Lateral leaves start in the direction an internode from that bud would grow. Hart also adds leaves at main buds, explicitly acknowledging this as a non-biological fullness adjustment. Its orientation procedure differs from Longay's rod model: three small iterative rotations, with independently adjustable elasticities and typically 10–20 iterations. These methods should not be silently mixed or described as equivalent.
* **Pałubicki et al. (2009), §3**, establishes the node/internode/metamer and axillary-bud relationships used by the core model. It supplies the growth architecture rather than a detailed implementation of Longay's independent foliage placement.
* **Makowski et al. (2019), rendering discussion immediately before §7.1**, uses textured quads for interactive foliage and procedural Houdini foliage for offline renders. This supports cards as a rendering representation, not an alternative leaf attachment algorithm.
* **Smelik et al. (2014)** provides procedural-modeling context; it is not the implementation specification for this feature.

Sources are the supplied files in `docs`, including Hart's text transcription. Longay's placement text, frame illustration, and torque equations were also checked on rendered PDF pages.

## Current integration constraints

- Core leaves are stored in `Tree`, included in snapshots and exposed through an engine-neutral API. Renderers should use these records rather than duplicate the placement algorithm.
- Bark and debug foliage have separate meshes/materials. Production foliage will need textured, lit rendering and a separate petiole representation.
- Width pins are applied to `ShownTree` before foliage is displayed. Keep final foliage driven by that same displayed geometry.
- The preset selector is still a mockup. Library presets and appearance keys exist, but selection and per-species texture overrides need wiring.
- Native and browser builds are supported. Upload entry points differ by platform and should feed a common PNG decoder and storage adapter.
- No growth simulator or project save/load system exists. Foliage does not need to wait for the simulator. Simulated leaf lighting, seasons, wind, fruit, LOD and new growth algorithms remain separate work.

## Accepted baseline

Follow Longay C.1, matching `design.md`: independently spaced leaves on eligible thin branches, species-controlled phyllotaxis and petiole response, then textured leaf cards. Stumps and zero-length internodes are excluded. Radius cutoff semantics, local taper sampling and branch-surface attachment are defined in the implemented contract.

Use distance-based sampling along each main-child axis, carrying residual distance and phyllotactic phase across internodes; lateral axes start their own sequence. This avoids density depending accidentally on how a branch is subdivided. It is an engineering specification beyond the paper, to be recorded as such. Use stable axis/path and sample identities for seeded variation. Changing a texture must never move leaves.

Store validated botanical foliage parameters in the species record. Associate species with an engine-neutral appearance/asset identifier, with actual decoded images and Bevy handles owned by the viewer. Start with one full-image texture per species, transparent background, blade base at bottom center and tip pointing up. Use alpha-masked, double-sided lit cards and a separate petiole representation. Use a generic broadleaf as the default and the only foliage form required for this milestone. The user will supply prepared leaf images. Provide a fallback so the default renders before a custom image is loaded. Keep attachment, organ geometry and appearance separate so future conifer parameters and templates can be added without rewriting broadleaf placement; do not claim that a broadleaf texture on the existing conifer preset models needles. Defaults are illustrative unless calibrated against named species.

## Remaining implementation units

Each row lists only work still needed, with documentation and validation alongside
implementation. Units 1–4 are complete and omitted. Dependencies below refer
to outstanding units; initial placement is already available to all of them.

| Original unit | Depends on | Remaining implementation and documentation | Remaining validation |
| --- | --- | --- | --- |
| 5. Production leaf/petiole geometry | — | Add production blade templates and compact rendering instances; generate petiole geometry from solver output. Reuse the established bottom-center pivot, independent width/length scaling and UV orientation. | Blade base equals the solved petiole endpoint; dimensions/orientation and UVs remain correct after reorientation; valid geometry with no NaNs or degenerate petioles at zero length. |
| 6. Textured viewer rendering | 5 | Add shared lit leaf meshes/materials, bundled fallback textures, alpha masking, correct two-sided normals and shadows. Apply the existing displayed-tree lifecycle to production foliage and release replaced assets. Measure before adding custom GPU instancing. | Native checks from above/below and grazing angles, transparent silhouettes, shadow shape, pin eligibility, repeated rebuild resource use and browser smoke build/run. Record counts and frame timing on a fixed seed. |
| 7. Remaining species/appearance controls | 6 | Wire presets and per-species appearance association, add reset/custom override behavior and expose petiole-response controls once they work. Reuse the existing visibility control for production foliage. Separate foliage/material updates from branch regrowth. Document switching presets and overrides. | Preset switches update geometry and texture together; texture and visibility edits preserve leaf transforms; foliage-only edits avoid branch regrowth and preserve width pins; reset restores expected defaults. |
| 8. Common image ingestion | 6 | Decode prepared PNG bytes into viewer-owned assets with orientation/alpha handling, dimensions/byte limits and clear errors. Preserve the last valid texture on failure. Document accepted input and image preparation. | Transparent PNG, opaque PNG, rejected JPEG, malformed/unsupported/oversized input, aspect ratio behavior, successful replacement and repeated asset cleanup. |
| 9. Native file selection | 7, 8 | Add asynchronous selection, preview, replace and reset for the active species. Keep filesystem details out of core. | Cancel, invalid selection, repeat upload, switch species and reset; responsive UI and previous image preserved on failure. |
| 10. Browser upload | 7, 8 | Add browser file selection using bytes, with no server upload. Preserve per-species association and ignore stale asynchronous results after a species change. Document persistent storage behavior. | Served WASM app: select/cancel/replace/reset, errors, species switch during decoding, transparent edges and aspect ratio; native compilation remains intact. |
| 11. Persistent appearance settings | 7–10 | Store imported image bytes, species association and foliage settings in a versioned local record. Native: application data directory and atomic replacement. Browser: IndexedDB through a storage adapter. Copy bytes rather than retaining a selected-file path. Restore on startup; reset removes the override. Report storage failures while retaining the working in-memory image. | Restart/reload restores images, settings and species association; moving the source file does not break restoration; corrupt records, quota/unavailable storage, failed writes and reset have defined fallback behavior. |
| 12. Final integration and delivery | All remaining units | Extend existing docs with image preparation, final solver/rendering source mapping, supported platforms and measured limits. Add production visual fixtures and end-to-end regressions for rendering, uploads and persistence. | Repeat workspace tests/lints for final code, native launch and Trunk release build; manually verify both upload flows, restart/reload restoration and deterministic seed/pin examples. Existing placement/debug tests remain regression coverage. |

Suggested sequence: **5 → 6**, then finish controls and PNG ingestion (**7–8**),
platform upload flows (**9–10**), persistence (**11**) and final delivery checks
(**12**). PNG ingestion can start once the material contract is settled. Expose
petiole-response widgets when the viewer consumes solved frames, so their effect
is visible rather than silently changing unused parameters.

Calibrated conifer needle arrangements remain future work. Browser persistence
is local to the site/browser profile and can be cleared or evicted; it does not
promise permanent retention or cross-device sync.

## Confirmed scope and decisions

1. **Placement:** Longay C.1, independent of branch/bud placement.
2. **Species:** generic broadleaf default now; conifers later, with extensible parameters and geometry.
3. **Images:** user-supplied cropped images with transparency; later expose the same prepared-image workflow to users. No automatic segmentation or cropping is required.
4. **Persistence:** preserve custom textures and their species association across restarts where supported. Persist foliage settings too so restored textures retain their intended size and placement; full project/save-history persistence remains outside this feature.

**Image format confirmed:** cropped PNG files with alpha transparency. Use the same format for supplied assets and future user imports. The initial workflow uses one blade per image, base at bottom center and tip pointing up; show this convention in the upload guidance. No JPEG support, separate alpha masks, background removal or crop editor is required.

All four planning questions are resolved. Sampling choices are implemented and documented; the proposed native/browser storage mechanisms remain to be implemented and validated.

Assume both native and browser support because both are already shipped. No atlas or multi-leaf image support is required. No performance budget is supplied: measure the existing seven-step tree first and report results before deciding whether custom GPU instancing is necessary.
