//! Serde decorators that enforce parse budgets while values are decoded.

use std::cell::RefCell;
use std::fmt;

use serde::Deserializer;
use serde::de::{
    self, DeserializeOwned, DeserializeSeed, EnumAccess, MapAccess, SeqAccess, VariantAccess,
    Visitor,
};

use super::{
    BoundedJsonDecodeError, PersistenceLimitError, PersistenceLimits, PersistenceResource,
};

pub(crate) fn deserialize_json_with_limits<T>(
    bytes: &[u8],
    limits: PersistenceLimits,
) -> Result<T, BoundedJsonDecodeError>
where
    T: DeserializeOwned,
{
    deserialize_json_with_budget_mode(bytes, limits, true)
}

pub(crate) fn deserialize_json_envelope_with_limits<T>(
    bytes: &[u8],
    limits: PersistenceLimits,
) -> Result<T, BoundedJsonDecodeError>
where
    T: DeserializeOwned,
{
    deserialize_json_with_budget_mode(bytes, limits, false)
}

pub(crate) fn validate_json_depth_with_limits(
    bytes: &[u8],
    limits: PersistenceLimits,
) -> Result<(), BoundedJsonDecodeError> {
    deserialize_json_envelope_with_limits::<de::IgnoredAny>(bytes, limits).map(|_| ())
}

fn deserialize_json_with_budget_mode<T>(
    bytes: &[u8],
    limits: PersistenceLimits,
    charge_ignored_values: bool,
) -> Result<T, BoundedJsonDecodeError>
where
    T: DeserializeOwned,
{
    let tracker = ParseBudgetTracker::new(limits, charge_ignored_values);
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    deserializer.disable_recursion_limit();
    let value = T::deserialize(BudgetedDeserializer::new(&mut deserializer, &tracker, 1));
    match value {
        Ok(value) => {
            deserializer.end().map_err(BoundedJsonDecodeError::Parse)?;
            Ok(value)
        }
        Err(source) => match tracker.failure() {
            Some(error) => Err(BoundedJsonDecodeError::Limit(error)),
            None => Err(BoundedJsonDecodeError::Parse(source)),
        },
    }
}

struct ParseBudgetTracker {
    state: RefCell<ParseBudgetState>,
    charge_ignored_values: bool,
}

#[derive(Clone, Copy)]
enum ParseResource {
    CollectionItems,
    StringBytes,
    JsonValues,
}

impl ParseResource {
    fn persistence_resource(self) -> PersistenceResource {
        match self {
            Self::CollectionItems => PersistenceResource::CollectionItems,
            Self::StringBytes => PersistenceResource::StringBytes,
            Self::JsonValues => PersistenceResource::JsonValues,
        }
    }
}

struct ParseBudgetState {
    limits: PersistenceLimits,
    collection_items: usize,
    string_bytes: usize,
    json_values: usize,
    failure: Option<PersistenceLimitError>,
}

impl ParseBudgetTracker {
    fn new(limits: PersistenceLimits, charge_ignored_values: bool) -> Self {
        Self {
            state: RefCell::new(ParseBudgetState {
                limits,
                collection_items: 0,
                string_bytes: 0,
                json_values: 0,
                failure: None,
            }),
            charge_ignored_values,
        }
    }

    fn charge_ignored_values(&self) -> bool {
        self.charge_ignored_values
    }

    fn failure(&self) -> Option<PersistenceLimitError> {
        self.state.borrow().failure.clone()
    }

    fn remaining_collection_items(&self) -> usize {
        let state = self.state.borrow();
        state
            .limits
            .max_collection_items()
            .saturating_sub(state.collection_items)
    }

    fn check_depth<E>(&self, depth: usize) -> Result<(), E>
    where
        E: de::Error,
    {
        let limits = self.state.borrow().limits;
        self.record(limits.ensure(PersistenceResource::JsonDepth, depth))
    }

    fn add_collection_item<E>(&self) -> Result<(), E>
    where
        E: de::Error,
    {
        self.charge(ParseResource::CollectionItems, 1)
    }

    fn add_string_bytes<E>(&self, bytes: usize) -> Result<(), E>
    where
        E: de::Error,
    {
        self.charge(ParseResource::StringBytes, bytes)
    }

    fn add_json_value<E>(&self) -> Result<(), E>
    where
        E: de::Error,
    {
        self.charge(ParseResource::JsonValues, 1)
    }

