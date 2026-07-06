---
title: "refactor: Open GPUI example boundary"
date: 2026-07-07
type: refactor
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
product_contract_source: ce-plan-bootstrap
execution: code
related_plans:
  - docs/plans/2026-06-30-002-feat-open-gpui-mature-adapter-plan.md
  - docs/plans/2026-07-04-001-feat-open-gpui-adapter-productization-plan.md
  - docs/plans/2026-07-05-002-refactor-open-gpui-atomic-node-scene-plan.md
related_decisions:
  - docs/knowledge/engineering/decisions/semantic-surface-and-framework-adapters.md
  - docs/knowledge/engineering/decisions/node-ui-kit-component-contract.md
  - docs/knowledge/engineering/decisions/open-gpui-node-component-kit.md
  - docs/knowledge/engineering/decisions/open-gpui-atomic-scene-boundary.md
---

# Open GPUI Example Boundary Migration - Plan

## Goal Capsule

| Field | Decision |
| --- | --- |
| Objective | Move the concrete Jellyflow Open GPUI product gallery out of the `repo-ref/open-gpui` release workspace and into the Jellyflow repository, while preserving the mature adapter proof, product node UI gates, and Open GPUI generic canvas fixes. |
| User-visible problem | Open GPUI is preparing for release, but its workspace still contains `examples/canvas-jellyflow`, a Jellyflow-specific example with reverse path dependencies back into the Jellyflow root. This couples Open GPUI releases to Jellyflow local state and makes the demo ownership confusing. |
| Architecture stance | Jellyflow owns the adapter/product gallery; Open GPUI owns reusable framework, component library, and canvas primitives. `jellyflow-open-gpui` remains widget-free. Concrete Open GPUI node components remain host-local in the example. |
| Execution profile | Fearless cross-repo cleanup. Breaking internal example paths, deleting the old Open GPUI example directory, and updating docs/tests is allowed. Do not move concrete Open GPUI widgets into runtime or into `jellyflow-open-gpui`. |
| Stop conditions | Stop if the implementation requires unpublished Open GPUI APIs that are not available from a pushed git revision or local checkout, changes Open GPUI public release APIs unnecessarily, adds Open GPUI dependencies to Jellyflow core/runtime, or makes Jellyflow's default workspace depend on a local `repo-ref/open-gpui` checkout. |
| Tail ownership | Execute with `ce-work` goal mode. Track progress in commits, verification output, and engineering memory; do not mutate this plan as a progress log. |

## Product Contract

### Summary

The current `repo-ref/open-gpui/examples/canvas-jellyflow` package is no longer just an Open GPUI sample.
It is the live Jellyflow Open GPUI host fixture: it renders Dify-style workflow cards, shader/blueprint cards, ERD rows, mind-map/source cards, measured internals, product routes, port handles, reconnect, resize, authoring controls, and native smoke tests.

That ownership belongs in Jellyflow.
Open GPUI should ship a clean framework workspace with generic canvas examples such as `canvas-notes`, not a product-specific example that requires a sibling Jellyflow checkout.
Jellyflow should expose the runnable GPUI product gallery from its own repository because it is the adapter proof users will inspect when deciding whether Jellyflow can support Dify, Unreal Blueprint, Unity Shader Graph, ERD, and mind-map style products.

### Requirements

**Repository boundary**

- R1. Remove `repo-ref/open-gpui/examples/canvas-jellyflow` from the Open GPUI workspace and delete the copied source from the Open GPUI repo after migration.
- R2. Keep Open GPUI generic canvas/framework fixes that are not Jellyflow-specific, including prepared scene refresh after paint callbacks, generic canvas scene APIs, route policies, hit testing, and native close behavior tests.
- R3. Ensure a standalone Open GPUI checkout can run its release-oriented checks without a Jellyflow checkout.
- R4. Keep Jellyflow root's default workspace gates free from mandatory local Open GPUI path dependencies until the required `open-gpui-*` crates are published and usable from crates.io.
- R5. Make the Jellyflow repository the authoritative location for the Open GPUI Jellyflow product gallery and its run/test documentation.

