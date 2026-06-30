# jellyflow-open-gpui

First-class Open GPUI adapter boundary for Jellyflow.

The crate maps Jellyflow's headless semantic node contracts to Open GPUI local
components and measurement facts. Runtime and core crates stay toolkit-free;
this crate owns GPUI-specific widgets, focus, menus, inspector plans, product
fixture gates, and layout reporting.

Current capability reporting is deliberately conservative. Projection fallback
can prove controls, repeatables, menus, inspector state, and product fixture
geometry, but full layout-pass measurement is only valid when bounds come from
the Open GPUI element-bounds hook.
