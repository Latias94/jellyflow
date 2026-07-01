use jellyflow::runtime::schema::{
    NodeControlBinding, NodeControlBindingSource, NodeRepeatableCollectionDescriptor,
};
use serde_json::Value;

pub(crate) fn read_bound_value<'a>(
    data: &'a Value,
    binding: &NodeControlBinding,
    field_row_slot_fallback: bool,
) -> Option<&'a Value> {
    match binding.source {
        NodeControlBindingSource::DataPath => semantic_json_lookup(data, &binding.path),
        NodeControlBindingSource::Slot => {
            if field_row_slot_fallback
                && let Some(fields) = data.get("fields").and_then(Value::as_object)
                && let Some(value) = fields.get(&binding.path)
            {
                return Some(value);
            }
            semantic_json_lookup(data, &binding.path)
        }
        NodeControlBindingSource::JsonPointer => data.pointer(&binding.path),
        NodeControlBindingSource::GraphSymbol | NodeControlBindingSource::PortAnchor => None,
    }
}

pub(crate) fn set_bound_value(
    data: &mut Value,
    binding: &NodeControlBinding,
    value: Value,
) -> Result<(), String> {
    match binding.source {
        NodeControlBindingSource::DataPath | NodeControlBindingSource::Slot => {
            set_dot_path_value(data, &binding.path, value)
        }
        NodeControlBindingSource::JsonPointer => set_json_pointer_value(data, &binding.path, value),
        NodeControlBindingSource::GraphSymbol | NodeControlBindingSource::PortAnchor => {
            Err(format!(
                "binding source `{:?}` is not writable by the GPUI node adapter",
                binding.source
            ))
        }
    }
}

pub(crate) fn set_dot_path_value(
    value: &mut Value,
    path: &str,
    new_value: Value,
) -> Result<(), String> {
    let segments = path
        .split('.')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if segments.is_empty() {
        *value = new_value;
        return Ok(());
    }
    set_path_segments(value, &segments, new_value)
}

pub(crate) fn set_json_pointer_value(
    value: &mut Value,
    pointer: &str,
    new_value: Value,
) -> Result<(), String> {
    if pointer.is_empty() {
        *value = new_value;
        return Ok(());
    }
    let Some(pointer) = pointer.strip_prefix('/') else {
        return Err(format!("json pointer `{pointer}` must start with `/`"));
    };
    let segments = pointer
        .split('/')
        .map(|segment| segment.replace("~1", "/").replace("~0", "~"))
        .collect::<Vec<_>>();
    let borrowed = segments.iter().map(String::as_str).collect::<Vec<_>>();
    set_path_segments(value, &borrowed, new_value)
}

