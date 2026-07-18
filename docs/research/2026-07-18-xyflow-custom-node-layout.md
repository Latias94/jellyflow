# xyflow Custom Node Layout and Handle Geometry

Status: source research and Jellyflow mapping

Date: 2026-07-18

Scope: how React Flow and Svelte Flow place custom node content and handles, how they keep edge
geometry current, and what their design implies for Jellyflow's Open GPUI node host.

## Source Baseline

This note uses only the first-party source, source documentation, and examples in the local
`repo-ref/xyflow` mirror. The mirror is pinned to:

- repository: `https://github.com/xyflow/xyflow`;
- commit: `dd308ab401d49518f73d1e91c43faf254ff5a4c9` (2026-07-06);
- tags: `@xyflow/react@12.11.2`, `@xyflow/svelte@1.6.2`, and
  `@xyflow/system@0.0.79`.

## Executive Finding

xyflow does **not** run a collision-avoidance algorithm between a port label and custom node
content. It does not render a port label at all. A custom node owns its complete DOM subtree and
interior layout. A `<Handle>` is an absolutely positioned DOM element, removed from normal content
flow, and the default stylesheet places it at the midpoint of a requested node side. Avoiding
overlap is therefore the custom node author's responsibility: reserve padding or a port gutter,
put related content in a deliberate row layout, and explicitly position multiple handles.

xyflow's generic responsibility begins after layout. It measures the resulting node and handle
DOM rectangles, caches handle geometry relative to the node, and routes edges from those measured
positions. `ResizeObserver` refreshes geometry when the node's dimensions change, while
`useUpdateNodeInternals` is the explicit invalidation path when handle count or position changes
without changing the outer node size.

For Jellyflow, the corresponding boundary is already mostly present: Open GPUI native renderers
own arbitrary content, `OpenGpuiPortHandle` measures the actual native marker, and the runtime
consumes node bounds plus port visual/hit/route geometry. The workflow example violates the
layout half of that boundary: it paints the fixed input marker **and its label** in an absolute
overlay independent of the branch content rows. The screenshot overlap is therefore a product
renderer composition bug, not evidence that the graph core needs a generic label-avoidance DSL.

## Verified xyflow Behavior

### 1. NodeWrapper is a positioned host, not an interior layout engine

React's `NodeWrapper` resolves the adapter-owned `nodeTypes` component, observes the node, and
applies the absolute canvas translation and optional dimensions to its outer `<div>`. It then
renders the custom component directly as its child and passes graph facts as props. It does not
insert port labels, content slots, gutters, or collision logic.

Sources:

