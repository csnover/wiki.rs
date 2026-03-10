//! Types and functions for visualisation data retrieval.

use super::{
    Node, TimeExt, expr::Result, geo::topojson, propset::RulePredicate, transform::Transform,
};
use crate::php::{DateTime, DateTimeZone};
use core::{cell::OnceCell, cmp::Ordering, fmt::Write as _};
use serde_json_borrow::Value;
use std::{borrow::Cow, collections::HashMap};

/// An extension trait for [`Value`].
pub(super) trait ValueExt<'s> {
    /// Performs loose relational comparison according to the ECMAScript 2026
    /// rules.
    fn fuzzy_cmp(&self, other: &Self) -> Option<Ordering>;
    /// Performs loose equality comparison according to the ECMAScript 2026
    /// rules.
    fn fuzzy_eq(&self, other: &Self) -> bool;
    /// Performs loose relational comparison according to the ECMAScript 2026
    /// rules, with total comparison rules for numbers.
    fn fuzzy_total_cmp(&self, other: &Self) -> Ordering;

    /// If the [`Value`] is a string, returns the associated str.
    fn as_cow(&self) -> Option<&Cow<'s, str>>;

    /// Gets a mutable property of the [`Value`].
    fn get_mut<'a>(&'a mut self, key: &str) -> Option<&'a mut Value<'s>>;

    /// Sets a property if the [`Value`] is an object, returning the old value.
    fn insert<K, V>(&mut self, key: K, value: V) -> Option<Self>
    where
        K: Into<serde_json_borrow::KeyStrType<'s>>,
        V: Into<Self>,
        Self: Sized;

    /// Converts the value into a bool, consuming it.
    fn into_bool(self) -> bool;
    /// Converts the value into an f64, consuming it.
    fn into_f64(self) -> f64;
    /// Converts the value into a string, consuming it.
    fn into_string(self) -> Cow<'s, str>;

    /// Returns true if `self` is loosely equal to `other`.
    #[inline]
    fn is_eq(&self, other: &Self) -> bool {
        self.fuzzy_eq(other)
    }
    /// Returns true if `self` is loosely greater than `other`.
    #[inline]
    fn is_gt(&self, other: &Self) -> bool {
        self.fuzzy_cmp(other).is_some_and(Ordering::is_gt)
    }
    /// Returns true if `self` is loosely greater than or equal to `other`.
    #[inline]
    fn is_gte(&self, other: &Self) -> bool {
        self.fuzzy_cmp(other).is_some_and(Ordering::is_ge)
    }
    /// Returns true if `self` is loosely not equal to `other`.
    #[inline]
    fn is_ne(&self, other: &Self) -> bool {
        !self.fuzzy_eq(other)
    }
    /// Returns true if `self` is loosely less than `other`.
    #[inline]
    fn is_lt(&self, other: &Self) -> bool {
        self.fuzzy_cmp(other).is_some_and(Ordering::is_lt)
    }
    /// Returns true if `self` is loosely less than or equal to `other`.
    #[inline]
    fn is_lte(&self, other: &Self) -> bool {
        self.fuzzy_cmp(other).is_some_and(Ordering::is_le)
    }

    /// Converts the value to a bool.
    fn to_bool(&self) -> bool;
    /// Tries to convert this value to a [`DateTime`].
    fn to_date(&self, utc: bool) -> Result<DateTime>;
    /// Converts the value to an f64.
    fn to_f64(&self) -> f64;
    /// Converts the value to a string.
    fn to_string(&self) -> Cow<'s, str>;
}