**Adapter and component ownership**

- R6. Keep `jellyflow-runtime`, `jellyflow-core`, and `jellyflow-layout` toolkit-free.
- R7. Keep `jellyflow-open-gpui` widget-free; it may own renderer plans, ids, authoring plans, measurement conversion, evidence reports, and test gates, but not concrete Open GPUI widgets.
- R8. Keep concrete GPUI `Button`, `Menu`, `TextInput`, `Textarea`, `Select`, `Switch`, `Slider`, measured element wrappers, focus/popup lifecycle, and product renderer layout in the migrated example.
- R9. Preserve the host-local node component kit as a reference consumer, not as a shared cross-framework widget crate.
- R10. Preserve the semantic component contract for fields, controls, repeatables, menus, inspector, anchors, and measured regions.

**Product proof and UX gates**

- R11. The migrated example must keep Dify/workflow, shader/blueprint, ERD, mind-map, and source fixtures runnable.
- R12. Existing regression gates for no-pointer readiness, atomic node scene behavior, measured internals, port/reconnect budgets, readable regions, drag exclusions, and product route previews must still run from the new path.
- R13. The known screenshot-exporter flake/hang may remain excluded from hard nextest gates, but screenshot output paths and skip messages must point to the new Jellyflow-owned example.
- R14. Documentation must stop instructing users to run Jellyflow code from `repo-ref/open-gpui/examples/canvas-jellyflow`.
- R15. Current Open GPUI crates.io availability must be checked before switching the example to registry dependencies. Until `open-gpui-canvas` and `open-gpui-ui-components` are published at the needed version, use a pinned Open GPUI git revision; do not use `repo-ref/open-gpui` path dependencies from this example while it also path-depends on Jellyflow workspace crates.

### Acceptance Examples

- AE1. Given a fresh Open GPUI repo checkout, when `cargo metadata` or a targeted Open GPUI release check runs, then it does not require `../../../../crates/jellyflow` or `jellyflow-open-gpui`.
- AE2. Given the Jellyflow repo with `repo-ref/open-gpui` present, when `cargo run --manifest-path examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow` runs, then the existing product gallery starts from the Jellyflow-owned path.
- AE3. Given the migrated example, when the focused product tests run, then Dify, shader, ERD, mind-map, source, authoring, measurement, and scene readiness tests still target the same product behavior as before migration.
- AE4. Given Jellyflow root, when `cargo nextest run -p jellyflow-open-gpui` runs, then it does not compile concrete Open GPUI widget crates.
- AE5. Given a user reads `README.md`, `docs/examples/README.md`, `docs/testing/node-ui-authoring-regression.md`, or `crates/jellyflow-open-gpui/README.md`, then the run commands and ownership language point to `examples/canvas-jellyflow`.
- AE6. Given Open GPUI 0.2 crates become available on crates.io, when the dependency cleanup slice is executed, then the example can switch from local `repo-ref/open-gpui` path dependencies to versioned dependencies without changing Jellyflow adapter contracts.

### Scope Boundaries

In scope:

- Copy or move `repo-ref/open-gpui/examples/canvas-jellyflow` into `examples/canvas-jellyflow`.
- Convert the migrated example into an independent example workspace, not a required Jellyflow root workspace member.
- Update manifest paths, relative dependencies, screenshot/output directories, run commands, and verification commands.
- Remove the old example from `repo-ref/open-gpui` workspace membership and README.
- Preserve or relocate all product-gallery tests and adapter evidence gates.
- Update engineering memory after the migration lands.

Deferred:

- Publishing `jellyflow-open-gpui`.
- Switching to crates.io `open-gpui-*` dependencies before the required 0.2 packages exist.
- Extracting a reusable `jellyflow-open-gpui-host` crate.
- Creating mature egui or Dioxus concrete adapters.
- Golden pixel infrastructure beyond existing screenshot/native smoke probes.

Out of scope:

