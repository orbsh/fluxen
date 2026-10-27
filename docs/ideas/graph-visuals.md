# Graph visuals — what this view is eventually for

Collected because the current renderer must not close doors these ideas need
open. Nothing here is scheduled; it is the target the foundation is judged
against.

## Light

- **Neon breathing**: nodes and edges pulse — intensity, not size — on a slow
  shared cycle, so the graph reads as alive rather than twinkling at random.
- **Node halo**: a soft glow around a node, brighter on focus or on a live
  event, so attention can be drawn without moving the camera.
- **Edge flow**: something travels along an edge — a gradient, dashes, or a
  particle — in the direction of the relationship, at a speed that can carry
  meaning (traffic, freshness, a run in progress).

All three need the geometry to be *world-space* and to carry a parameter along
its length. A glow also needs a post-processing pass: the scene is rendered into
a texture, bright areas are blurred and added back. None of this is possible on
edges drawn as 2D screen-space quads.

## Space

- **Nodes that scale up into places**: a node becomes a small planet — PBR
  material, image-based lighting, a surface you could circle — when it is
  entered or focused.
- **The graph inside a scene**: an environment behind it (skybox, fog, ground),
  so "a graph" becomes "a place": camera parallax, depth cues, a horizon.
- **Other elements in the same space**: things that are neither nodes nor edges —
  labels that hold their size in 3D, markers, annotations, zones, arrows,
  particles. The view must accept them without becoming a second renderer.

## The site, eventually

- **The whole site in 3D**: pages and panels as objects in a scene rather than
  stacked boxes.
- **Dialogs pinned to one side**: a conversation stays where the user put it
  while the camera moves; it is an object with a fixed place, not an overlay.
- **Other UI embedded in the graph**: an input, a chart or a table living at a
  position in the scene, at a readable angle.

This is a much larger decision than the graph component — it changes how the
whole application is laid out — but the graph component is where it starts, and
the graph component must not be the thing that blocks it.

## Form

- **A tree, not a web**: one trunk edge — the spine of the structure — and every
  other edge as a vine, curved and slightly irregular, wrapping the trunk.
- Leaves at the ends of vines, so the shape reads as organic growth rather than
  as a diagram.

This is a data-shape problem as much as a visual one: an edge needs a role
(trunk, vine, connector) and a curve, not just two endpoints.

## What this implies for the foundation

Recorded here because the earlier renderer got it wrong:

- Edges were drawn as quads expanded in *clip space*, i.e. as 2D pixels on top
  of a 3D scene. That was the cheapest way to get a visible 2.5px line, and it
  is a dead end for every idea above — flow needs a length parameter, glow needs
  depth and a post pass, vines need a curve in the scene, and a trunk needs a
  width in the world.
- The fix is the standard one: an edge is world-space geometry — a camera-facing
  quad (or a tube) whose *width* is computed in the vertex shader from the
  distance to the camera, so the on-screen width stays constant while the
  geometry stays in the scene. That single change makes light, space and form
  reachable without redesigning again.
- A middle layer earns its place for exactly these features: a scene graph, a
  material system and render targets are what "glow", "a planet" and "the graph
  in a place" are made of. Hand-rolling them is a renderer project, not a
  component.