impl<'s> ValueExt<'s> for Value<'s> {
    fn as_cow(&self) -> Option<&Cow<'s, str>> {
        match self {
            Value::Str(cow) => Some(cow),
            _ => None,
        }
    }

    fn fuzzy_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (Self::Str(lhs), Self::Str(rhs)) => lhs.partial_cmp(rhs),
            (Self::Str(lhs), rhs) => lhs.partial_cmp(&ValueExt::to_string(rhs)),
            (lhs, Self::Str(rhs)) => ValueExt::to_string(lhs).partial_cmp(rhs),
            // ECMAScript 2026 §7.2.12 defines relational comparison by
            // converting `ToPrimitive` and then `ToNumber`; this *should* be
            // equivalent, since the only thing that happens differently in the
            // ES algorithm is that `ToPrimitive` converts via `Array#toString`
            // and this will cause a lexicographic comparison when the other
            // operand is a string in step 3
            (lhs, rhs) => lhs.to_f64().partial_cmp(&rhs.to_f64()),
        }
    }

    fn fuzzy_eq(&self, other: &Self) -> bool {
        // ECMAScript 2026 §7.2.13 defines equality using more complex recursive
        // steps. This “should” be equivalent
        match (self, other) {
            (Value::Null, Value::Null) => true,
            (Value::Bool(lhs), Value::Bool(rhs)) => lhs == rhs,
            (Value::Number(lhs), Value::Number(rhs)) => lhs == rhs,
            (Value::Array(lhs), Value::Array(rhs)) => core::ptr::eq(lhs, rhs),
            (Value::Object(lhs), Value::Object(rhs)) => core::ptr::eq(lhs, rhs),
            (Value::Number(n), other) | (other, Value::Number(n)) => {
                other.to_f64() == n.as_f64().unwrap_or(f64::NAN)
            }
            (lhs, rhs) => ValueExt::to_string(lhs) == ValueExt::to_string(rhs),
        }
    }

    fn fuzzy_total_cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Str(lhs), Self::Str(rhs)) => lhs.cmp(rhs),
            (Self::Str(lhs), rhs) => lhs.cmp(&ValueExt::to_string(rhs)),
            (lhs, Self::Str(rhs)) => ValueExt::to_string(lhs).cmp(rhs),
            // ECMAScript 2026 §7.2.12 defines relational comparison by
            // converting `ToPrimitive` and then `ToNumber`; this *should* be
            // equivalent, since the only thing that happens differently in the
            // ES algorithm is that `ToPrimitive` converts via `Array#toString`
            // and this will cause a lexicographic comparison when the other
            // operand is a string in step 3
            (lhs, rhs) => lhs.to_f64().total_cmp(&rhs.to_f64()),
        }
    }

    #[inline]
    fn get_mut<'a>(&'a mut self, key: &str) -> Option<&'a mut Value<'s>> {
        self.as_object_mut().and_then(|object| object.get_mut(key))
    }

    #[inline]
    fn insert<K, V>(&mut self, key: K, value: V) -> Option<Self>
    where
        K: Into<serde_json_borrow::KeyStrType<'s>>,
        V: Into<Self>,
    {
        self.as_object_mut()
            .and_then(|object| object.insert(key, value))
    }

    #[inline]
    fn into_bool(self) -> bool {
        self.to_bool()
    }

    #[inline]
    fn into_f64(self) -> f64 {
        self.to_f64()
    }

    #[inline]
    fn into_string(self) -> Cow<'s, str> {
        match self {
            Value::Str(s) => s,
            value => ValueExt::to_string(&value),
        }
    }

    fn to_bool(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(b) => *b,
            Value::Number(n) => n.as_f64() != Some(0.0),
            Value::Str(s) => !s.is_empty(),
            Value::Array(_) | Value::Object(_) => true,
        }
    }

    fn to_date(&self, utc: bool) -> Result<DateTime> {
        Ok(if let Some(n) = self.as_f64() {
            DateTime::from_f64(n, utc)
        } else {
            let date = DateTime::new(&ValueExt::to_string(self), None, None)?;
            if utc {
                date.into_offset(DateTimeZone::UTC)?
            } else {
                date.into_offset(DateTimeZone::local()?)?
            }
        })
    }

    fn to_f64(&self) -> f64 {
        match self {
            Value::Null => 0.0,
            Value::Bool(b) => f64::from(*b),
            Value::Number(n) => n.as_f64().unwrap_or(f64::NAN),
            Value::Str(s) => s.parse::<f64>().unwrap_or(f64::NAN),
            Value::Array(a) => {
                if a.is_empty() {
                    0.0
                } else if let [v] = a.as_slice() {
                    v.to_f64()
                } else {
                    f64::NAN
                }
            }
            Value::Object(_) => f64::NAN,
        }
    }

    fn to_string(&self) -> Cow<'s, str> {
        match self {
            Value::Null => Cow::Borrowed("null"),
            Value::Bool(b) => Cow::Owned(format!("{b}")),
            Value::Number(n) => Cow::Owned(format!("{n}")),
            Value::Str(s) => s.clone(),
            Value::Array(a) => {
                let mut out = String::new();
                let _ = array_to_str(&mut out, a);
                Cow::Owned(out)
            }
            Value::Object(_) => Cow::Borrowed("[object Object]"),
        }
    }
}