    fn charge<E>(&self, resource: ParseResource, amount: usize) -> Result<(), E>
    where
        E: de::Error,
    {
        let result = {
            let mut state = self.state.borrow_mut();
            let actual = match resource {
                ParseResource::CollectionItems => {
                    state.collection_items = state.collection_items.saturating_add(amount);
                    state.collection_items
                }
                ParseResource::StringBytes => {
                    state.string_bytes = state.string_bytes.saturating_add(amount);
                    state.string_bytes
                }
                ParseResource::JsonValues => {
                    state.json_values = state.json_values.saturating_add(amount);
                    state.json_values
                }
            };
            state.limits.ensure(resource.persistence_resource(), actual)
        };
        self.record(result)
    }

    fn record<E>(&self, result: Result<(), PersistenceLimitError>) -> Result<(), E>
    where
        E: de::Error,
    {
        result.map_err(|error| {
            let message = error.to_string();
            let mut state = self.state.borrow_mut();
            if state.failure.is_none() {
                state.failure = Some(error);
            }
            E::custom(message)
        })
    }
}

struct BudgetedDeserializer<'a, D> {
    inner: D,
    tracker: &'a ParseBudgetTracker,
    depth: usize,
    depth_checked: bool,
    count_identifiers: bool,
}

impl<'a, D> BudgetedDeserializer<'a, D> {
    fn new(inner: D, tracker: &'a ParseBudgetTracker, depth: usize) -> Self {
        Self {
            inner,
            tracker,
            depth,
            depth_checked: false,
            count_identifiers: false,
        }
    }

    fn checked(
        inner: D,
        tracker: &'a ParseBudgetTracker,
        depth: usize,
        count_identifiers: bool,
    ) -> Self {
        Self {
            inner,
            tracker,
            depth,
            depth_checked: true,
            count_identifiers,
        }
    }

    fn prepare<E>(&self) -> Result<(), E>
    where
        E: de::Error,
    {
        if self.depth_checked {
            Ok(())
        } else {
            self.tracker.check_depth(self.depth)
        }
    }

    fn visitor<V>(
        &self,
        visitor: V,
        count_strings: bool,
        count_collection_items: bool,
    ) -> BudgetedVisitor<'a, V> {
        BudgetedVisitor {
            inner: visitor,
            tracker: self.tracker,
            depth: self.depth,
            count_strings,
            count_collection_items,
            count_identifiers: self.count_identifiers,
        }
    }
}

macro_rules! delegate_deserialize {
    ($method:ident $(, $arg:ident : $arg_ty:ty)*) => {
        fn $method<V>(self, $($arg: $arg_ty,)* visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.prepare::<Self::Error>()?;
            let visitor = self.visitor(visitor, false, true);
            self.inner.$method($($arg,)* visitor)
        }
    };
}

impl<'de, D> Deserializer<'de> for BudgetedDeserializer<'_, D>
where
    D: Deserializer<'de>,
{
    type Error = D::Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.prepare::<Self::Error>()?;
        self.tracker.add_json_value::<Self::Error>()?;
        let visitor = self.visitor(visitor, true, true);
        self.inner.deserialize_any(visitor)
    }

    delegate_deserialize!(deserialize_bool);
    delegate_deserialize!(deserialize_i8);
    delegate_deserialize!(deserialize_i16);
    delegate_deserialize!(deserialize_i32);
    delegate_deserialize!(deserialize_i64);
    delegate_deserialize!(deserialize_i128);
    delegate_deserialize!(deserialize_u8);
    delegate_deserialize!(deserialize_u16);
    delegate_deserialize!(deserialize_u32);
    delegate_deserialize!(deserialize_u64);
    delegate_deserialize!(deserialize_u128);
    delegate_deserialize!(deserialize_f32);
    delegate_deserialize!(deserialize_f64);
    delegate_deserialize!(deserialize_char);

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.prepare::<Self::Error>()?;
        let visitor = self.visitor(visitor, self.count_identifiers, true);
        self.inner.deserialize_str(visitor)
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.prepare::<Self::Error>()?;
        let visitor = self.visitor(visitor, true, true);
        self.inner.deserialize_string(visitor)
    }

    delegate_deserialize!(deserialize_bytes);
    delegate_deserialize!(deserialize_byte_buf);
    delegate_deserialize!(deserialize_option);
    delegate_deserialize!(deserialize_unit);
    delegate_deserialize!(deserialize_unit_struct, name: &'static str);
    delegate_deserialize!(deserialize_newtype_struct, name: &'static str);
    delegate_deserialize!(deserialize_seq);
    delegate_deserialize!(deserialize_tuple, len: usize);
    delegate_deserialize!(deserialize_tuple_struct, name: &'static str, len: usize);

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.prepare::<Self::Error>()?;
        let visitor = self.visitor(visitor, false, true);
        self.inner.deserialize_map(visitor)
    }

    fn deserialize_struct<V>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.prepare::<Self::Error>()?;
        let visitor = self.visitor(visitor, false, false);
        self.inner.deserialize_struct(name, fields, visitor)
    }

    delegate_deserialize!(deserialize_enum, name: &'static str, variants: &'static [&'static str]);

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.prepare::<Self::Error>()?;
        let visitor = self.visitor(visitor, self.count_identifiers, false);
        self.inner.deserialize_identifier(visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.prepare::<Self::Error>()?;
        let charge_value = self.tracker.charge_ignored_values();
        if charge_value {
            self.tracker.add_json_value::<Self::Error>()?;
        }
        let visitor = self.visitor(visitor, charge_value, charge_value);
        self.inner.deserialize_any(visitor)
    }

    fn is_human_readable(&self) -> bool {
        self.inner.is_human_readable()
    }
}

struct BudgetedSeed<'a, S> {
    inner: S,
    tracker: &'a ParseBudgetTracker,
    depth: usize,
    count_collection_item: bool,
    count_identifiers: bool,
}

impl<'de, S> DeserializeSeed<'de> for BudgetedSeed<'_, S>
where
    S: DeserializeSeed<'de>,
{
    type Value = S::Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        if self.count_collection_item {
            self.tracker.add_collection_item::<D::Error>()?;
        }
        self.tracker.check_depth::<D::Error>(self.depth)?;
        self.inner.deserialize(BudgetedDeserializer::checked(
            deserializer,
            self.tracker,
            self.depth,
            self.count_identifiers,
        ))
    }
}

