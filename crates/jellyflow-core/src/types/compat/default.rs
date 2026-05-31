use std::collections::BTreeMap;

use crate::types::TypeDesc;

use super::{TypeCompatibility, TypeCompatibilityResult};

/// A small, conservative default compatibility table.
///
/// This is intentionally minimal: rich domains (shader precision/space, JSON schema, tool
/// signatures) should implement their own policy.
#[derive(Debug, Default, Clone)]
pub struct DefaultTypeCompatibility;

impl TypeCompatibility for DefaultTypeCompatibility {
    fn compatible(&mut self, from: &TypeDesc, to: &TypeDesc) -> TypeCompatibilityResult {
        use TypeCompatibilityResult::Compatible;
        use TypeDesc::*;

        match (from, to) {
            (_, Any) => Compatible,
            (Any, _) => Compatible,
            (_, Unknown) => Compatible,
            (Unknown, _) => Compatible,

            (Never, _) => Compatible,
            (_, Never) => incompatible("cannot assign into never"),

            (Null, Null) => Compatible,
            (Null, Option { .. }) => Compatible,
            (Null, Union { types }) => compatible_null_to_union(types),

            (Bool, Bool) | (Int, Int) | (Float, Float) | (String, String) | (Bytes, Bytes) => {
                Compatible
            }

            (Int, Float) => Compatible,

            (Option { of }, other) => self.compatible(of, other),
            (other, Option { of }) => {
                if matches!(other, Null) {
                    Compatible
                } else {
                    self.compatible(other, of)
                }
            }

            (Union { types }, other) => self.compatible_union_source(types, other),
            (other, Union { types }) => self.compatible_union_target(other, types),

            (List { of: a }, List { of: b }) => self.compatible(a, b),

            (Map { key: ak, value: av }, Map { key: bk, value: bv }) => {
                self.compatible_map(ak, av, bk, bv)
            }

            (
                Object {
                    fields: a_fields,
                    open: a_open,
                },
                Object {
                    fields: b_fields,
                    open: b_open,
                },
            ) => self.compatible_object(a_fields, *a_open, b_fields, *b_open),

            (Var { id: _ }, _) | (_, Var { id: _ }) => Compatible,

            (
                Opaque {
                    key: ak,
                    params: ap,
                },
                Opaque {
                    key: bk,
                    params: bp,
                },
            ) => self.compatible_opaque(ak, ap, bk, bp),

            _ => incompatible("types are incompatible"),
        }
    }
}

impl DefaultTypeCompatibility {
    fn compatible_union_source(
        &mut self,
        types: &[TypeDesc],
        other: &TypeDesc,
    ) -> TypeCompatibilityResult {
        for ty in types {
            if !self.compatible(ty, other).is_compatible() {
                return incompatible("union member is incompatible");
            }
        }
        TypeCompatibilityResult::Compatible
    }

    fn compatible_union_target(
        &mut self,
        other: &TypeDesc,
        types: &[TypeDesc],
    ) -> TypeCompatibilityResult {
        for ty in types {
            if self.compatible(other, ty).is_compatible() {
                return TypeCompatibilityResult::Compatible;
            }
        }
        incompatible("no union member is compatible")
    }

    fn compatible_map(
        &mut self,
        source_key: &TypeDesc,
        source_value: &TypeDesc,
        target_key: &TypeDesc,
        target_value: &TypeDesc,
    ) -> TypeCompatibilityResult {
        if !self.compatible(source_key, target_key).is_compatible() {
            return incompatible("map key types are incompatible");
        }
        self.compatible(source_value, target_value)
    }

    fn compatible_object(
        &mut self,
        source_fields: &BTreeMap<String, TypeDesc>,
        source_open: bool,
        target_fields: &BTreeMap<String, TypeDesc>,
        target_open: bool,
    ) -> TypeCompatibilityResult {
        // Conservative rule:
        // - closed target requires exact field set,
        // - open target requires at least the target fields.
        if !target_open && source_fields.len() != target_fields.len() {
            return incompatible("closed object requires exact field set");
        }

        for (name, target_ty) in target_fields {
            let Some(source_ty) = source_fields.get(name) else {
                return incompatible(format!("missing required field: {name}"));
            };
            if !self.compatible(source_ty, target_ty).is_compatible() {
                return incompatible(format!("field type mismatch: {name}"));
            }
        }

        if !target_open && source_open {
            // An open source may contain additional unknown fields; keep conservative.
            return incompatible("open object is not assignable to closed object");
        }

        TypeCompatibilityResult::Compatible
    }

