//! Types and functions for conditional application of properties.

use super::{
    Node,
    data::{ValueExt, get_nested_value},
};
use serde_json_borrow::Value;
use std::borrow::Cow;

/// A predicate call.
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(from = "CallRepr<'s>")]
pub(super) struct Call<'s> {
    /// The name of the predicate to invoke.
    #[serde(borrow)]
    name: Cow<'s, str>,
    /// The arguments to the predicate.
    #[serde(borrow, flatten)]
    args: Value<'s>,
}

impl<'s> Call<'s> {
    /// Evaluates a predicate.
    pub fn eval(&self, node: &Node<'s, '_>) -> bool {
        node.spec
            .predicates
            .iter()
            .find(|callee| callee.name == self.name)
            .is_some_and(|callee| callee.eval(&self.args, node))
    }
}

/// A serialised predicate call.
#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum CallRepr<'s> {
    /// A predicate call with no arguments.
    #[serde(borrow)]
    Call(Cow<'s, str>),
    /// A predicate call with arguments.
    WithArgs {
        /// The name of the callee.
        #[serde(borrow)]
        name: Cow<'s, str>,
        /// The arguments to pass to the callee.
        #[serde(borrow, flatten)]
        args: Value<'s>,
    },
}

impl<'s> From<CallRepr<'s>> for Call<'s> {
    fn from(value: CallRepr<'s>) -> Self {
        let (name, args) = match value {
            CallRepr::Call(name) => (name, <_>::default()),
            CallRepr::WithArgs { name, args } => (name, args),
        };
        Self { name, args }
    }
}

/// A predicate definition.
#[derive(Debug, serde::Deserialize)]
#[serde(bound = "'s: 'de")]
pub(super) struct Definition<'s> {
    /// The name of the predicate.
    #[serde(borrow)]
    name: Cow<'s, str>,
    /// The operator.
    #[serde(borrow, rename = "type")]
    op: Operator<'s>,
    /// The operands.
    #[serde(borrow)]
    operands: Vec<Operand<'s>>,
}

impl<'s> Definition<'s> {
    /// Evaluates a predicate using the given arguments.
    fn eval(&self, args: &Value<'s>, node: &Node<'s, '_>) -> bool {
        match &self.op {
            Operator::And => self
                .operands
                .iter()
                .all(|op| op.eval(args, node).is_some_and(|result| result.to_bool())),
            Operator::Or => self
                .operands
                .iter()
                .any(|op| op.eval(args, node).is_some_and(|result| result.to_bool())),
            Operator::Eq
            | Operator::Ne
            | Operator::Gt
            | Operator::Gte
            | Operator::Lt
            | Operator::Lte => {
                let lhs = self
                    .operands
                    .first()
                    .and_then(|lhs| lhs.eval(args, node))
                    .unwrap_or_default();
                let rhs = self
                    .operands
                    .get(1)
                    .and_then(|rhs| rhs.eval(args, node))
                    .unwrap_or_default();
                match self.op {
                    Operator::Eq => lhs.is_eq(&rhs),
                    Operator::Ne => lhs.is_ne(&rhs),
                    Operator::Gt => lhs.is_gt(&rhs),
                    Operator::Gte => lhs.is_gte(&rhs),
                    Operator::Lt => lhs.is_lt(&rhs),
                    Operator::Lte => lhs.is_lte(&rhs),
                    _ => unreachable!(),
                }
            }
            Operator::In { item, set } => {
                let value = item.eval(args, node);
                match set {
                    SourceSet::Data { data, field } => {
                        // This would be `[...].indexOf(null)`
                        // or `[].indexOf(value)`
                        let (Some(value), Some(data)) = (value, node.data_values(data)) else {
                            return false;
                        };
                        data.iter().any(|item| {
                            get_nested_value(item, field)
                                .is_some_and(|candidate| value.fuzzy_eq(candidate))
                        })
                    }
                    SourceSet::Range { range, scale } => {
                        // This would be `start <= null && null <= end`
                        let mut start = range
                            .first()
                            .and_then(|value| value.eval(args, node))
                            .map_or(0.0, |v| v.to_f64());
                        let mut end = range
                            .get(1)
                            .and_then(|value| value.eval(args, node))
                            .map_or(0.0, |v| v.to_f64());
                        if scale.is_some() {
                            todo!("range scale");
                        }
                        if start > end {
                            core::mem::swap(&mut start, &mut end);
                        }
                        (start..=end).contains(&value.map_or(0.0, |v| v.to_f64()))
                    }
                }
            }
        }
    }
}