struct BudgetedVisitor<'a, V> {
    inner: V,
    tracker: &'a ParseBudgetTracker,
    depth: usize,
    count_strings: bool,
    count_collection_items: bool,
    count_identifiers: bool,
}

macro_rules! delegate_visit {
    ($method:ident, $value_ty:ty) => {
        fn $method<E>(self, value: $value_ty) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.inner.$method(value)
        }
    };
}

impl<'de, V> Visitor<'de> for BudgetedVisitor<'_, V>
where
    V: Visitor<'de>,
{
    type Value = V::Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.expecting(formatter)
    }

    delegate_visit!(visit_bool, bool);
    delegate_visit!(visit_i8, i8);
    delegate_visit!(visit_i16, i16);
    delegate_visit!(visit_i32, i32);
    delegate_visit!(visit_i64, i64);
    delegate_visit!(visit_i128, i128);
    delegate_visit!(visit_u8, u8);
    delegate_visit!(visit_u16, u16);
    delegate_visit!(visit_u32, u32);
    delegate_visit!(visit_u64, u64);
    delegate_visit!(visit_u128, u128);
    delegate_visit!(visit_f32, f32);
    delegate_visit!(visit_f64, f64);
    delegate_visit!(visit_char, char);

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        if self.count_strings {
            self.tracker.add_string_bytes::<E>(value.len())?;
        }
        self.inner.visit_str(value)
    }

    fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        if self.count_strings {
            self.tracker.add_string_bytes::<E>(value.len())?;
        }
        self.inner.visit_borrowed_str(value)
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        if self.count_strings {
            self.tracker.add_string_bytes::<E>(value.len())?;
        }
        self.inner.visit_string(value)
    }

    fn visit_bytes<E>(self, value: &[u8]) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.inner.visit_bytes(value)
    }

    fn visit_borrowed_bytes<E>(self, value: &'de [u8]) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.inner.visit_borrowed_bytes(value)
    }

    fn visit_byte_buf<E>(self, value: Vec<u8>) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.inner.visit_byte_buf(value)
    }

    fn visit_none<E>(self) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.inner.visit_none()
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        self.inner.visit_some(BudgetedDeserializer::checked(
            deserializer,
            self.tracker,
            self.depth,
            self.count_identifiers,
        ))
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.inner.visit_unit()
    }

    fn visit_newtype_struct<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        self.inner
            .visit_newtype_struct(BudgetedDeserializer::checked(
                deserializer,
                self.tracker,
                self.depth,
                self.count_identifiers,
            ))
    }

    fn visit_seq<A>(self, sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        self.inner.visit_seq(BudgetedSeqAccess {
            inner: sequence,
            tracker: self.tracker,
            depth: self.depth.saturating_add(1),
            count_items: self.count_collection_items,
        })
    }

    fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        self.inner.visit_map(BudgetedMapAccess {
            inner: map,
            tracker: self.tracker,
            depth: self.depth.saturating_add(1),
            count_entries: self.count_collection_items,
        })
    }

    fn visit_enum<A>(self, data: A) -> Result<Self::Value, A::Error>
    where
        A: EnumAccess<'de>,
    {
        self.inner.visit_enum(BudgetedEnumAccess {
            inner: data,
            tracker: self.tracker,
            depth: self.depth,
        })
    }
}