- Shared cross-framework widget crates.
- Runtime-owned Open GPUI widgets.
- DOM/CSS z-index concepts as the model.
- Backend Dify execution, shader compilation, database persistence, sync, or multiplayer.

## Planning Contract

### Key Technical Decisions

- KTD1. The migrated package should live at `examples/canvas-jellyflow` and keep package/bin name `open-gpui-canvas-jellyflow` to minimize churn in tests and history.
- KTD2. The example should declare its own nested `[workspace]` initially. Do not add it to root `members` while it depends on local `repo-ref/open-gpui` path crates.
- KTD3. Open GPUI dependencies should be pinned git dependencies until registry availability is verified. Local `repo-ref/open-gpui` path dependencies conflict with Cargo workspace inheritance when mixed with local Jellyflow path dependencies from this example. Do not add Open GPUI dependencies to Jellyflow root `[workspace.dependencies]` yet.
- KTD4. The Open GPUI release workspace must not carry product-specific reverse dependencies. Remove the member and directory from `repo-ref/open-gpui`, but keep generic canvas framework fixes and tests.
- KTD5. `jellyflow-open-gpui` remains the reusable adapter contract crate. Concrete Open GPUI components remain in the host example.
- KTD6. Verification must prefer structured reports and nextest gates over screenshots. The known screenshot exporter can stay excluded from hard gates with an explicit note.
- KTD7. Any stale path or ownership language is a correctness bug because this project uses docs as architectural memory for future agents.
- KTD8. If implementation discovers duplicate migration-only glue or dead compatibility shims, delete them rather than preserving old paths.

### Target Output Structure

```text
examples/canvas-jellyflow/
  Cargo.toml
  README.md
  src/
    gallery_screenshot.rs
    main.rs
    measurement_bridge.rs
    native_smoke.rs
    node_component_kit.rs
    product_gallery.rs
    product_renderers.rs
    visual_regression.rs
```

The example manifest should be usable with:

```sh
cargo run --manifest-path examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow
```

### Dependency Shape

Near-term local development:

```toml
[workspace]

[package]
name = "open-gpui-canvas-jellyflow"
publish = false

[dependencies]
jellyflow = { path = "../../crates/jellyflow" }
jellyflow-open-gpui = { path = "../../crates/jellyflow-open-gpui" }
open_gpui = { package = "open-gpui", git = "https://github.com/Latias94/open-gpui", rev = "<pushed-open-gpui-rev>", default-features = false }
open_gpui_canvas = { package = "open-gpui-canvas", git = "https://github.com/Latias94/open-gpui", rev = "<pushed-open-gpui-rev>" }
open_gpui_platform = { package = "open-gpui-platform", git = "https://github.com/Latias94/open-gpui", rev = "<pushed-open-gpui-rev>", default-features = false, features = ["font-kit"] }
open_gpui_ui_components = { package = "open-gpui-ui-components", git = "https://github.com/Latias94/open-gpui", rev = "<pushed-open-gpui-rev>" }
open_gpui_ui_core = { package = "open-gpui-ui-core", git = "https://github.com/Latias94/open-gpui", rev = "<pushed-open-gpui-rev>" }
```

Later registry mode after release:

```toml
open_gpui = { package = "open-gpui", version = "0.2", default-features = false }
open_gpui_canvas = { package = "open-gpui-canvas", version = "0.2" }
open_gpui_platform = { package = "open-gpui-platform", version = "0.2", default-features = false, features = ["font-kit"] }
open_gpui_ui_components = { package = "open-gpui-ui-components", version = "0.2" }
open_gpui_ui_core = { package = "open-gpui-ui-core", version = "0.2" }
```

### Implementation Units

#### U1. Migrate the Jellyflow Open GPUI product gallery into Jellyflow

**Goal:** Create `examples/canvas-jellyflow` as the Jellyflow-owned host fixture.

**Requirements:** R5, R8, R9, R11, R15, AE2, AE3.

**Files:**