pub(crate) fn semantic_json_lookup<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut cursor = value;
    for segment in path.split('.') {
        if segment.is_empty() {
            continue;
        }
        cursor = match cursor {
            Value::Object(map) => map.get(segment)?,
            Value::Array(items) => items.get(segment.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(cursor)
}

pub(crate) fn join_data_path(prefix: &str, suffix: &str) -> String {
    if suffix.is_empty() {
        prefix.to_owned()
    } else {
        format!("{prefix}.{suffix}")
    }
}

pub(crate) fn field_row_slot_data_path(path: &str) -> String {
    join_data_path("fields", path)
}

pub(crate) fn repeatable_item_id(
    collection: &NodeRepeatableCollectionDescriptor,
    item: &Value,
) -> Option<String> {
    let raw = semantic_json_lookup(item, &collection.item_id_path)
        .and_then(json_scalar_to_stable_string)?;
    sanitize_repeatable_key(&raw).map(ToOwned::to_owned)
}

pub(crate) fn json_scalar_to_stable_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn sanitize_repeatable_key(value: &str) -> Option<&str> {
    (!value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')))
    .then_some(value)
}

fn set_path_segments(value: &mut Value, segments: &[&str], new_value: Value) -> Result<(), String> {
    let Some((segment, rest)) = segments.split_first() else {
        *value = new_value;
        return Ok(());
    };

    if rest.is_empty() {
        match value {
            Value::Object(map) => {
                map.insert((*segment).to_owned(), new_value);
                Ok(())
            }
            Value::Array(items) => {
                let index = segment
                    .parse::<usize>()
                    .map_err(|_| format!("array path segment `{segment}` is not an index"))?;
                let Some(slot) = items.get_mut(index) else {
                    return Err(format!("array index `{index}` is out of bounds"));
                };
                *slot = new_value;
                Ok(())
            }
            Value::Null => {
                let mut map = serde_json::Map::new();
                map.insert((*segment).to_owned(), new_value);
                *value = Value::Object(map);
                Ok(())
            }
            _ => Err(format!(
                "cannot set path segment `{segment}` on scalar value"
            )),
        }
    } else {
        match value {
            Value::Object(map) => {
                let child = map
                    .entry((*segment).to_owned())
                    .or_insert_with(|| Value::Object(serde_json::Map::new()));
                set_path_segments(child, rest, new_value)
            }
            Value::Array(items) => {
                let index = segment
                    .parse::<usize>()
                    .map_err(|_| format!("array path segment `{segment}` is not an index"))?;
                let Some(child) = items.get_mut(index) else {
                    return Err(format!("array index `{index}` is out of bounds"));
                };
                set_path_segments(child, rest, new_value)
            }
            Value::Null => {
                *value = Value::Object(serde_json::Map::new());
                set_path_segments(value, segments, new_value)
            }
            _ => Err(format!(
                "cannot traverse path segment `{segment}` on scalar value"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jellyflow::runtime::schema::{NodeControlBinding, NodeRepeatableCollectionDescriptor};
    use serde_json::json;

    #[test]
    fn dot_path_lookup_and_write_preserve_existing_semantics() {
        let mut value = json!({
            "items": [
                { "name": "old" }
            ]
        });

        assert_eq!(
            semantic_json_lookup(&value, "items.0.name"),
            Some(&json!("old"))
        );
        set_dot_path_value(&mut value, "items.0.name", json!("new")).expect("array path write");
        assert_eq!(value["items"][0]["name"], json!("new"));

        set_dot_path_value(&mut value, "meta.prompt", json!("hello"))
            .expect("nested object creation");
        assert_eq!(value["meta"]["prompt"], json!("hello"));

        set_dot_path_value(&mut value, "", json!({ "root": true })).expect("root replacement");
        assert_eq!(value, json!({ "root": true }));

        let mut scalar = json!({ "a": 1 });
        let error =
            set_dot_path_value(&mut scalar, "a.b.c", json!(2)).expect_err("scalar traversal");
        assert_eq!(error, "cannot traverse path segment `b` on scalar value");
    }

    #[test]
    fn json_pointer_lookup_and_write_preserve_existing_semantics() {
        let mut value = json!({
            "a/b": {
                "~key": "old"
            }
        });
        let binding = NodeControlBinding::json_pointer("/a~1b/~0key");

        assert_eq!(
            read_bound_value(&value, &binding, false),
            Some(&json!("old"))
        );
        set_json_pointer_value(&mut value, "/a~1b/~0key", json!("new"))
            .expect("decoded json pointer write");
        assert_eq!(value["a/b"]["~key"], json!("new"));

        let error = set_json_pointer_value(&mut value, "a/b", json!(1))
            .expect_err("pointer must start with slash");
        assert_eq!(error, "json pointer `a/b` must start with `/`");

        set_json_pointer_value(&mut value, "", json!({ "root": "replaced" }))
            .expect("empty pointer root replacement");
        assert_eq!(value, json!({ "root": "replaced" }));
    }

    #[test]
    fn field_row_slot_binding_prefers_fields_namespace() {
        let value = json!({
            "prompt": "top-level",
            "fields": {
                "prompt": "field-row"
            }
        });
        let binding = NodeControlBinding::slot("prompt");

        assert_eq!(
            read_bound_value(&value, &binding, true),
            Some(&json!("field-row"))
        );
        assert_eq!(
            read_bound_value(&value, &binding, false),
            Some(&json!("top-level"))
        );
        assert_eq!(field_row_slot_data_path("prompt"), "fields.prompt");
    }

    #[test]
    fn repeatable_item_id_accepts_stable_scalars_and_rejects_invalid_ids() {
        let collection = NodeRepeatableCollectionDescriptor::new("demo.items", "items", "id");

        assert_eq!(
            repeatable_item_id(&collection, &json!({ "id": "alpha_1" })),
            Some("alpha_1".to_owned())
        );
        assert_eq!(
            repeatable_item_id(&collection, &json!({ "id": 7 })),
            Some("7".to_owned())
        );
        assert_eq!(
            repeatable_item_id(&collection, &json!({ "id": true })),
            Some("true".to_owned())
        );
        assert_eq!(repeatable_item_id(&collection, &json!({ "id": "" })), None);
        assert_eq!(
            repeatable_item_id(&collection, &json!({ "id": "bad id" })),
            None
        );
        assert_eq!(
            repeatable_item_id(&collection, &json!({ "id": ["bad"] })),
            None
        );
    }
}