struct BudgetedSeqAccess<'a, A> {
    inner: A,
    tracker: &'a ParseBudgetTracker,
    depth: usize,
    count_items: bool,
}

impl<'de, A> SeqAccess<'de> for BudgetedSeqAccess<'_, A>
where
    A: SeqAccess<'de>,
{
    type Error = A::Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        self.inner.next_element_seed(BudgetedSeed {
            inner: seed,
            tracker: self.tracker,
            depth: self.depth,
            count_collection_item: self.count_items,
            count_identifiers: false,
        })
    }

    fn size_hint(&self) -> Option<usize> {
        self.inner.size_hint().map(|hint| {
            if self.count_items {
                hint.min(self.tracker.remaining_collection_items())
            } else {
                hint
            }
        })
    }
}

struct BudgetedMapAccess<'a, A> {
    inner: A,
    tracker: &'a ParseBudgetTracker,
    depth: usize,
    count_entries: bool,
}

impl<'de, A> MapAccess<'de> for BudgetedMapAccess<'_, A>
where
    A: MapAccess<'de>,
{
    type Error = A::Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: DeserializeSeed<'de>,
    {
        self.inner.next_key_seed(BudgetedSeed {
            inner: seed,
            tracker: self.tracker,
            depth: self.depth,
            count_collection_item: self.count_entries,
            count_identifiers: self.count_entries,
        })
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: DeserializeSeed<'de>,
    {
        self.inner.next_value_seed(BudgetedSeed {
            inner: seed,
            tracker: self.tracker,
            depth: self.depth,
            count_collection_item: false,
            count_identifiers: false,
        })
    }

    fn size_hint(&self) -> Option<usize> {
        self.inner.size_hint().map(|hint| {
            if self.count_entries {
                hint.min(self.tracker.remaining_collection_items())
            } else {
                hint
            }
        })
    }
}

struct BudgetedEnumAccess<'a, A> {
    inner: A,
    tracker: &'a ParseBudgetTracker,
    depth: usize,
}

impl<'de, 'a, A> EnumAccess<'de> for BudgetedEnumAccess<'a, A>
where
    A: EnumAccess<'de>,
{
    type Error = A::Error;
    type Variant = BudgetedVariantAccess<'a, A::Variant>;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
    where
        V: DeserializeSeed<'de>,
    {
        let (value, variant) = self.inner.variant_seed(BudgetedSeed {
            inner: seed,
            tracker: self.tracker,
            depth: self.depth,
            count_collection_item: false,
            count_identifiers: false,
        })?;
        Ok((
            value,
            BudgetedVariantAccess {
                inner: variant,
                tracker: self.tracker,
                depth: self.depth,
            },
        ))
    }
}

struct BudgetedVariantAccess<'a, A> {
    inner: A,
    tracker: &'a ParseBudgetTracker,
    depth: usize,
}

impl<'de, A> VariantAccess<'de> for BudgetedVariantAccess<'_, A>
where
    A: VariantAccess<'de>,
{
    type Error = A::Error;

    fn unit_variant(self) -> Result<(), Self::Error> {
        self.inner.unit_variant()
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        self.inner.newtype_variant_seed(BudgetedSeed {
            inner: seed,
            tracker: self.tracker,
            depth: self.depth.saturating_add(1),
            count_collection_item: false,
            count_identifiers: false,
        })
    }

    fn tuple_variant<V>(self, len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.inner.tuple_variant(
            len,
            BudgetedVisitor {
                inner: visitor,
                tracker: self.tracker,
                depth: self.depth,
                count_strings: false,
                count_collection_items: true,
                count_identifiers: false,
            },
        )
    }

    fn struct_variant<V>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.inner.struct_variant(
            fields,
            BudgetedVisitor {
                inner: visitor,
                tracker: self.tracker,
                depth: self.depth,
                count_strings: false,
                count_collection_items: false,
                count_identifiers: false,
            },
        )
    }
}