- Create `examples/canvas-jellyflow/Cargo.toml`
- Create `examples/canvas-jellyflow/README.md`
- Move/copy `repo-ref/open-gpui/examples/canvas-jellyflow/src/**` to `examples/canvas-jellyflow/src/**`

**Approach:**

- Keep package/bin name `open-gpui-canvas-jellyflow`.
- Add a nested `[workspace]` so the example is not pulled into Jellyflow root workspace gates.
- Adjust Jellyflow path dependencies from `../../../../crates/...` to `../../crates/...`.
- Adjust Open GPUI dependencies from workspace dependencies to a pinned Open GPUI git revision.
- Move screenshot/native smoke output directories from `repo-ref/open-gpui/target/...` to `target/open-gpui-jellyflow-gallery/...` or an example-local equivalent.

**Test scenarios:**

- `cargo metadata --manifest-path examples/canvas-jellyflow/Cargo.toml --no-deps` resolves the package.
- `RUSTFLAGS='-Awarnings' cargo check --manifest-path examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow` compiles.

#### U2. Preserve product behavior and test gates under the new path

**Goal:** Make existing product-gallery tests pass from the Jellyflow-owned example.

**Requirements:** R11, R12, R13, AE3.

**Files:**

- `examples/canvas-jellyflow/src/main.rs`
- `examples/canvas-jellyflow/src/product_gallery.rs`
- `examples/canvas-jellyflow/src/product_renderers.rs`
- `examples/canvas-jellyflow/src/visual_regression.rs`
- `examples/canvas-jellyflow/src/native_smoke.rs`
- `examples/canvas-jellyflow/src/gallery_screenshot.rs`

**Approach:**

- Keep fixture names, renderer keys, and test names stable where possible.
- Fix only migration-caused path/build issues first.
- Preserve the hard exclusion for the known screenshot exporter if it still hangs.
- Do not use this unit to redesign node UI layout or component APIs unless migration breaks them.

**Test scenarios:**

- `RUSTFLAGS='-Awarnings' cargo nextest run --manifest-path examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow --no-fail-fast --status-level fail --final-status-level fail -E 'not test(gallery_screenshot::product_gallery_screenshot_exporter_writes_nonblank_pngs_or_skips)'`
- Targeted smoke if full nextest is too slow: renderer registry, mind-map readiness, product renderer, measurement bridge, and native smoke tests from the new manifest.

#### U3. Keep `jellyflow-open-gpui` widget-free and gate it independently

**Goal:** Confirm the adapter contract crate remains reusable and does not absorb host widgets during migration.

**Requirements:** R6, R7, R10, AE4.

**Files:**

- `crates/jellyflow-open-gpui/Cargo.toml`
- `crates/jellyflow-open-gpui/src/**`
- `crates/jellyflow-open-gpui/README.md`

**Approach:**

- Do not add `open_gpui`, `open_gpui_canvas`, or `open_gpui_ui_components` to `jellyflow-open-gpui`.
- Update README paths from `repo-ref/open-gpui/examples/canvas-jellyflow` to `examples/canvas-jellyflow`.
- Add or keep grep-style checks if needed to prevent concrete widget imports from entering the crate.

**Test scenarios:**

- `RUSTFLAGS='-Awarnings' cargo nextest run -p jellyflow-open-gpui --no-fail-fast --status-level fail --final-status-level fail`
- `rg "open_gpui|open-gpui" crates/jellyflow-open-gpui/src crates/jellyflow-open-gpui/Cargo.toml` should not show concrete Open GPUI crate dependencies; documentation references are allowed.

#### U4. Remove the Jellyflow example from the Open GPUI release workspace

**Goal:** Make `repo-ref/open-gpui` release-ready without reverse path dependencies to Jellyflow.

**Requirements:** R1, R2, R3, R14, AE1.

**Files:**

- `repo-ref/open-gpui/Cargo.toml`
- `repo-ref/open-gpui/README.md`
- Delete `repo-ref/open-gpui/examples/canvas-jellyflow/**`
- `repo-ref/open-gpui/Cargo.lock` if the workspace lock changes