/// Converts a JSON array to a string using the ECMAScript algorithm.
fn array_to_str(out: &mut String, values: &[Value<'_>]) -> core::fmt::Result {
    let mut first = true;
    for value in values {
        if first {
            first = false;
        } else {
            out.push(',');
        }
        match value {
            Value::Null => {}
            Value::Bool(b) => write!(out, "{b:?}")?,
            Value::Number(n) => write!(out, "{n}")?,
            Value::Str(s) => write!(out, "{s}")?,
            Value::Array(values) => array_to_str(out, values)?,
            Value::Object(_) => write!(out, "[object Object]")?,
        }
    }
    Ok(())
}

/// Discards data from a deserializer, while keeping the known type of that data
/// available for documentation purposes.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub(super) struct IgnoredAny<T>(serde::de::IgnoredAny, core::marker::PhantomData<T>);

impl<'de, T> serde::Deserialize<'de> for IgnoredAny<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        serde::de::IgnoredAny::deserialize(deserializer).map(|s| Self(s, <_>::default()))
    }
}

/// A data set.
#[derive(Debug, serde::Deserialize)]
#[serde(bound = "'s: 'de", deny_unknown_fields)]
pub(super) struct Data<'s> {
    /// The name of the data set.
    #[serde(borrow)]
    pub name: Cow<'s, str>,
    /// The format of the data set.
    #[serde(borrow)]
    format: Option<Format<'s>>,
    /// The actual data.
    #[serde(borrow, flatten)]
    source: Source<'s>,
    /// The list of streaming operators to insert, remove, & toggle data values,
    /// post-data-transform.
    #[serde(borrow, default)]
    modify: Vec<Modify<'s>>,
    /// The list of data transformations.
    #[serde(borrow, default)]
    transform: Vec<Transform<'s>>,
    /// Transformed data cache.
    #[serde(skip)]
    transform_cache: OnceCell<Vec<Value<'s>>>,
}

impl<'s> Data<'s> {
    /// Retrieves the raw source value from this data definition.
    pub fn source<'b>(&'b self, node: &'b Node<'s, '_>) -> Option<Cow<'b, [Value<'s>]>> {
        match &self.source {
            Source::Values(data) => Some(Cow::Owned(if let Some(format) = &self.format {
                format.format(data)
            } else {
                format_json(data, &FormatTypes::Auto, None)
            })),
            Source::Named(name) => node
                .spec
                .data(name)
                .map(|data| Cow::Borrowed(data.values(node))),
            Source::Url(_) => {
                log::warn!("Remote data fetching is not supported");
                None
            }
        }
    }

