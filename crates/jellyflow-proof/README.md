# jellyflow-proof

Proof crate for a second Jellyflow adapter boundary.

This crate is intentionally small. It demonstrates how a non-egui adapter can reuse the semantic
surface, schema registry, and headless store without depending on `jellyflow-egui`.

The crate is not a shared UI layer. It is a concrete proof that a second adapter can:

- build a rich node schema from `jellyflow::runtime::schema`;
- instantiate graph documents through the headless model;
- keep adapter-owned renderer keys and semantic slots separate.

Run:

```sh
cargo test -p jellyflow-proof
cargo run -p jellyflow-proof
```

