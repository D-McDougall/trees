# Measurement conventions

- Store physical lengths in metres throughout `trees-core`, `trees-mesh`, and
  viewer state. One world coordinate unit represents one metre. This includes
  positions, internode lengths, branch radii and width pins, foliage dimensions
  and spacing, sampling-cell width, occupancy radius, and perception distance.
- Display and edit physical lengths in **feet** in the user interface. Label
  controls and readouts with `ft`, including branch menus, pin lists, warnings,
  and tooltips. Use exactly **1 ft = 0.3048 m**; use the shared conversion helpers
  in `crates/trees-viewer/src/editor.rs` at the UI boundary. Convert edits back
  to metres, preserve validation bounds, and do not write back conversions on
  unchanged frames (rounding must not trigger regrowth).
- Do not rescale stored parameters, presets, geometry, or simulation constants
  when changing display units. The default 0.1016 m leaf dimension is 1/3 ft
  (four inches).
- Store angles in radians and display/edit them in degrees (`°`). The
  perception angle is the cone's **half-angle**.
- Counts remain counts: maximum shoot length is measured in internodes, shadow
  depth in cells, and preview duration in growth steps. Seeds are integers.
- Fractions, jitter (a fraction of cell width), direction weights, pipe
  exponents, and other model coefficients are dimensionless; do not convert
  them to feet. Gravimorphism ellipse parameters describe relative bud
  propensity, not world-space geometry.
- UI layout and pointer distances use logical pixels; viewport dimensions use
  physical pixels where required by Bevy. These are not world lengths.