/// A predicate operand.
#[derive(Debug, serde::Deserialize)]
#[serde(bound = "'s: 'de")]
enum Operand<'s> {
    /// A fixed value.
    #[serde(borrow)]
    Value(Value<'s>),
    /// A named argument.
    #[serde(borrow)]
    Arg(Cow<'s, str>),
    /// The result of another predicate call.
    #[serde(borrow)]
    Predicate(Call<'s>),
}

impl<'s> Operand<'s> {
    /// Evaluates an operand using the given arguments.
    fn eval<'b>(&'b self, args: &'b Value<'s>, node: &Node<'s, '_>) -> Option<Cow<'b, Value<'s>>> {
        match self {
            Operand::Value(value) => Some(Cow::Borrowed(value)),
            Operand::Arg(name) => get_nested_value(args, name).map(Cow::Borrowed),
            Operand::Predicate(call) => Some(Cow::Owned(Value::Bool(call.eval(node)))),
        }
    }
}

/// A predicate operator.
#[derive(Debug, serde::Deserialize)]
#[serde(bound = "'s: 'de")]
enum Operator<'s> {
    /// Return true if all operands are true.
    #[serde(alias = "&&")]
    And,
    /// Return true if any operand is true.
    #[serde(alias = "||")]
    Or,
    /// Return true if `lhs == rhs`.
    #[serde(rename = "==")]
    Eq,
    /// Return true if `lhs != rhs`.
    #[serde(rename = "!=")]
    Ne,
    /// Return true if `lhs > rhs`.
    #[serde(rename = ">")]
    Gt,
    /// Return true if `lhs >= rhs`.
    #[serde(rename = ">=")]
    Gte,
    /// Return true if `lhs < rhs`.
    #[serde(rename = "<")]
    Lt,
    /// Return true if `lhs <= rhs`.
    #[serde(rename = "<=")]
    Lte,
    /// Return true if the given item is in the given set.
    #[serde(rename = "in")]
    In {
        /// The item to find.
        #[serde(borrow)]
        item: Operand<'s>,
        /// The data set to search.
        #[serde(borrow, flatten)]
        set: SourceSet<'s>,
    },
}

/// A reference to a scale.
#[allow(dead_code)]
#[derive(Debug, serde::Deserialize)]
#[serde(untagged, deny_unknown_fields)]
enum ScopedScaleRef<'s> {
    /// The name of the scale.
    #[serde(borrow)]
    Short(Cow<'s, str>),
    /// Use a
    Long {
        /// If true, use an inverse mapping from the scale output to input.
        #[serde(default)]
        invert: bool,
        /// The name of the scale.
        #[serde(borrow)]
        name: Cow<'s, str>,
        /// The name of the group mark containing the scale. If unspecified,
        /// the root scale set will be searched.
        #[serde(borrow, default)]
        scope: Option<Cow<'s, str>>,
    },
}

/// A dynamic variable.
#[allow(dead_code)]
#[derive(Debug, serde::Deserialize)]
#[serde(bound = "'s: 'de")]
pub(super) struct Signal<'s> {
    /// The expression that evaluates to the signal value.
    #[serde(borrow, default)]
    expr: Option<Cow<'s, str>>,
    /// The initial signal value.
    #[serde(borrow, default)]
    init: Option<Value<'s>>,
    /// The name of the signal.
    #[serde(borrow)]
    name: Cow<'s, str>,
    /// The scale to apply to the signal value.
    #[serde(borrow, default)]
    scale: Option<ScopedScaleRef<'s>>,
    /// Optional additional event streams to use as signal sources.
    #[serde(borrow, default)]
    streams: Option<Vec<Stream<'s>>>,
    /// If true, a signal will always send an update, even if the new value is
    /// the same as the old value.
    #[serde(default)]
    verbose: bool,
}

/// The source data for an in-set operation.
#[derive(Debug, serde::Deserialize)]
#[serde(bound = "'s: 'de", untagged)]
enum SourceSet<'s> {
    /// Use a data set.
    Data {
        /// The name of the data set to search.
        #[serde(borrow)]
        data: Cow<'s, str>,
        /// The name of the field to search in the data set.
        #[serde(borrow)]
        field: Cow<'s, str>,
    },
    /// Use a range.
    Range {
        /// The range.
        #[serde(borrow)]
        range: Vec<Operand<'s>>,
        /// A scale transform to apply to the range.
        #[serde(borrow, default)]
        scale: Option<ScopedScaleRef<'s>>,
    },
}

/// An event stream.
#[allow(dead_code)]
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Stream<'s> {
    /// The expression that evaluates to the signal value.
    #[serde(borrow)]
    pub expr: Cow<'s, str>,
    /// The event selector string.
    #[serde(borrow, rename = "type")]
    pub kind: Cow<'s, str>,
    /// The scale to apply to the expression result.
    #[serde(borrow, default)]
    pub scale: Option<ScopedScaleRef<'s>>,
}
