# Runtime Performance Baseline - 2026-06-12

## Summary

Jellyflow now has Criterion coverage for three runtime performance areas:

- `rendering_query`: large-graph visible ordering and culling reads.
- `schema_create_node`: schema descriptor enumeration, node instantiation, and store-level
  schema-driven node creation.
- `layout_pipeline`: runtime layout context construction, engine planning, transaction conversion,
  and store apply costs.

CI runs these benchmarks in Criterion `--test` mode. This catches compile errors and broken fixtures
without treating noisy runner timing as a regression signal.

## Commands

Run full local measurements with:

```text
cargo bench -p jellyflow-runtime --bench rendering_query
cargo bench -p jellyflow-runtime --bench schema_create_node
cargo bench -p jellyflow-runtime --bench layout_pipeline
```

Run the same smoke gate used by CI with:

```text
cargo bench -p jellyflow-runtime --bench rendering_query -- --test
cargo bench -p jellyflow-runtime --bench schema_create_node -- --test
cargo bench -p jellyflow-runtime --bench layout_pipeline -- --test
```

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