    fn compatible_opaque(
        &mut self,
        source_key: &str,
        source_params: &[TypeDesc],
        target_key: &str,
        target_params: &[TypeDesc],
    ) -> TypeCompatibilityResult {
        if source_key != target_key {
            return incompatible("opaque type keys differ");
        }
        if source_params.len() != target_params.len() {
            return incompatible("opaque type arity differs");
        }
        for (source, target) in source_params.iter().zip(target_params.iter()) {
            if !self.compatible(source, target).is_compatible() {
                return incompatible("opaque type parameter mismatch");
            }
        }
        TypeCompatibilityResult::Compatible
    }
}

fn compatible_null_to_union(types: &[TypeDesc]) -> TypeCompatibilityResult {
    if types.iter().any(|ty| matches!(ty, TypeDesc::Null)) {
        TypeCompatibilityResult::Compatible
    } else {
        incompatible("null is not a member of the union")
    }
}

fn incompatible(reason: impl Into<String>) -> TypeCompatibilityResult {
    TypeCompatibilityResult::Incompatible {
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compatibility(from: TypeDesc, to: TypeDesc) -> TypeCompatibilityResult {
        let mut compat = DefaultTypeCompatibility;
        compat.compatible(&from, &to)
    }

    fn assert_compatible(from: TypeDesc, to: TypeDesc) {
        assert!(compatibility(from, to).is_compatible());
    }

    fn assert_incompatible(from: TypeDesc, to: TypeDesc, expected_reason: &str) {
        assert_eq!(
            compatibility(from, to),
            TypeCompatibilityResult::Incompatible {
                reason: expected_reason.to_string(),
            }
        );
    }

    fn object(fields: &[(&str, TypeDesc)], open: bool) -> TypeDesc {
        TypeDesc::Object {
            fields: fields
                .iter()
                .map(|(name, ty)| ((*name).to_string(), ty.clone()))
                .collect(),
            open,
        }
    }

    #[test]
    fn union_target_accepts_any_compatible_member() {
        assert_compatible(
            TypeDesc::Int,
            TypeDesc::Union {
                types: vec![TypeDesc::String, TypeDesc::Float],
            },
        );
        assert_incompatible(
            TypeDesc::Bool,
            TypeDesc::Union {
                types: vec![TypeDesc::String, TypeDesc::Int],
            },
            "no union member is compatible",
        );
    }

    #[test]
    fn object_compatibility_enforces_target_shape() {
        assert_compatible(
            object(&[("name", TypeDesc::String), ("age", TypeDesc::Int)], false),
            object(&[("name", TypeDesc::String)], true),
        );
        assert_incompatible(
            object(&[("name", TypeDesc::String), ("age", TypeDesc::Int)], false),
            object(&[("name", TypeDesc::String)], false),
            "closed object requires exact field set",
        );
        assert_incompatible(
            object(&[("name", TypeDesc::String)], true),
            object(&[("name", TypeDesc::String)], false),
            "open object is not assignable to closed object",
        );
    }

    #[test]
    fn opaque_compatibility_checks_key_arity_and_params() {
        assert_compatible(
            TypeDesc::Opaque {
                key: "vec".to_string(),
                params: vec![TypeDesc::Int],
            },
            TypeDesc::Opaque {
                key: "vec".to_string(),
                params: vec![TypeDesc::Float],
            },
        );
        assert_incompatible(
            TypeDesc::Opaque {
                key: "vec".to_string(),
                params: vec![TypeDesc::String],
            },
            TypeDesc::Opaque {
                key: "vec".to_string(),
                params: vec![TypeDesc::Float],
            },
            "opaque type parameter mismatch",
        );
    }
}