    /// Gets the list of transformed values from the data set.
    pub fn values<'b>(&'b self, node: &'b Node<'s, '_>) -> &'b [Value<'s>] {
        // TODO: It is technically not possible to always reuse the input values
        // because every object is supposed to be given a generated _id property
        // and this property is required in edge cases where e.g. a data set is
        // filtered into another data set and now it wants to match objects
        // between the two data sets. The only place this happens in the example
        // data is in interaction (which is not a thing) and in the Force
        // transformer (where it is not actually needed for the one test case
        // that Vega has). So if you are reading this, because you think there
        // should be _id, sorry!
        if let Some(cached) = self.transform_cache.get() {
            return cached;
        } else if let Source::Named(name) = &self.source
            && self.transform.is_empty()
            && self.modify.is_empty()
            && self.format.as_ref().is_none_or(Format::is_auto_format)
        {
            return node.data_values(name).unwrap_or_default();
        }

        self.transform_cache.get_or_init(|| {
            let mut data = self.source(node).unwrap_or_default();
            for transform in &self.transform {
                data = Cow::Owned(transform.transform(node, data));
            }
            for _ in &self.modify {
                todo!()
            }
            data.into_owned()
        })
    }
}

/// A streaming data operator.
#[expect(dead_code, reason = "this is used only for dynamic runtime")]
#[derive(Debug, serde::Deserialize)]
#[serde(tag = "type")]
enum Modify<'s> {
    /// Insert a value using the given mutation.
    #[serde(borrow)]
    Insert(Mutation<'s>),
    /// Remove a value using the given mutation.
    #[serde(borrow)]
    Remove(Mutation<'s>),
    /// Toggle a value using the given mutation.
    #[serde(borrow)]
    Toggle(Mutation<'s>),
    /// Insert or update a value using the given mutation.
    #[serde(borrow)]
    Upsert(Mutation<'s>),
    /// Clear inserted values.
    #[serde(borrow)]
    Clear(RulePredicate<'s>),
}

/// A data mutation definition.
#[expect(dead_code, reason = "this is used only for dynamic runtime")]
#[derive(Debug, serde::Deserialize)]
struct Mutation<'s> {
    /// The field to mutate.
    #[serde(borrow)]
    field: Cow<'s, str>,
    /// The source signal which triggers the mutation and returns the value
    /// for the field.
    #[serde(borrow)]
    signal: Cow<'s, str>,
}

/// A data set source.
#[derive(Debug, serde::Deserialize)]
#[serde(bound = "'s: 'de", rename_all = "lowercase")]
enum Source<'s> {
    /// Inline JSON data.
    #[serde(borrow)]
    Values(Value<'s>),
    /// The name of another data set.
    #[serde(borrow, rename = "source")]
    Named(Cow<'s, str>),
    /// A URL to a data file.
    #[serde(borrow)]
    Url(IgnoredAny<Cow<'s, str>>),
}

/// A TopoJSON data source specification.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum TopoJson<'s> {
    /// The name of the TopoJSON object set to convert to a GeoJSON feature
    /// collection.
    #[serde(borrow)]
    Feature(Cow<'s, str>),
    /// The name of the TopoJSON object set to convert to a mesh.
    #[serde(borrow)]
    Mesh(Cow<'s, str>),
}

impl<'s> TopoJson<'s> {
    /// Converts TopoJSON data to GeoJSON data.
    fn format(&self, data: &Value<'s>) -> Vec<Value<'s>> {
        match self {
            TopoJson::Feature(key) => {
                let feature = data.get("objects").and_then(|objects| objects.get(key));
                topojson::features(data, feature.unwrap())
            }
            TopoJson::Mesh(key) => {
                let mesh = data.get("objects").and_then(|objects| objects.get(key));
                vec![topojson::mesh(data, mesh.unwrap())]
            }
        }
    }
}

/// Hierarchical JSON.
#[derive(Debug, serde::Deserialize)]
struct TreeJson<'s> {
    /// The JSON property that contains an array of children nodes for each
    /// intermediate node. This parameter defaults to "children".
    #[serde(borrow, default = "TreeJson::default_children")]
    children: Cow<'s, str>,
    /// The JSON property to use to point to the parent node of an
    /// intermediate node.
    #[serde(borrow, default = "TreeJson::default_parent")]
    parent: Cow<'s, str>,
}

impl<'s> TreeJson<'s> {
    /// The default value for [`Self::TreeJson`]`::children`.
    const fn default_children() -> Cow<'static, str> {
        Cow::Borrowed("children")
    }

    /// The default value for [`Self::TreeJson`]`::parent`.
    const fn default_parent() -> Cow<'static, str> {
        Cow::Borrowed("parent")
    }

    /// Converts tree data to a flat node list.
    fn format(&self, root: &Value<'s>) -> Vec<Value<'s>> {
        let mut table = vec![];
        let root = root.clone();
        self.visit(&mut table, root, None);
        // Vega put a root property on the array, but we know the root is always
        // the first item, and since arrays cannot have extra properties in Rust
        // just don’t bother making things weird and complicated
        table
    }

    /// Flattens a data node by adding parent indexes to each node and
    /// recursively moving all children into the flat list, replacing child
    /// nodes with indexes into the flat list.
    ///
    /// Vega only collected references to the child nodes and added parent
    /// references to each child node, but this is impossible to do without
    /// either `Rc` or garbage collection, which remains a bridge too far for
    /// this for now. Since this format seems to be primarily intended for use
    /// by specific transforms like Treemap, hopefully this is OK enough since
    /// the transforms can just be written to understand that an index of
    /// children is a lookup.
    fn visit(&self, table: &mut Vec<Value<'s>>, mut node: Value<'s>, parent: Option<usize>) {
        node.insert(
            self.parent.clone(),
            parent.map_or(Value::Null, |parent| (parent as u64).into()),
        );

        let parent = table.len();

        // To avoid double-borrows, push a dummy into the table, then replace it
        // later
        table.push(<_>::default());

        if let Some(children) = node.get_mut(&self.children).and_then(Value::as_array_mut) {
            for child in children {
                let child = core::mem::replace(child, Value::Number((table.len() as u64).into()));
                self.visit(table, child, Some(parent));
            }
        }

        table[parent] = node;
    }
}

/// A data type for a field.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum FieldType<'s> {
    /// Interpret the field as a bool.
    Boolean,
    /// Interpret the field as a D3 time string.
    Date,
    /// Interpret the field as a floating-point number.
    #[serde(alias = "integer")]
    Number,
    /// Interpret the field as a string.
    String,
    /// Interpret the field as some other type. In practice, this should only
    /// ever be a "date:'%...'" explicit date format string.
    #[serde(borrow, untagged)]
    Other(Cow<'s, str>),
}

impl<'s> FieldType<'s> {
    /// Heuristically converts a string into a millisecond-precision Unix
    /// timestamp.
    ///
    /// ECMAScript 2026 §21.4.3.2: “If the String does not conform to [the
    /// simplified ISO 8601 format from §21.4.1.32], the function may fall back
    /// to any implementation-specific heuristics or implementation-specific
    /// date formats.”
    #[inline]
    fn guess_date(value: &str) -> f64 {
        DateTime::new(value, None, None).map_or(f64::NAN, TimeExt::into_f64)
    }

    /// Maps the type of the given value into the type defined by `self`.
    fn map(&self, value: Value<'s>) -> Value<'s> {
        match self {
            Self::Boolean => Value::Bool(value.into_bool()),
            // TODO: I guess it should be possible to actually hold this as a
            // proper Date type, but since that is not one of the valid types of
            // JSON, `Value` would need to be replaced with some super-type
            Self::Date => {
                let as_str = value.into_string();
                Value::Number(Self::guess_date(&as_str).into())
            }
            Self::Number => Value::Number(value.into_f64().into()),
            Self::String => Value::Str(value.into_string()),
            Self::Other(s) => {
                if let Some(p) = s.strip_prefix("date:") {
                    todo!("explicit date format parsing '{p}'")
                } else {
                    todo!("unknown mapped type name")
                }
            }
        }
    }
}

/// A map from a data object field name to its explicitly specified type.
type FieldTypes<'s> = HashMap<Cow<'s, str>, FieldType<'s>>;

/// A data format specification.
#[derive(Debug, serde::Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Format<'s> {
    /// JavaScript object notation.
    Json {
        /// The expected data types for the fields of the data.
        #[serde(borrow, default)]
        parse: FormatTypes<'s>,
        /// An object path to use as the root of the data.
        #[serde(borrow, default)]
        property: Option<Cow<'s, str>>,
    },
    /// Comma-separated values.
    Csv {
        /// The expected data types for the fields of the data.
        #[serde(borrow, rename = "parse")]
        _parse: FormatTypes<'s>,
    },
    /// Tab-separated values.
    Tsv {
        /// The expected data types for the fields of the data.
        #[serde(borrow, rename = "parse")]
        _parse: FormatTypes<'s>,
    },
    /// Topographical JSON.
    #[serde(borrow)]
    TopoJson(TopoJson<'s>),
    /// Hierarchical JSON.
    #[serde(borrow)]
    TreeJson(TreeJson<'s>),
}

impl<'s> Format<'s> {
    /// Converts data to the expected types according to the formatting data.
    fn format(&self, data: &Value<'s>) -> Vec<Value<'s>> {
        match self {
            Format::Json { parse, property } => format_json(data, parse, property.as_deref()),
            Format::TopoJson(topojson) => topojson.format(data),
            Format::TreeJson(treejson) => treejson.format(data),
            _ => todo!(),
        }
    }

    /// Returns true if this format passes through data as-is.
    fn is_auto_format(&self) -> bool {
        matches!(
            self,
            Self::Json {
                parse: FormatTypes::Auto,
                property: None
            }
        )
    }
}

/// A data type specifier.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum FormatTypes<'s> {
    /// Heuristically determine the types of data fields.
    #[default]
    Auto,
    /// Explicitly define the types of data fields.
    #[serde(borrow, untagged)]
    Explicit(FieldTypes<'s>),
}

/// Converts the field types of a JSON data source.
fn format_json<'s>(
    data: &Value<'s>,
    parse: &FormatTypes<'s>,
    property: Option<&str>,
) -> Vec<Value<'s>> {
    let data = if let Some(property) = property {
        Cow::Borrowed(
            get_nested_value(data, property)
                .and_then(Value::as_array)
                .unwrap_or_default(),
        )
    } else if let Value::Array(data) = data {
        Cow::Borrowed(data.as_slice())
    } else if !data.is_null() {
        // TODO: Verify what is actually supposed to happen in this case.
        Cow::Owned(vec![data.clone()])
    } else {
        <_>::default()
    };

    match parse {
        FormatTypes::Auto => data.iter().map(value_to_map(&<_>::default())).collect(),
        FormatTypes::Explicit(types) => data.iter().map(value_to_map(types)).collect(),
    }
}

/// Gets a value from a data object using a nested path key.
pub(super) fn get_nested_value<'b, 's>(item: &'b Value<'s>, key: &str) -> Option<&'b Value<'s>> {
    if key.contains(['[']) {
        todo!("nested property array expression");
    }
    if key.contains("\\.") {
        todo!("nested property escape sequence");
    }

    key.split('.')
        .try_fold(item, |item, part| item.as_object()?.get(part))
        .filter(|item| !item.is_null())
}

/// Serialisation utilities.
mod ser {
    /// Serialises a unit enum variant into a string reference.
    pub(super) struct StringSerializer;
    /// The error type for [`StringSerializer`].
    pub(super) struct Error;
    impl core::fmt::Debug for Error {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str("error")
        }
    }
    impl core::fmt::Display for Error {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str("error")
        }
    }
    impl core::error::Error for Error {}
    impl serde::ser::Error for Error {
        fn custom<T>(_: T) -> Self
        where
            T: core::fmt::Display,
        {
            Self
        }
    }
    impl serde::Serializer for StringSerializer {
        type Ok = &'static str;
        type Error = Error;
        type SerializeSeq = serde::ser::Impossible<Self::Ok, Self::Error>;
        type SerializeTuple = serde::ser::Impossible<Self::Ok, Self::Error>;
        type SerializeTupleStruct = serde::ser::Impossible<Self::Ok, Self::Error>;
        type SerializeTupleVariant = serde::ser::Impossible<Self::Ok, Self::Error>;
        type SerializeMap = serde::ser::Impossible<Self::Ok, Self::Error>;
        type SerializeStruct = serde::ser::Impossible<Self::Ok, Self::Error>;
        type SerializeStructVariant = serde::ser::Impossible<Self::Ok, Self::Error>;

        fn serialize_bool(self, _: bool) -> Result<Self::Ok, Self::Error> {
            Err(Error)
        }

        fn serialize_i8(self, _: i8) -> Result<Self::Ok, Self::Error> {
            Err(Error)
        }

        fn serialize_i16(self, _: i16) -> Result<Self::Ok, Self::Error> {
            Err(Error)
        }

        fn serialize_i32(self, _: i32) -> Result<Self::Ok, Self::Error> {
            Err(Error)
        }

        fn serialize_i64(self, _: i64) -> Result<Self::Ok, Self::Error> {
            Err(Error)
        }

        fn serialize_u8(self, _: u8) -> Result<Self::Ok, Self::Error> {
            Err(Error)
        }

        fn serialize_u16(self, _: u16) -> Result<Self::Ok, Self::Error> {
            Err(Error)
        }

        fn serialize_u32(self, _: u32) -> Result<Self::Ok, Self::Error> {
            Err(Error)
        }

        fn serialize_u64(self, _: u64) -> Result<Self::Ok, Self::Error> {
            Err(Error)
        }

        fn serialize_f32(self, _: f32) -> Result<Self::Ok, Self::Error> {
            Err(Error)
        }

        fn serialize_f64(self, _: f64) -> Result<Self::Ok, Self::Error> {
            Err(Error)
        }

        fn serialize_char(self, _: char) -> Result<Self::Ok, Self::Error> {
            Err(Error)
        }

        fn serialize_str(self, _: &str) -> Result<Self::Ok, Self::Error> {
            Err(Error)
        }

        fn serialize_bytes(self, _: &[u8]) -> Result<Self::Ok, Self::Error> {
            Err(Error)
        }

        fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
            Err(Error)
        }

        fn serialize_some<T>(self, _: &T) -> Result<Self::Ok, Self::Error>
        where
            T: ?Sized + serde::Serialize,
        {
            Err(Error)
        }

        fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
            Err(Error)
        }

        fn serialize_unit_struct(self, _: &'static str) -> Result<Self::Ok, Self::Error> {
            Err(Error)
        }

        fn serialize_unit_variant(
            self,
            _: &'static str,
            _: u32,
            variant: &'static str,
        ) -> Result<Self::Ok, Self::Error> {
            Ok(variant)
        }

        fn serialize_newtype_struct<T>(
            self,
            _: &'static str,
            _: &T,
        ) -> Result<Self::Ok, Self::Error>
        where
            T: ?Sized + serde::Serialize,
        {
            Err(Error)
        }

        fn serialize_newtype_variant<T>(
            self,
            _: &'static str,
            _: u32,
            _: &'static str,
            _: &T,
        ) -> Result<Self::Ok, Self::Error>
        where
            T: ?Sized + serde::Serialize,
        {
            Err(Error)
        }

        fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
            Err(Error)
        }

        fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, Self::Error> {
            Err(Error)
        }

        fn serialize_tuple_struct(
            self,
            _: &'static str,
            _: usize,
        ) -> Result<Self::SerializeTupleStruct, Self::Error> {
            Err(Error)
        }

        fn serialize_tuple_variant(
            self,
            _: &'static str,
            _: u32,
            _: &'static str,
            _: usize,
        ) -> Result<Self::SerializeTupleVariant, Self::Error> {
            Err(Error)
        }

        fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
            Err(Error)
        }

        fn serialize_struct(
            self,
            _: &'static str,
            _: usize,
        ) -> Result<Self::SerializeStruct, Self::Error> {
            Err(Error)
        }

        fn serialize_struct_variant(
            self,
            _: &'static str,
            _: u32,
            _: &'static str,
            _: usize,
        ) -> Result<Self::SerializeStructVariant, Self::Error> {
            Err(Error)
        }
    }
}

