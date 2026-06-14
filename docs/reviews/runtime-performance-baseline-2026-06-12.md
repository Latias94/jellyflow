# Runtime Performance Baseline - 2026-06-12

## Summary

Jellyflow now has Criterion coverage for three runtime performance areas:

- `rendering_query`: large-graph visible ordering and culling reads.
- `schema_create_node`: schema descriptor enumeration, node instantiation, and store-level
  schema-driven node creation.
- `layout_pipeline`: runtime layout context construction, engine planning, transaction conversion,
  and store apply costs.
- `layout_engines`: native layout engine planning and transaction conversion costs.

CI runs these benchmarks in Criterion `--test` mode. This catches compile errors and broken fixtures
without treating noisy runner timing as a regression signal.

## Commands

Run full local measurements with:

```text
cargo bench -p jellyflow-runtime --bench rendering_query
cargo bench -p jellyflow-runtime --bench schema_create_node
cargo bench -p jellyflow-runtime --bench layout_pipeline
cargo bench -p jellyflow-layout --bench layout_engines
```

Run the same smoke gate used by CI with:

```text
cargo bench -p jellyflow-runtime --bench rendering_query -- --test
cargo bench -p jellyflow-runtime --bench schema_create_node -- --test
cargo bench -p jellyflow-runtime --bench layout_pipeline -- --test
cargo bench -p jellyflow-layout --bench layout_engines -- --test
```

## Layout Baseline

Local baseline from this workspace snapshot:

| Path | Fixture | Baseline |
| --- | --- | --- |
| `layout_pipeline/context_from_store` | 100 nodes | about `2.76 us` |
| `layout_pipeline/context_from_store` | 250 nodes | about `7.91 us` |
| `layout_pipeline/context_from_store` | 500 nodes | about `17.94 us` |
| `layout_pipeline/store_plan_layout` | 100 nodes | about `1.84 ms` |
| `layout_pipeline/store_plan_layout` | 250 nodes | about `6.92 ms` |
| `layout_pipeline/store_plan_layout` | 500 nodes | about `21.28 ms` |
| `layout_pipeline/to_transaction` | 100 nodes | about `4.54 us` |
| `layout_pipeline/to_transaction` | 250 nodes | about `10.67 us` |
| `layout_pipeline/to_transaction` | 500 nodes | about `24.77 us` |
| `layout_pipeline/apply` | 100 nodes | about `1.92 ms` |
| `layout_pipeline/apply` | 250 nodes | about `7.03 ms` |
| `layout_pipeline/apply` | 500 nodes | about `21.98 ms` |
| `layout_engines/tidy_tree_balanced/plan` | 121 nodes | about `54.9 us` |
| `layout_engines/tidy_tree_balanced/plan` | 1,093 nodes | about `1.73 ms` |
| `layout_engines/tidy_tree_balanced/plan` | 9,841 nodes | about `100 ms` |
| `layout_engines/tidy_tree_wide/plan` | 101 nodes | about `31.2 us` |
| `layout_engines/tidy_tree_wide/plan` | 1,001 nodes | about `756 us` |
| `layout_engines/tidy_tree_wide/plan` | 5,001 nodes | about `9.40 ms` |
| `layout_engines/dugong_layered/plan` | 100 nodes | about `1.84 ms` |
| `layout_engines/dugong_layered/plan` | 250 nodes | about `6.76 ms` |
| `layout_engines/dugong_layered/plan` | 500 nodes | about `21.46 ms` |
| `layout_engines/dugong_layered/to_transaction` | 100 nodes | about `4.63 us` |
| `layout_engines/dugong_layered/to_transaction` | 250 nodes | about `11.0 us` |
| `layout_engines/dugong_layered/to_transaction` | 500 nodes | about `22.1 us` |

## Layout Interpretation

- Runtime layout context construction and `LayoutResult::to_transaction` are microsecond-scale for
  the current 100/250/500-node fixtures. They are not the primary layout bottleneck.
- `apply_layout` mostly tracks engine planning cost plus normal dispatch work, which means runtime
  wrapper caching is unlikely to materially improve layout latency by itself.
- `dugong` planning dominates layered DAG layout time. Future performance work should instrument and
  optimize the `dugong` projection/build/solve path before adding runtime-level dirty-scope caches.
- Native tidy tree planning is much faster on tree-shaped fixtures than `dugong` on layered DAG
  fixtures, which supports routing tree/workflow presets to specialized engines where possible.

## Schema Create-Node Baseline

Local baseline from this workspace snapshot:

| Path | Fixture | Baseline |
| --- | --- | --- |
| `view_descriptors` | 10 schemas | about `4.2 us` |
| `view_descriptors` | 1,000 schemas | about `455 us` |
| `instantiate_node` | 1 port | about `1.66 us` |
| `instantiate_node` | 4 ports | about `3.90 us` |
| `instantiate_node` | 16 ports | about `12.63 us` |
| `apply_create_node_from_schema` | 1 port | about `3.76 us` |
| `apply_create_node_from_schema` | 4 ports | about `6.91 us` |
| `apply_create_node_from_schema` | 16 ports | about `20.08 us` |

## Interpretation

- `view_descriptors` is linear in schema count because it returns owned adapter-facing descriptors.
- `instantiate_node` scales with port count because it allocates node/port records, ids, and default
  data.
- `apply_create_node_from_schema` intentionally includes normal store dispatch, history, profile,
  lookup, and patch work. It should not bypass the dispatch pipeline unless adapter traces prove
  this path is user-visible overhead.

## Optimization Rules

Only optimize this area when one of these signals exists:

- create-node appears in adapter interaction traces as visible latency;
- descriptor enumeration becomes hot with very large schema catalogs;
- a future change roughly doubles a benchmark without a behavior reason.

Prefer this order:

1. Remove accidental allocation or cloning in the hot path.
2. Reuse existing owned data where the public API already allows it.
3. Add a specialized fast path only when both the benchmark and adapter traces justify it.