- [React `NodeWrapper`, component resolution and observation, lines 44-80](../../repo-ref/xyflow/packages/react/src/components/NodeWrapper/index.tsx#L44-L80)
- [React `NodeWrapper`, positioned shell and custom component, lines 184-249](../../repo-ref/xyflow/packages/react/src/components/NodeWrapper/index.tsx#L184-L249)
- [Svelte `NodeWrapper`, positioned shell and custom component, lines 238-321](../../repo-ref/xyflow/packages/svelte/src/lib/components/NodeWrapper/NodeWrapper.svelte#L238-L321)

The shared CSS makes each node wrapper `position: absolute` and gives it normal box sizing. The
custom component remains responsible for every layout decision inside that box.

Source: [node wrapper base CSS, lines 194-219](../../repo-ref/xyflow/packages/system/src/styles/init.css#L194-L219)

### 2. Handle is an absolute overlay and does not reserve content space

React and Svelte Handle components emit a plain element with semantic data attributes and side
classes. Their only children are whatever the custom component supplies. Neither implementation
contains a label or layout wrapper.

Sources:

- [React `Handle` element, lines 239-279](../../repo-ref/xyflow/packages/react/src/components/Handle/index.tsx#L239-L279)
- [Svelte `Handle` element, lines 195-234](../../repo-ref/xyflow/packages/svelte/src/lib/components/Handle/Handle.svelte#L195-L234)

The base CSS sets handles to `position: absolute`. Left and right handles default to `top: 50%`
and are translated half their own size across the node boundary; top and bottom handles use the
same rule on the other axis. Because absolute elements do not participate in normal layout, this
cannot push text or controls away from the node edge.

Source: [Handle positioning CSS, lines 233-272](../../repo-ref/xyflow/packages/system/src/styles/init.css#L233-L272)

The complete theme only supplies the marker's visual size and appearance (6 by 6 pixels in the
default theme). It does not add a content inset based on that size.

Source: [default Handle theme, lines 112-118](../../repo-ref/xyflow/packages/system/src/styles/style.css#L112-L118)

### 3. Default spacing is just CSS; complex placement is authored explicitly

Built-in nodes render their label and one or two handles as siblings. Their apparent separation
comes from the built-in node theme: a 150-pixel width, 10-pixel padding, and centered text. This is
not a generic port-label layout facility.

Sources:

- [React built-in default node, lines 6-18](../../repo-ref/xyflow/packages/react/src/components/Nodes/DefaultNode.tsx#L6-L18)
- [built-in node theme, lines 71-95](../../repo-ref/xyflow/packages/system/src/styles/style.css#L71-L95)

The first-party React custom-node example supplies `padding: 10` on the custom node and explicitly
places two right handles using `top: 10` and `bottom: 10`. The Svelte example does the same with
component-local CSS and inline handle positions. This is the concrete authoring model used when
the default side midpoint is insufficient.

Sources:

- [React custom handle offsets, lines 6-12 and 27-45](../../repo-ref/xyflow/examples/react/src/examples/CustomNode/ColorSelectorNode.tsx#L6-L45)
- [React custom node padding, lines 88-102](../../repo-ref/xyflow/examples/react/src/examples/CustomNode/index.tsx#L88-L102)
- [Svelte custom node layout, lines 10-31](../../repo-ref/xyflow/examples/svelte/src/routes/examples/customnode/CustomNode.svelte#L10-L31)

Handle children can themselves be detached or enlarged with custom CSS. The first-party detached
handle example places a button one rem beyond the handle using absolute positioning, reinforcing
that the product component controls visual composition.

Sources:

- [React detached Handle child, lines 32-41](../../repo-ref/xyflow/examples/react/src/examples/DetachedHandle/index.tsx#L32-L41)
- [React detached Handle CSS, lines 1-10](../../repo-ref/xyflow/examples/react/src/examples/DetachedHandle/style.css#L1-L10)

### 4. Measurement records the final layout; it does not create that layout

React and Svelte attach a `ResizeObserver` to node wrappers. Each observer reports the changed node
element to the shared `updateNodeInternals` function.

Sources:

- [React resize observer, lines 9-38](../../repo-ref/xyflow/packages/react/src/container/NodeRenderer/useResizeObserver.ts#L9-L38)
- [Svelte resize observer, lines 25-47](../../repo-ref/xyflow/packages/svelte/src/lib/container/NodeRenderer/NodeRenderer.svelte#L25-L47)

The shared function reads the node's `offsetWidth`/`offsetHeight`, then measures every `.source`
and `.target` descendant with `getBoundingClientRect`. Handle rectangles are normalized by zoom
and stored relative to the node. The resulting node dimensions and handle bounds are cached in the
internal node. No measured rectangle is fed back into DOM layout to avoid content.

Sources:

- [DOM node and Handle measurement, lines 30-33 and 63-93](../../repo-ref/xyflow/packages/system/src/utils/dom.ts#L30-L93)
- [`updateNodeInternals`, lines 401-499](../../repo-ref/xyflow/packages/system/src/utils/store.ts#L401-L499)
- [internal measured dimensions and `handleBounds`, lines 90-106](../../repo-ref/xyflow/packages/system/src/types/nodes.ts#L90-L106)

Edges are withheld until node dimensions and handle geometry exist. Once initialized, their
endpoints come from cached handle bounds and the handle's declared outward side.

Sources:

- [edge initialization and Handle lookup, lines 19-62](../../repo-ref/xyflow/packages/system/src/utils/edges/positions.ts#L19-L62)
- [endpoint calculation from measured Handle bounds, lines 99-124](../../repo-ref/xyflow/packages/system/src/utils/edges/positions.ts#L99-L124)
- [`useNodesInitialized` geometry gate, lines 11-26](../../repo-ref/xyflow/packages/react/src/hooks/useNodesInitialized.ts#L11-L26)

### 5. Dynamic handles require explicit geometry invalidation

A node resize changes the outer element and naturally triggers `ResizeObserver`. Moving, adding,
or removing a Handle can leave the node's outer size unchanged, so React and Svelte expose
`useUpdateNodeInternals`. Both hooks find the node DOM element and schedule a forced internal
update on the next animation frame. Their source documentation explicitly requires this after
programmatic handle count or position changes.

Sources:

- [React `useUpdateNodeInternals` contract and implementation, lines 6-14 and 51-68](../../repo-ref/xyflow/packages/react/src/hooks/useUpdateNodeInternals.ts#L6-L68)
- [Svelte `useUpdateNodeInternals` contract and implementation, lines 5-40](../../repo-ref/xyflow/packages/svelte/src/lib/hooks/useUpdateNodeInternals.svelte.ts#L5-L40)
- [Svelte dynamic-handle example, lines 1-20](../../repo-ref/xyflow/examples/svelte/src/routes/examples/useupdatenodeinternals/CustomNode.svelte#L1-L20)

The wrappers also force a refresh when node type or default source/target side changes, because
those changes can move handles without resizing the node.

Sources:

- [React wrapper invalidation, lines 50-70](../../repo-ref/xyflow/packages/react/src/components/NodeWrapper/useNodeObserver.ts#L50-L70)
- [Svelte wrapper invalidation, lines 126-156](../../repo-ref/xyflow/packages/svelte/src/lib/components/NodeWrapper/NodeWrapper.svelte#L126-L156)

### 6. NodeResizer and NodeOrigin solve different problems

`NodeResizer` adds absolutely positioned resize controls and emits node dimension and position
changes. Its own example still adds padding around content and renders handles separately. It
does not inspect content or solve label/handle collisions.

Sources:

- [`NodeResizer` source documentation and composition, lines 6-28 and 53-97](../../repo-ref/xyflow/packages/react/src/additional-components/NodeResizer/NodeResizer.tsx#L6-L97)
- [resize changes emitted to the node store, lines 85-173](../../repo-ref/xyflow/packages/react/src/additional-components/NodeResizer/NodeResizeControl.tsx#L85-L173)
- [absolute resizer controls, lines 5-66](../../repo-ref/xyflow/packages/system/src/styles/node-resizer.css#L5-L66)

`NodeOrigin` only defines which point in a node's measured rectangle corresponds to the graph
position. `[0, 0]` means top-left and `[0.5, 0.5]` means center. The position calculation subtracts
an origin-weighted width and height; it does not change the node's interior coordinate system or
Handle/content relationship.

Sources:

- [`NodeOrigin` definition, lines 157-164](../../repo-ref/xyflow/packages/system/src/types/nodes.ts#L157-L164)
- [origin-to-position calculation, lines 136-145](../../repo-ref/xyflow/packages/system/src/utils/graph.ts#L136-L145)

## Mapping to Jellyflow's Open GPUI Host

### Existing contract that matches xyflow

Jellyflow already separates native composition from graph geometry in the right direction:

- `OpenGpuiNodeBinding::port_handle` exposes a semantic handle for a real graph port, without
  prescribing a widget tree;
- `OpenGpuiPortHandle::interactive` wraps an ordinary native marker and owns only measurement,
  interaction, accessibility, and typed port events;
- post-layout prepaint records the marker bounds, and the adapter derives separate visual bounds,
  expanded hit bounds, a side midpoint route anchor, and outward direction;
- the runtime layout builder requires one outcome per real port and otherwise remains unaware of
  labels or native controls.

Sources:

- [native binding port handles, lines 328-387](../../crates/jellyflow-open-gpui/src/binding.rs#L328-L387)
- [native port wrapper contract, lines 171-258](../../crates/jellyflow-open-gpui/src/port_handle.rs#L171-L258)
- [post-layout measurement, lines 304-364](../../crates/jellyflow-open-gpui/src/port_handle.rs#L304-L364)
- [visual/hit/route geometry derivation, lines 591-638](../../crates/jellyflow-open-gpui/src/port_handle.rs#L591-L638)
- [complete port layout contract, lines 139-221](../../crates/jellyflow-runtime/src/runtime/adapter_protocol/feedback.rs#L139-L221)

### Why the pre-fix workflow node overlapped

At baseline commit `610470f`, the workflow helper rendered each fixed port as an independent
absolute row. For an input, it put the marker at `left: -6`, then appended the port key as a label,
with a hard-coded first `top: 60`. The condition node separately rendered its first branch row
inside body padding immediately below the 44-pixel header. The two trees had no shared row or
reserved gutter, so `input` and `Intent matches` occupied the same pixels. This is exactly the kind
of overlap xyflow leaves to the custom node component rather than repairing in its core.

Sources:

- baseline source: `git show 610470f:examples/canvas-jellyflow/src/nodes/workflow/mod.rs`
- baseline source: `git show 610470f:examples/canvas-jellyflow/src/nodes/workflow/condition.rs`

### Recommended layout contract

Keep the graph core and `OpenGpuiPortHandle` free of labels and automatic collision solving. Make
these rules explicit for native node authors and shared product chrome:

1. **The native renderer owns port labels and content spacing.** A semantic port exposes identity,
   direction, and presentation hints; it does not create a second host-painted label layer.
2. **Measure only the actual marker.** The label may sit beside it, but the marker child passed to
   `OpenGpuiPortHandle` remains the visual/hit/route source.
3. **Use one layout tree for related pixels.** Put marker, label, and row content in the same flex
   row, or reserve a fixed port gutter before laying out the body. Do not combine a labeled
   absolute port overlay with body content that does not reserve its width.
4. **Invalidate geometry after topology or placement changes.** A port move that leaves node size
   unchanged needs `OpenGpuiNodeBinding::invalidate_layout`, analogous to xyflow's
   `useUpdateNodeInternals`.
5. **Keep canvas origin separate from interior layout.** Node origin affects graph placement only;
   it must not be used as a workaround for node-content overlap.

For a workflow execution port, the product may deliberately render a marker without visible text
when the header or branch row already communicates the meaning. The catalog label remains useful
for menus and accessibility, but it must not become an independently positioned second label.

### Applied Open GPUI mapping

The current branch applies the boundary above without changing the graph core or adding a DSL:

- fixed execution ports are children of the shared workflow header, so their vertical position
  follows header layout instead of a page-level `top` constant;
- the fixed-port helper measures and paints only the marker; it no longer clones or renders the
  schema key as visible text;
- condition output markers remain owned by their branch rows, while the scrolling layer spans the
  node width so markers are not clipped outside a padded scroll content box;
- each branch marker's outer edge coincides with the node's right boundary, keeping both the marker
  and the route endpoint visible;
- an Open GPUI integration test compares real published marker and node geometry and keeps the
  header input vertically clear of the first branch row.

Sources:

- [header-owned fixed markers](../../examples/canvas-jellyflow/src/nodes/workflow/mod.rs)
- [row-owned condition markers](../../examples/canvas-jellyflow/src/nodes/workflow/condition.rs)
- [native geometry regression](../../examples/canvas-jellyflow/tests/native_workflow_host.rs)

## Conclusion

The transferable xyflow mechanism is **native layout first, measured graph geometry second**.
DOM makes custom UI convenient because ordinary CSS solves the interior layout, not because
React Flow has a port-label collision system. Jellyflow should preserve the same ownership split
for Open GPUI: native components compose and space their own marker/label/control rows; the
headless runtime consumes only the measured node and port geometry needed for graph behavior.