**Approach:**

- Remove `"examples/canvas-jellyflow"` from workspace members.
- Remove the Jellyflow example from Open GPUI repository layout and run-command docs.
- Keep `examples/canvas-notes`, `examples/ui-foundation-gallery`, `examples/smoke-native`, and generic canvas tests.
- Run Open GPUI formatting/targeted checks from `repo-ref/open-gpui`.

**Test scenarios:**

- `git -C repo-ref/open-gpui diff --check`
- `cargo fmt --manifest-path repo-ref/open-gpui/Cargo.toml --all -- --check`
- `RUSTFLAGS='-Awarnings' cargo check --manifest-path repo-ref/open-gpui/Cargo.toml -p open-gpui-canvas -p open-gpui-canvas-notes`
- `rg "canvas-jellyflow|jellyflow" repo-ref/open-gpui/Cargo.toml repo-ref/open-gpui/README.md repo-ref/open-gpui/examples repo-ref/open-gpui/crates` returns no product-specific references, except intentional historical notes if any are explicitly justified.

#### U5. Update Jellyflow documentation and runbook paths

**Goal:** Make the new ownership discoverable and remove stale run commands.

**Requirements:** R5, R14, AE5.

**Files:**

- `README.md`
- `docs/examples/README.md`
- `docs/testing/node-ui-authoring-regression.md`
- `crates/jellyflow-open-gpui/README.md`
- `docs/knowledge/engineering/current-state.md`
- `docs/knowledge/engineering/log.md`

**Approach:**

- Replace `repo-ref/open-gpui/examples/canvas-jellyflow` with `examples/canvas-jellyflow` for Jellyflow product gallery commands.
- Keep wording clear that the example currently depends on a pinned Open GPUI git revision until Open GPUI 0.2 crates are published. Mention that local path dependencies to `repo-ref/open-gpui` are not the default because they conflict with Cargo workspace inheritance in this mixed local-workspace setup.
- Preserve the architecture language: semantic contracts in Jellyflow, concrete widgets in the Open GPUI host example.

**Test scenarios:**

- `rg "repo-ref/open-gpui/examples/canvas-jellyflow|cargo run -p open-gpui-canvas-jellyflow" README.md docs/examples/README.md docs/testing/node-ui-authoring-regression.md crates/jellyflow-open-gpui/README.md docs/knowledge/engineering/current-state.md` returns no stale current run-command guidance. Historical plans, progress notes, and verification logs may retain old paths as historical facts.
- Manual scan of the changed docs confirms the new command is copy-paste runnable.

#### U6. Registry availability and dependency cleanup checkpoint

**Goal:** Avoid pretending Open GPUI 0.2 is published when only local path dependencies are currently available.

**Requirements:** R4, R15, AE6.

**Files:**

- `examples/canvas-jellyflow/Cargo.toml`
- `examples/canvas-jellyflow/README.md`
- `docs/examples/README.md`

**Approach:**

- Query crates.io with `cargo search open-gpui`, `cargo search open-gpui-canvas`, and `cargo search open-gpui-ui-components`.
- If all required Open GPUI 0.2 crates are available, switch the example to registry dependencies and document optional local development override.
- If not available, keep the pinned Open GPUI git revision and document why local `repo-ref/open-gpui` path dependencies are avoided.

**Test scenarios:**

- `cargo metadata --manifest-path examples/canvas-jellyflow/Cargo.toml --no-deps` proves the chosen dependency mode resolves.
- The docs explicitly match the chosen mode.

#### U7. Verification, commits, and engineering memory

**Goal:** Close the migration with durable evidence and clean cross-repo state.

**Requirements:** R1-R15, AE1-AE6.

**Files:**

- `docs/knowledge/engineering/current-state.md`
- `docs/knowledge/engineering/log.md`
- New verification/progress note if the implementation changes enough to warrant one.

**Approach:**

