---
type: "Memory Event"
title: "Progress: Open GPUI canvas-jellyflow scene host now prefers prepared CanvasSceneFrame reco"
description: "Open GPUI canvas-jellyflow scene host now prefers prepared CanvasSceneFrame records and treats document-only ordering as initial bootstrap f"
timestamp: 2026-07-05T12:37:26Z
event_kind: "Progress"
---
# Event

Open GPUI canvas-jellyflow scene host now prefers prepared CanvasSceneFrame records and treats document-only ordering as initial bootstrap fallback.

# Impact

This demotes the old global overlay/product-surface ordering shape further.
Future product-node rendering should consume prepared scene facts or an explicitly named scene recomputation path, not an anonymous document-only z sort.

# Citations

- [Progress](../../progress/2026-07-05-open-gpui-scene-host-product-path-progress.md)
- [Verification](../../verification/2026-07-05-open-gpui-scene-host-cleanup-verification.md)
