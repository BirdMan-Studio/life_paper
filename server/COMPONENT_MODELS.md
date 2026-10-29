# Component Models

The existing `GET /organism-components` and unlocked-component endpoints include
a `model` on every definition. Dimensions are map pixels, not CSS pixels. The
assembler currently displays two CSS pixels per map pixel before zoom.

Register a component in `src/domain/organism_component/builtin.rs`, then use
`registry.set_model(id, model)` to customize its category defaults. Registration
rejects incompatible geometry, assembly, collision, and size combinations.
Existing primitives require no client component-ID branches:

| Category | Geometry | Assembly | Collision |
| --- | --- | --- | --- |
| Body | `capsule` | `free` | `geometry` |
| External organ | `surface` | `surface` | `none` |
| Wrapping organ | `host_outline` | `wrap` | `none` |
| Internal organ icon | `ellipse`, `rectangle` | `interior` | `none` |

- `size`: default `width_px` and `length_px`; external width is always zero.
- `size_bounds`: optional `min`/`max` sizes reserved for resizing and growth.
  `null` means fixed size. No growth controls or simulation are implemented yet.
- `appearance`: `fill`/`stroke` (`#RRGGBB` or `none`), `stroke_width_px`,
  `protrusion_px`, and optional `range_radius_px` for a dashed range circle.
  Stroke and protrusion are visual only and do not create collision thickness.

Saving a model resolves omitted legacy sizes, validates permitted sizes and
connected body hosts, rejects invalid anchors, computes wrapping length from
host perimeter, and derives surface-organ position from its host and anchor.
Internal organs retain only their connected body host; their `position` is
omitted from persisted model data because they do not have player-configurable
coordinates.
Client payloads cannot override the catalog's collision or appearance rules.
Body placement remains player-controlled. Existing saved models are not migrated.

The client implements these renderer primitives, not per-component definitions.
A new primitive or assembly algorithm still requires a coordinated client/server
update. Collision declarations do not implement a runtime physics solver.