- Run the verification contract below.
- Commit Jellyflow root changes separately from `repo-ref/open-gpui` changes.
- Push only if the user explicitly requests it during execution or if the active goal instruction has already authorized push for that repo.
- Record the new path, dependency mode, and any skipped gate in engineering memory.

**Test scenarios:**

- Root and nested repo `git status --short --branch` show only intentional changes before commit.
- Final status is clean or documented with intentionally uncommitted user changes.

## Verification Contract

Run from Jellyflow root unless a command says otherwise.

### Required gates

```sh
cargo fmt --all --check
cargo fmt --manifest-path examples/canvas-jellyflow/Cargo.toml --all -- --check
git diff --check
RUSTFLAGS='-Awarnings' cargo nextest run -p jellyflow-open-gpui --no-fail-fast --status-level fail --final-status-level fail
RUSTFLAGS='-Awarnings' cargo check --manifest-path examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow
RUSTFLAGS='-Awarnings' cargo nextest run --manifest-path examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow --no-fail-fast --status-level fail --final-status-level fail -E 'not test(gallery_screenshot::product_gallery_screenshot_exporter_writes_nonblank_pngs_or_skips)'
git -C repo-ref/open-gpui diff --check
cargo fmt --manifest-path repo-ref/open-gpui/Cargo.toml --all -- --check
RUSTFLAGS='-Awarnings' cargo check --manifest-path repo-ref/open-gpui/Cargo.toml -p open-gpui-canvas -p open-gpui-canvas-notes
```

### Boundary checks

```sh
rg "repo-ref/open-gpui/examples/canvas-jellyflow|cargo run -p open-gpui-canvas-jellyflow" README.md docs/examples/README.md docs/testing/node-ui-authoring-regression.md crates/jellyflow-open-gpui/README.md docs/knowledge/engineering/current-state.md
rg "examples/canvas-jellyflow|jellyflow" repo-ref/open-gpui/Cargo.toml repo-ref/open-gpui/README.md repo-ref/open-gpui/examples repo-ref/open-gpui/crates
cargo metadata --manifest-path examples/canvas-jellyflow/Cargo.toml --no-deps
```

Expected result:

- First `rg` returns no stale current Jellyflow run commands. Historical plans, progress notes, and verification logs are allowed to retain old paths as historical evidence.
- Second `rg` returns no Open GPUI release-workspace references to the Jellyflow product example; if it returns generic historical notes, document why they remain.
- Metadata resolves the migrated example without adding it to Jellyflow root workspace or requiring local Open GPUI path dependencies.

### Optional/manual gates

```sh
RUSTFLAGS='-Awarnings' cargo run --manifest-path examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow
RUSTFLAGS='-Awarnings' cargo test --manifest-path examples/canvas-jellyflow/Cargo.toml -p open-gpui-canvas-jellyflow product_gallery_native_smoke -- --nocapture --test-threads=1
```

Manual launch should show the same product gallery fixture set as before migration.

## Landing Strategy

- Commit 1, Jellyflow root: migrate `examples/canvas-jellyflow`, update manifests/docs/tests, and record engineering memory.
- Commit 2, `repo-ref/open-gpui`: remove the Jellyflow example from workspace/docs and commit the release-boundary cleanup.
- If implementation requires fixes to generic Open GPUI canvas APIs, keep those in the Open GPUI commit and explain why they are framework-level rather than Jellyflow-specific.
- Do not squash Jellyflow and Open GPUI commits together; they are separate repositories with separate release boundaries.

## Definition of Done

- `examples/canvas-jellyflow` is the only source location for the Jellyflow Open GPUI product gallery.
- `repo-ref/open-gpui` no longer has `examples/canvas-jellyflow` as a member or directory.
- Jellyflow docs and adapter README point to the new path and state the pinned Open GPUI dependency mode honestly.
- `jellyflow-open-gpui` still has no concrete Open GPUI widget dependencies.
- Required verification gates pass, or any skipped/blocked gate is documented with exact command, failure, and follow-up.
- Root and `repo-ref/open-gpui` commits are created with conventional commit messages if changes land.