/// Use serde’s auto-generated variant names to deserialise a unit enum from a
/// [`Value`].
pub(super) fn value_to_unit_enum<'de, T: serde::Deserialize<'de> + Default>(
    value: &Value<'_>,
) -> T {
    let de = serde::de::IntoDeserializer::<serde::de::value::Error>::into_deserializer(
        ValueExt::to_string(value),
    );
    serde::Deserialize::deserialize(de).unwrap_or(T::default())
}

/// Use serde’s auto-generated variant names to serialise a unit enum into a
/// [`Value`].
pub(super) fn unit_enum_to_value<T: serde::Serialize>(value: T) -> Value<'static> {
    Value::Str(Cow::Borrowed(unit_enum_to_str(value)))
}

/// Use serde’s auto-generated variant names to serialise a unit enum into a
/// string reference.
pub(super) fn unit_enum_to_str<T: serde::Serialize>(value: T) -> &'static str {
    value.serialize(ser::StringSerializer).unwrap()
}

/// Converts a JSON value of any variant to a data object.
fn value_to_map<'s>(types: &FieldTypes<'s>) -> impl FnMut(&Value<'s>) -> Value<'s> {
    |value| {
        if let Value::Object(items) = value {
            items
                .as_vec()
                .iter()
                .map(|(key, value)| {
                    let key = &key.0;
                    let value = value.clone();
                    let value = if let Some(kind) = types.get(key) {
                        kind.map(value)
                    } else {
                        value
                    };

                    (key.clone(), value)
                })
                .collect()
        } else {
            // TODO: Check to see what Vega does in this edge case, if it uses
            // the "data" key on the format object or not
            Value::from([("data", value.clone())])
        }
    }
}

/// A deserialisable [`Value`] restricted to being either a string or a number.
#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum NumberOrString<'s> {
    /// A number.
    Number(f64),
    /// A string.
    #[serde(borrow)]
    String(Cow<'s, str>),
}

/// A serde helper function for [`NumberOrString`] field representations.
pub(super) fn scalar<'de: 's, 's, D>(d: D) -> Result<Value<'s>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = <NumberOrString<'s> as serde::Deserialize>::deserialize(d)?;
    Ok(match value {
        NumberOrString::Number(n) => Value::Number(n.into()),
        NumberOrString::String(s) => Value::Str(s),
    })
}

/// A serde helper function for [`Option<NumberOrString>`] field representations.
pub(super) fn option_scalar<'de: 's, 's, D>(d: D) -> Result<Option<Value<'s>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = <Option<NumberOrString<'s>> as serde::Deserialize>::deserialize(d)?;
    Ok(value.map(|value| match value {
        NumberOrString::Number(n) => Value::Number(n.into()),
        NumberOrString::String(s) => Value::Str(s),
    }))
}

/// A serde helper function for [`Vec<NumberOrString>`] field representations.
pub(super) fn vec_scalar<'de: 's, 's, D>(d: D) -> Result<Vec<Value<'s>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = <Vec<NumberOrString<'s>> as serde::Deserialize>::deserialize(d)?;
    Ok(value
        .into_iter()
        .map(|value| match value {
            NumberOrString::Number(n) => Value::Number(n.into()),
            NumberOrString::String(s) => Value::Str(s),
        })
        .collect())
}
