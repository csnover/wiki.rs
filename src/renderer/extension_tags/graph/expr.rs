//! Vega expression language.
//!
//! Current limitations:
//!
//! 1. Because Vega simply used runtime eval instead of implementing a sandboxed
//!    VM, much of the behaviour and standard library of ECMAScript is
//!    technically required, even though the grammar of the Vega language is
//!    limited. (This is why Vega has problems with code injection
//!    vulnerabilities; it cannot function with a CSP which forbids eval because
//!    the whole thing is basically a DIY JIT, and it does not perform miracles
//!    to stop user code from chaining its way to the Function constructor.)
//!    This is not fully realised in this implementation. Most critically, this
//!    implementation currently has no garbage collector, so objects are cloned
//!    excessively, and as a result, object-object equality never works. This is
//!    not an insurmountable problem, but this is a toy project, so it does not
//!    matter unless it matters to you, personally, dear reader… in which case
//!    it is probably time for you to break out `gc_arena` and get to coding!
//!    The lack of any standard library prototypes (or prototypes at all,
//!    really) also means that any code that tries to call a method on a value
//!    instead of using one of the Vega expression functions will fail.
//!
//! 2. Because the Vega spec is deserialised from JSON, it is possible that
//!    escape sequences cause the input code string to be `Cow::Owned` and owned
//!    by the `Transform` or `RulePredicate` object instead of `Cow::Borrowed`
//!    from the original JSON source. This means that the lifetime `'s` and
//!    lifetime `'code` can be different, and the expression engine might return
//!    strings from either one. To work around this, strings from code which can
//!    end up in the expression result (i.e. object keys and string literals)
//!    are always parsed into owned strings with `'static` lifetime.

// SPDX-License-Identifier: BSD-3-clause
// Adapted from Vega 2 by Trifacta, Inc., Univerity of Washington Interactive
// Data Lab

use super::{Node as GraphNode, TimeExt as _, data::ValueExt};
use crate::{
    common::CowExt as _,
    php::{DateTime, DateTimeError, DateTimeZone, floatval, intval, strtr},
};
use core::{cmp::Ordering, fmt::Write as _};
use either::Either;
use rand::Rng as _;
use regex::{Regex, RegexBuilder};
use serde_json_borrow::Value;
use std::{borrow::Cow, collections::HashMap};
use unicode_general_category::{
    GeneralCategory::{
        ConnectorPunctuation, DecimalNumber, LetterNumber, LowercaseLetter, ModifierLetter,
        NonspacingMark, OtherLetter, SpacingMark, TitlecaseLetter, UppercaseLetter,
    },
    get_general_category,
};

/// The result type for expression evaluation.
pub(super) type Result<T, E = Error> = core::result::Result<T, E>;

/// An expression error.
#[derive(Debug, thiserror::Error)]
pub(super) enum Error {
    /// What is time, anyway?
    #[error(transparent)]
    Date(#[from] DateTimeError),
    /// The expression was either too much JS, or not enough JS.
    #[error(transparent)]
    Parse(#[from] peg::error::ParseError<peg::str::LineCol>),
    /// The regular expression engine did not like that very much.
    #[error(transparent)]
    Regex(#[from] regex::Error),
    /// A required argument was missing from a host function call.
    #[error("missing required argument")]
    RequiredArg,
}

/// An expression abstract syntax tree.
#[derive(Clone, Debug)]
pub(super) struct Ast<'code> {
    /// The root expression.
    root: Node<'code>,
}

impl<'code> Ast<'code> {
    /// Creates a new AST from the given code.
    #[inline]
    pub fn new(code: &'code str) -> Result<Self> {
        let root = expr::expr(code)?;
        Ok(Self { root })
    }

    /// Evaluates the expression with the given graph node.
    #[inline]
    pub fn eval<'s>(&self, node: &GraphNode<'s, '_>) -> Result<Value<'s>> {
        self.root.eval(node).map(VmValue::unwrap_value)
    }
}

peg::parser! { grammar expr() for str {
  /// A simple expression which resolves to a value.
  pub rule expr() -> Node<'input>
  = expr:cond_expr() !","
  { expr }

  /// A binary or conditional expression.
  ///
  /// ```js
  /// -x.y(z)[a].b || c.d["e"] ? 1.0 : false
  /// ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  /// -x.y(z)[a].b || c.d["e"] ? 1.0 : false
  /// ^^^^^^^^^^^^^^^^^^^^^^^^
  /// ```
  rule cond_expr() -> Node<'input>
  = test:binary_expr()
    ternary:(_ "?" _ c:cond_expr() _ ":" _ a:cond_expr() { (c, a) })?
  {
      if let Some((consequent, alternate)) = ternary {
          let test = Box::new(test);
          let consequent = Box::new(consequent);
          let alternate = Box::new(alternate);
          Node::CondExpr { test, consequent, alternate }
      } else {
          test
      }
  }

  /// A binary expression.
  ///
  /// ```js
  /// -x.y(z)[a].b || c.d["e"] ? 1.0 : false
  /// ^^^^^^^^^^^^^^^^^^^^^^^^
  /// ```
  rule binary_expr() -> Node<'input>
  = precedence!{
    a:(@) _ "||" _ b:@
    { Node::new_logical(LogicalOp::Or, a, b) }
    --
    a:(@) _ "&&" _ b:@
    { Node::new_logical(LogicalOp::And, a, b) }
    --
    a:(@) _ "|" _ b:@
    { Node::new_binary(BinaryOp::BitOr, a, b) }
    --
    a:(@) _ "^" _ b:@
    { Node::new_binary(BinaryOp::BitXor, a, b) }
    --
    a:(@) _ "&" _ b:@
    { Node::new_binary(BinaryOp::BitAnd, a, b) }
    --
    a:(@) _ "===" _ b:@
    { Node::new_binary(BinaryOp::StrictEq, a, b) }
    a:(@) _ "==" _ b:@
    { Node::new_binary(BinaryOp::Eq, a, b) }
    a:(@) _ "!==" _ b:@
    { Node::new_binary(BinaryOp::StrictNe, a, b) }
    a:(@) _ "!=" _ b:@
    { Node::new_binary(BinaryOp::Ne, a, b) }
    --
    a:(@) _ "instanceof" _ b:@
    { Node::new_binary(BinaryOp::InstanceOf, a, b) }
    a:(@) _ "in" _ b:@
    { Node::new_binary(BinaryOp::In, a, b) }
    a:(@) _ "<=" _ b:@
    { Node::new_binary(BinaryOp::Lte, a, b) }
    a:(@) _ "<" _ b:@
    { Node::new_binary(BinaryOp::Lt, a, b) }
    a:(@) _ ">=" _ b:@
    { Node::new_binary(BinaryOp::Gte, a, b) }
    a:(@) _ ">" _ b:@
    { Node::new_binary(BinaryOp::Gt, a, b) }
    --
    a:(@) _ ">>>" _ b:@
    { Node::new_binary(BinaryOp::Ushr, a, b) }
    a:(@) _ "<<" _ b:@
    { Node::new_binary(BinaryOp::Shl, a, b) }
    a:(@) _ ">>" _ b:@
    { Node::new_binary(BinaryOp::Sshr, a, b) }
    --
    a:(@) _ "+" _ b:@
    { Node::new_binary(BinaryOp::Add, a, b) }
    a:(@) _ "-" _ b:@
    { Node::new_binary(BinaryOp::Sub, a, b) }
    --
    a:(@) _ "*" _ b:@
    { Node::new_binary(BinaryOp::Mul, a, b) }
    a:(@) _ "/" _ b:@
    { Node::new_binary(BinaryOp::Div, a, b) }
    a:(@) _ "%" _ b:@
    { Node::new_binary(BinaryOp::Mod, a, b) }
    --
    expr:unary_expr() { expr }
  }

  /// A unary expression.
  ///
  /// ```js
  /// -x.y(z)[a].b || c.d["e"] ? 1.0 : false
  /// ^^^^^^^^^^^^    ^^^^^^^^
  /// ```
  rule unary_expr() -> Node<'input>
  = operator:unary_op() _ argument:unary_expr()
  { Node::UnaryExpr { operator, argument: Box::new(argument) } }
  / !forbidden_unary() expr:lhs_expr() !(space()* ("++" / "--"))
  { expr }

  /// A unary operator.
  rule unary_op() -> UnaryOp
  = "+" { UnaryOp::Num }
  / "-" { UnaryOp::Neg }
  / "~" { UnaryOp::BitNot }
  / "!" { UnaryOp::LogicalNot }

  /// Keyword-based unary operators forbidden by the Vega parser.
  rule forbidden_unary()
  = k:keyword()
  {?
      if matches!(k, Ident::Keyword("delete" | "void" | "typeof")) {
          Ok(())
      } else {
          Err("forbidden keyword")
      }
  }

  /// A left-hand-side expression.
  ///
  /// ```js
  /// -x.y(z)[a].b || c.d["e"] ? 1.0 : false
  ///  ^^^^^^^^^^^    ^^^^^^^^
  /// ```
  rule lhs_expr() -> Node<'input>
  = call_expr()
  / member_expr()

  /// A call expression.
  ///
  /// ```js
  /// -x.y(z)[a].b || c.d["e"] ? 1.0 : false
  ///    ^^^^^^^^^
  /// ```
  rule call_expr() -> Node<'input>
  = head:call_head()
    tail:(
        t:arguments() { Either::Left(t) }
      / t:member_tail() { Either::Right(t) }
    )*
  {
      tail.into_iter().fold(head, |object, args_or_property| {
          match args_or_property {
              Either::Left(arguments) => Node::CallExpr {
                  callee: Box::new(object),
                  arguments,
              },
              Either::Right(property) => Node::MemberExpr {
                  object: Box::new(object),
                  property,
              }
          }
      })
  }

  /// The head of a call expression.
  ///
  /// ```js
  /// -x.y(z)[a].b || c.d["e"] ? 1.0 : false
  ///    ^^^^
  /// ```
  rule call_head() -> Node<'input>
  = callee:member_expr() arguments:arguments()
  { Node::CallExpr { callee: Box::new(callee), arguments } }

  /// Call expression arguments.
  ///
  /// ```js
  /// -x.y(z)[a].b || c.d["e"] ? 1.0 : false
  ///     ^^^
  /// ```
  rule arguments() -> Vec<Node<'input>>
  = _ "(" _ arguments:cond_expr()**(_ "," _) _ ")"
  { arguments }

  /// A member expression.
  ///
  /// ```js
  /// -x.y(z)[a].b || c.d["e"] ? 1.0 : false
  ///                 ^^^^^^^^
  /// ```
  rule member_expr() -> Node<'input>
  = head:primary_expr() tail:member_tail()*
  {
      tail.into_iter().fold(head, |object, property| {
          Node::MemberExpr {
              object: Box::new(object),
              property,
          }
      })
  }

  /// A member expression tail.
  ///
  /// ```js
  /// -x.y(z)[a].b || c.d["e"] ? 1.0 : false
  ///                  ^^^^^^^
  /// ```
  rule member_tail() -> MemberProperty<'input>
  = _ "." _ property:$(scan_ident())
  { MemberProperty::Left(property) }
  / _ "[" _ property:expr() _ "]"
  { MemberProperty::Right(Box::new(property)) }

  /// A primary expression.
  ///
  /// ```js
  /// (expr)
  /// [0, 1, 2]
  /// { k: v, 0: n, "s": s }
  /// "lit"
  /// ident
  /// 1.0
  /// 0x10
  /// /regex/
  /// ```
  rule primary_expr() -> Node<'input>
  = "(" _ expression:expr() _ ")"
  { expression }
  / "[" _ elements:(cond_expr() / "" { Node::Literal(Value::Null.into()) })**(_ "," _) _ "]"
  { Node::ArrayExpr(elements) }
  / "{" _ properties:object_prop()**(_ "," _) _ "}"
  // TODO: Technically this is supposed to verify that all keys are unique
  { Node::ObjectExpr(properties) }
  / s:scan_string() { Node::Literal(s.into_owned().into()) }
  / i:scan_ident()
  {?
    if let Ident::Keyword(kw) = i && !["if", "this"].contains(&kw) {
      Err("whitelisted keyword")
    } else {
      Ok(Node::Var(i))
    }
  }
  / n:scan_non_octal_number()
  { Node::Literal(n.into()) }
  / r:scan_regex()
  { Node::Literal(r) }

  /// An object key-value pair.
  ///
  /// ```js
  /// { k: v, 0: n, "s": s }
  ///   ^^^^  ^^^^  ^^^^^^
  /// ```
  rule object_prop() -> ObjectProperty<'input>
  = key:object_key() _ ":" _ value:cond_expr()
  { ObjectProperty { key, value } }

  /// An object key.
  ///
  /// ```js
  /// { k: v, 0: n, "s": s }
  ///   ^     ^     ^^^
  /// ```
  rule object_key() -> Cow<'static, str>
  = s:scan_string() { s.into_owned().into() }
  / n:scan_number() { n.to_string().into() }
  / i:$(scan_ident()) { i.to_owned().into() }

  /// A regular expression literal.
  ///
  /// ```js
  /// /regex/flags
  /// ```
  rule scan_regex() -> VmValue<'static>
  = "/" r:($([^'\\'|'/']+) / escape_regex())+ "/" f:$(['g'|'i'|'m'|'u'|'y']*)
  {?
    let r = if let &[r] = r.as_slice() {
        Cow::Borrowed(r)
    } else {
        Cow::Owned(r.join(""))
    };

    make_regex(&r, f).map_err(|_| "valid regex")
  }

  /// A regex literal escape sequence.
  ///
  /// ```js
  /// /reg\/ex/flags
  ///     ^^
  /// ```
  rule escape_regex() -> &'input str
  = "\\" !eol() c:$([_])
  { c }

  /// A quoted string literal.
  ///
  /// ```js
  /// "string\x66~\141~\u0072~\x{74}~\n"
  /// 'string\x66~\141~\u0072~\x{74}~\n'
  /// ```
  rule scan_string() -> Cow<'input, str>
  = q:['"'|'\'']
    s:(simple_string(q) / complex_string(q))
    [c if c == q]
  { s }

  /// A quoted string literal without escape sequences.
  ///
  /// ```js
  /// "boring"
  /// 'boring'
  /// ```
  rule simple_string(q: char) -> Cow<'input, str>
  = s:$([c if c != q && c != '\\']+)
  { Cow::Borrowed(s) }

  /// A quoted string literal with escape sequences.
  ///
  /// ```js
  /// "exciting\x66~\141~\u0072~\x{74}~\n"
  /// 'exciting\x66~\141~\u0072~\x{74}~\n'
  /// ```
  rule complex_string(q: char) -> Cow<'static, str>
  = s:(
    "\\" c:string_escape_or_eol() { c }
    / !eol() c:[c if c != q] { Some(c) }
  )+
  { Cow::Owned(s.into_iter().flatten().collect()) }

  /// A line continuation or escape sequence.
  ///
  /// ```js
  /// "continu\␤ation"
  ///         ^^
  /// 'exciting\x66~\141~\u0072~\x{74}~\n'
  ///          ^^^^ ^^^^ ^^^^^^ ^^^^^^ ^^
  /// ```
  rule string_escape_or_eol() -> Option<char>
  = eol() { None }
  / c:string_escape() { Some(c) }

  /// A string escape sequence.
  ///
  /// ```js
  /// 'exciting\x66~\141~\u0072~\x{74}~\n'
  ///          ^^^^ ^^^^ ^^^^^^ ^^^^^^ ^^
  /// ```
  rule string_escape() -> char
  = "u" d:("{" d:$(hex_digit()*<1,6>) "}" { d } / d:$(hex_digit()*<4,4>) { d })
  {? u32::from_str_radix(d, 16).ok().and_then(char::from_u32).ok_or("unicode escape") }
  / "x" d:$(hex_digit()*<2,2>)
  { char::from_u32(u32::from_str_radix(d, 16).unwrap()).unwrap() }
  / "b" { '\x08' }
  / "f" { '\x0c' }
  / "n" { '\n' }
  / "r" { '\r' }
  / "t" { '\t' }
  / "v" { '\x0b' }
  / "\r" "\n"? { '\n' }
  / octal_escape()
  / [_]

  /// An octal escape sequence.
  ///
  /// ```js
  /// 'exciting\x66~\141~\u0072~\x{74}~\n'
  ///               ^^^^
  /// ```
  rule octal_escape() -> char
  = o:$(['0'..='3']? octal_digit() octal_digit()?)
  { char::from_u32(u32::from_str_radix(o, 8).unwrap()).unwrap() }

  /// A number literal.
  ///
  /// ```js
  /// 0x10
  /// 10
  /// .10
  /// 1e10
  /// 1e+10
  /// 1e-10
  /// 010
  /// ```
  rule scan_number() -> f64
  = n:(hex_number() / octal_number() / decimal_number()) !ident_start()
  { n }

  /// A non-octal number literal.
  ///
  /// ```js
  /// 0x10
  /// 10
  /// .10
  /// ```
  rule scan_non_octal_number() -> f64
  = n:(hex_number() / decimal_number()) !ident_start()
  { n }

  /// A hexadecimal number literal.
  ///
  /// ```js
  /// 0x10
  /// ```
  rule hex_number() -> f64
  = "0" ['x'|'X'] d:$(hex_digit()+)
  {
    #[expect(clippy::cast_precision_loss, reason = "loss would also occur in ECMAScript")]
    { u64::from_str_radix(d, 16).unwrap() as f64 }
  }

  /// An octal number literal.
  ///
  /// ```js
  /// 010
  /// ```
  rule octal_number() -> f64
  = d:$("0" octal_digit()*)
  {
    #[expect(clippy::cast_precision_loss, reason = "loss would also occur in ECMAScript")]
    { u64::from_str_radix(d, 8).unwrap() as f64 }
  }

  /// A decimal number literal.
  ///
  /// ```js
  /// 10
  /// .10
  /// 1e10
  /// 1e+10
  /// 1e-10
  /// ```
  rule decimal_number() -> f64
  = d:$((digit()+ ("." digit()+)? / "." digit()+) (['e'|'E'] ['+'|'-']? digit()+)?)
  { d.parse().unwrap() }

  /// A null, bool, keyword, or ident.
  rule scan_ident() -> Ident<'input>
  = "null" { Ident::Null }
  / "true" { Ident::Bool(true) }
  / "false" { Ident::Bool(false) }
  / keyword()
  / ident()

  /// Decimal digit.
  rule digit()
  = ['0'..='9']

  /// Hexadecimal digit.
  rule hex_digit()
  = ['0'..='9'|'A'..='F'|'a'..='f']

  /// Octal digit.
  rule octal_digit()
  = ['0'..='7']

  /// Whitespace.
  rule space()
  = [' '|'\t'|'\x0b'|'\x0c'|'\u{00a0}'
    |'\u{1680}'|'\u{180E}'
    |'\u{2000}'|'\u{2001}'|'\u{2002}'|'\u{2003}'|'\u{2004}'|'\u{2005}'
    |'\u{2006}'|'\u{2007}'|'\u{2008}'|'\u{2009}'|'\u{200A}'
    |'\u{202F}'|'\u{205F}'
    |'\u{3000}'|
    '\u{FEFF}']

  /// A line terminator.
  rule eol()
  = ['\r'|'\n'|'\u{2028}'|'\u{2029}']

  /// A token separator.
  rule _ = (space() / eol())*

  /// An ident.
  rule ident() -> Ident<'input>
  = i:$(ident_start() ident_rest()*)
  { Ident::Plain(i) }

  /// Ident first character.
  rule ident_start()
  = ['$'|'_'|'A'..='Z'|'a'..='z'|'\\']
  / [c if is_ident_start(c)]

  /// Ident remaining characters.
  rule ident_rest()
  = ident_start()
  / [c if is_ident_rest(c)]

  /// A keyword.
  rule keyword() -> Ident<'input>
  = k:$(
    "instanceof" / "implements"
  / "interface" / "protected"
  / "function" / "continue" / "debugger"
  / "default" / "finally" / "extends" / "package" / "private"
  / "return" / "typeof" / "delete" / "switch" / "export" / "import" / "public" / "static"
  / "while" / "break" / "catch" / "throw" / "const" / "yield" / "class" / "super"
  / "this" / "else" / "case" / "void" / "with" / "enum"
  / "var" / "for" / "new" / "try" / "let"
  / "if" / "in" / "do")
  !ident_rest()
  { Ident::Keyword(k) }
}}

/// A callable function.
trait Callable: for<'s> Fn(&GraphNode<'s, '_>, &[VmValue<'s>]) -> Result<VmValue<'s>> {}
impl<T> Callable for T where T: for<'s> Fn(&GraphNode<'s, '_>, &[VmValue<'s>]) -> Result<VmValue<'s>>
{}
impl core::fmt::Debug for dyn Callable {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Callable")
    }
}

/// Predefined constants.
const CONSTANTS: phf::Map<&'static str, f64> = phf::phf_map! {
    "NaN" => f64::NAN,
    "E" => core::f64::consts::E,
    "LN2" => core::f64::consts::LN_2,
    "LN10" => core::f64::consts::LN_10,
    "LOG2E" => core::f64::consts::LOG2_E,
    "LOG10E" => core::f64::consts::LOG10_E,
    "PI" => core::f64::consts::PI,
    "SQRT1_2" => core::f64::consts::FRAC_1_SQRT_2,
    "SQRT2" => core::f64::consts::SQRT_2,
};

/// A binary operator.
#[derive(Clone, Copy, Debug)]
#[allow(
    clippy::allow_attributes,
    reason = "https://github.com/rust-lang/rust-clippy/issues/13358"
)]
#[allow(
    clippy::missing_docs_in_private_items,
    reason = "self-documenting variants"
)]
enum BinaryOp {
    BitAnd,
    BitOr,
    BitXor,
    Eq,
    Ne,
    StrictEq,
    StrictNe,
    Gt,
    Gte,
    Lt,
    Lte,
    InstanceOf,
    In,
    Shl,
    Sshr,
    Ushr,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
}

impl BinaryOp {
    /// Applies the operator to the given operands.
    fn apply(self, lhs: VmValue<'_>, rhs: VmValue<'_>) -> VmValue<'static> {
        match self {
            Self::BitAnd => f64::from(lhs.into_i32() & rhs.into_i32()).into(),
            Self::BitOr => f64::from(lhs.into_i32() | rhs.into_i32()).into(),
            Self::BitXor => f64::from(lhs.into_i32() ^ rhs.into_i32()).into(),
            Self::Eq => lhs.is_eq(&rhs).into(),
            Self::Ne => lhs.is_ne(&rhs).into(),
            Self::StrictEq => (lhs == rhs).into(),
            Self::StrictNe => (lhs != rhs).into(),
            Self::Gt => lhs.is_gt(&rhs).into(),
            Self::Gte => lhs.is_gte(&rhs).into(),
            Self::Lt => lhs.is_lt(&rhs).into(),
            Self::Lte => lhs.is_lte(&rhs).into(),
            Self::InstanceOf => todo!(),
            Self::In => todo!(),
            Self::Shl => f64::from(lhs.into_i32() << rhs.into_i32()).into(),
            Self::Sshr => f64::from(lhs.into_i32() >> rhs.into_i32()).into(),
            Self::Ushr => f64::from(lhs.into_i32().cast_unsigned() >> rhs.into_i32()).into(),
            Self::Add => match (&*lhs.to_primitive(), &*rhs.to_primitive()) {
                (Value::Str(lhs), rhs) => {
                    format!("{lhs}{rhs}", rhs = ValueExt::to_string(rhs)).into()
                }
                (lhs, Value::Str(rhs)) => {
                    format!("{lhs}{rhs}", lhs = ValueExt::to_string(lhs)).into()
                }
                (lhs, rhs) => (lhs.to_f64() + rhs.to_f64()).into(),
            },
            Self::Sub => (lhs.into_f64() - rhs.into_f64()).into(),
            Self::Mul => (lhs.into_f64() * rhs.into_f64()).into(),
            Self::Div => (lhs.into_f64() / rhs.into_f64()).into(),
            Self::Mod => (lhs.into_f64() % rhs.into_f64()).into(),
        }
    }
}

/// An ident.
#[derive(Clone, Copy, Debug)]
enum Ident<'code> {
    /// A boolean literal.
    Bool(bool),
    /// A keyword ident.
    Keyword(&'code str),
    /// A null literal.
    Null,
    /// A non-keyword ident.
    Plain(&'code str),
}

/// A logical operator.
#[derive(Clone, Copy, Debug)]
#[allow(
    clippy::allow_attributes,
    reason = "https://github.com/rust-lang/rust-clippy/issues/13358"
)]
#[allow(
    clippy::missing_docs_in_private_items,
    reason = "self-documenting variants"
)]
enum LogicalOp {
    Or,
    And,
}

/// A member expression property.
///
/// If the property should be computed, this is a Node; otherwise, it is an
/// ident.
type MemberProperty<'code> = Either<&'code str, Box<Node<'code>>>;

/// An AST node.
#[derive(Clone, Debug)]
enum Node<'code> {
    /// An array literal.
    ArrayExpr(Vec<Node<'code>>),
    /// A binary expression.
    BinaryExpr {
        /// The operator.
        operator: BinaryOp,
        /// The left hand side.
        lhs: Box<Node<'code>>,
        /// The right hand side.
        rhs: Box<Node<'code>>,
    },
    /// A call expression.
    CallExpr {
        /// The callee.
        callee: Box<Node<'code>>,
        /// The list of arguments.
        arguments: Vec<Node<'code>>,
    },
    /// A conditional expression.
    CondExpr {
        /// The test expression.
        test: Box<Node<'code>>,
        /// The consequent.
        consequent: Box<Node<'code>>,
        /// The alternate.
        alternate: Box<Node<'code>>,
    },
    /// A literal (number, string, regex).
    Literal(VmValue<'static>),
    /// A short-circuiting logical binary expression.
    LogicalExpr {
        /// The operator.
        operator: LogicalOp,
        /// The left hand side.
        lhs: Box<Node<'code>>,
        /// The right hand side.
        rhs: Box<Node<'code>>,
    },
    /// A member accessor expression.
    MemberExpr {
        /// The object.
        object: Box<Node<'code>>,
        /// The property.
        property: MemberProperty<'code>,
    },
    /// An object literal.
    ObjectExpr(Vec<ObjectProperty<'code>>),
    /// A unary expression.
    UnaryExpr {
        /// The operator.
        operator: UnaryOp,
        /// The operand.
        argument: Box<Node<'code>>,
    },
    /// A variable.
    Var(Ident<'code>),
}

impl<'code> Node<'code> {
    /// Shorthand for creating a new binary expression node.
    fn new_binary(operator: BinaryOp, lhs: Node<'code>, rhs: Node<'code>) -> Self {
        Self::BinaryExpr {
            operator,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        }
    }

    /// Shorthand for creating a new logical binary expression node.
    fn new_logical(operator: LogicalOp, lhs: Node<'code>, rhs: Node<'code>) -> Self {
        Self::LogicalExpr {
            operator,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        }
    }

    /// Evaluates the node against the given environment, returning the result.
    fn eval<'s>(&self, env: &GraphNode<'s, '_>) -> Result<VmValue<'s>> {
        let result = match self {
            Self::ArrayExpr(nodes) => nodes
                .iter()
                .map(|node| node.eval(env))
                .collect::<Result<Vec<_>>>()?
                .into(),
            Self::BinaryExpr {
                operator,
                lhs: left,
                rhs: right,
            } => {
                let lhs = left.eval(env)?;
                let rhs = right.eval(env)?;
                operator.apply(lhs, rhs)
            }
            Self::CallExpr { callee, arguments } => {
                let callee = callee.eval(env)?;
                if let VmValue::Callable(callee) = callee {
                    let arguments = arguments
                        .iter()
                        .map(|arg| arg.eval(env))
                        .collect::<Result<Vec<_>>>()?;
                    callee(env, &arguments)?
                } else {
                    <_>::default()
                }
            }
            Self::CondExpr {
                test,
                consequent,
                alternate,
            } => {
                if test.eval(env)?.to_bool() {
                    consequent.eval(env)?
                } else {
                    alternate.eval(env)?
                }
            }
            Self::Literal(value) => value.clone(),
            Self::LogicalExpr {
                operator,
                lhs: left,
                rhs: right,
            } => {
                let if_true = matches!(operator, LogicalOp::Or);
                let value = left.eval(env)?;
                if value.to_bool() == if_true {
                    value
                } else {
                    right.eval(env)?
                }
            }
            Self::MemberExpr { object, property } => {
                let object = object.eval(env)?;
                let property = match property {
                    Either::Left(ident) => ident.to_string().into(),
                    Either::Right(expr) => expr.eval(env)?,
                };
                object.get(&property)
            }
            Self::ObjectExpr(properties) => properties
                .iter()
                .map(|property| Ok((property.key.clone(), property.value.eval(env)?)))
                .collect::<Result<_>>()?,
            Self::UnaryExpr { operator, argument } => {
                let value = argument.eval(env)?;
                operator.apply(value)
            }
            Self::Var(token) => match *token {
                Ident::Bool(b) => b.into(),
                Ident::Plain(key) | Ident::Keyword(key) => {
                    // TODO: Cloning is inefficient and bad. To start avoiding
                    // it, it is necessary for `VmValue` to be able to hold
                    // `Vec<Cow<'_, VmValue<'_>>>` and `&VmValue<'_>`.
                    if key == "datum" {
                        VmValue::Value(env.item.clone())
                    } else if key == "parent" {
                        VmValue::Value(env.parent.map_or(Value::Null, |parent| parent.item.clone()))
                    } else if let Some(value) = CONSTANTS.get(key) {
                        (*value).into()
                    } else if let Some(value) = FUNCTIONS.get(key) {
                        (*value).into()
                    } else {
                        Value::Null.into()
                    }
                }
                Ident::Null => Value::Null.into(),
            },
        };
        Ok(result)
    }
}

/// An object literal property.
#[derive(Clone, Debug)]
struct ObjectProperty<'code> {
    /// The key.
    key: Cow<'static, str>,
    /// The value.
    value: Node<'code>,
}

/// A unary operator.
#[derive(Clone, Copy, Debug)]
#[allow(
    clippy::allow_attributes,
    reason = "https://github.com/rust-lang/rust-clippy/issues/13358"
)]
#[allow(
    clippy::missing_docs_in_private_items,
    reason = "self-documenting variants"
)]
enum UnaryOp {
    BitNot,
    LogicalNot,
    Neg,
    Num,
}

impl UnaryOp {
    /// Applies the operator to the given operand.
    fn apply(self, value: VmValue<'_>) -> VmValue<'static> {
        match self {
            Self::Num => value.into_f64().into(),
            Self::Neg => (-value.into_f64()).into(),
            Self::BitNot => f64::from(!value.to_i32()).into(),
            Self::LogicalNot => (!value.into_bool()).into(),
        }
    }
}

/// A type which is convertible from an iterator of [`&VmValue<'_>`](VmValue).
trait FromValues<'s>: Sized {
    /// Converts the iterator to the value.
    fn from_values<'a>(values: impl Iterator<Item = &'a VmValue<'s>>) -> Result<Self>
    where
        's: 'a;
}

impl<'s, T> FromValues<'s> for Option<T>
where
    for<'a> T: From<&'a VmValue<'s>>,
{
    fn from_values<'a>(mut values: impl Iterator<Item = &'a VmValue<'s>>) -> Result<Self>
    where
        's: 'a,
    {
        Ok(values.next().map(Into::into))
    }
}

impl<'s, T> FromValues<'s> for T
where
    for<'a> T: From<&'a VmValue<'s>>,
{
    fn from_values<'a>(mut values: impl Iterator<Item = &'a VmValue<'s>>) -> Result<Self>
    where
        's: 'a,
    {
        values.next().ok_or(Error::RequiredArg).map(Into::into)
    }
}

impl From<&VmValue<'_>> for bool {
    fn from(value: &VmValue<'_>) -> Self {
        value.to_bool()
    }
}

impl From<&VmValue<'_>> for f64 {
    fn from(value: &VmValue<'_>) -> Self {
        value.to_f64()
    }
}

impl From<&VmValue<'_>> for i64 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "this conversion is used only for dates and matches ES2026 §§21.4.1.27-28"
    )]
    fn from(value: &VmValue<'_>) -> Self {
        value.to_f64() as i64
    }
}

impl<'s> From<&VmValue<'s>> for Cow<'s, str> {
    fn from(value: &VmValue<'s>) -> Self {
        ValueExt::to_string(value)
    }
}

// SPDX-SnippetBegin
// SPDX-License-Identifier: MIT
// SPDX-SnippetComment: Copied from piccolo
/// Implements [`FromValues`] for a tuple.
macro_rules! impl_tuple {
    ($($name:ident),* $(,)?) => (
        impl<'s, $($name,)*> FromValues<'s> for ($($name,)*)
            where $($name: FromValues<'s>,)*
        {
            #[allow(clippy::allow_attributes, unused_variables, unused_mut, non_snake_case, reason = "autogenerated code")]
            fn from_values<'a>(
                mut values: impl Iterator<Item = &'a VmValue<'s>>,
            ) -> Result<Self> where 's: 'a {
                $(let $name = FromValues::from_values(&mut values)?;)*
                Ok(($($name,)*))
            }
        }
    );
}

/// Expands `$m` for all shorter tuples `(C, B, A)` -> `(B, A)` -> `(A)`.
macro_rules! smaller_tuples_too {
    ($m:ident, $ty:ident) => {
        $m!{}
        $m!{$ty}
    };

    ($m:ident, $ty:ident, $($tt:ident),*) => {
        smaller_tuples_too!{$m, $($tt),*}
        $m!{$ty, $($tt),*}
    };
}

smaller_tuples_too!(impl_tuple, G, F, E, D, C, B, A);

// SPDX-SnippetEnd

/// Creates an expression function wrapper around an existing Rust function.
const fn wrap<F, A, T>(
    f: F,
) -> impl for<'s> Fn(&GraphNode<'s, '_>, &[VmValue<'s>]) -> Result<VmValue<'s>>
where
    F: Fn(A) -> T,
    A: for<'s> FromValues<'s>,
    T: for<'s> Into<VmValue<'s>>,
{
    move |_, values| Ok(f(A::from_values(values.iter())?).into())
}

/// Creates an expression function wrapper for a single value.
// TODO: This is only required because I cannot figure out why `wrap` refuses to
// work with `impl<'input> FromValues<'input> for Cow<'input, str>`. Presumably
// it will refuse to work with any borrowed value. I have better things to do
// with life than figure this out.
const fn wrap_single<F, T>(
    f: F,
) -> impl for<'s> Fn(&GraphNode<'s, '_>, &[VmValue<'s>]) -> Result<VmValue<'s>>
where
    F: for<'s> Fn(&'_ VmValue<'s>) -> T,
    T: for<'s> Into<VmValue<'s>>,
{
    move |_, values| {
        let first = values.first().ok_or(Error::RequiredArg)?;
        Ok(f(first).into())
    }
}

/// Creates an expression function wrapper for a string plus extra arguments.
// TODO: This is only required because I cannot figure out why `wrap` refuses to
// work with `impl<'code> FromValues<'code> for Cow<'code, str>`. Presumably it will
// refuse to work with any borrowed value. I have better things to do with life
// than figure this out.
const fn wrap_str<F, A, T>(
    f: F,
) -> impl for<'s> Fn(&GraphNode<'s, '_>, &[VmValue<'s>]) -> Result<VmValue<'s>>
where
    F: Fn(&'_ str, A) -> T,
    A: for<'s> FromValues<'s>,
    T: for<'s> Into<VmValue<'s>>,
{
    move |_, values| {
        let first = values
            .first()
            .ok_or(Error::RequiredArg)
            .map(ValueExt::to_string)?;
        let rest = values.get(1..).unwrap_or_default().iter();
        Ok(f(&first, A::from_values(rest)?).into())
    }
}

/// Creates an expression function wrapper around a date function.
const fn wrap_date<const UTC: bool, F, T>(
    f: F,
) -> impl for<'s> Fn(&GraphNode<'s, '_>, &[VmValue<'s>]) -> Result<VmValue<'s>>
where
    F: Fn(DateTime) -> T,
    T: Into<f64>,
{
    move |_, values| {
        let date = values.first().ok_or(Error::RequiredArg)?.to_date(UTC)?;
        Ok(VmValue::from(f(date).into()))
    }
}

/// Built-in expression functions.
mod built_ins {
    use super::*;

    /// Creates a new date.
    pub(super) fn new_date<'s, const UTC: bool>(
        node: &GraphNode<'s, '_>,
        args: &[VmValue<'s>],
    ) -> Result<VmValue<'s>> {
        Ok(if args.is_empty() {
            if UTC {
                node.now
            } else {
                node.now.into_offset(DateTimeZone::local()?)?
            }
        } else if let [timestamp] = args {
            DateTime::from_f64(timestamp.to_f64(), UTC)
        } else {
            type TimeParts = (
                i64,
                Option<i64>,
                Option<i64>,
                Option<i64>,
                Option<i64>,
                Option<i64>,
                Option<i64>,
            );
            const MS_TO_US: i64 = 1_000;

            let (year, month, day, hour, minute, second, micros): TimeParts =
                FromValues::from_values(args.iter())?;
            let month = month.map(|month| month + 1);
            let micros = micros.map(|millis| millis * MS_TO_US);
            DateTime::from_parts(
                year,
                month,
                day,
                hour,
                minute,
                second,
                micros,
                Some(if UTC {
                    &DateTimeZone::UTC
                } else {
                    node.now.time_zone()
                }),
            )?
        }
        .into())
    }

    /// Returns true if the given `value` is in the range of `min..=max` (if
    /// `exclusive` is false) or `min < value < max` (if `exclusive` is true).
    pub(super) fn in_range(
        (value, min, max, exclusive): (Option<f64>, Option<f64>, Option<f64>, Option<bool>),
    ) -> bool {
        let value = value.unwrap_or(0.0);
        let min = min.unwrap_or(0.0);
        let max = max.unwrap_or(0.0);
        let exclusive = exclusive.unwrap_or_default();
        let min = min.min(max);
        let max = max.max(min);
        if exclusive {
            min < value && value < max
        } else {
            (min..=max).contains(&value)
        }
    }

    /// Returns the minimum of `items`.
    pub(super) fn min<'s>(_: &GraphNode<'_, '_>, items: &[VmValue<'s>]) -> Result<VmValue<'s>> {
        Ok(items
            .iter()
            .map(ValueExt::to_f64)
            .reduce(f64::min)
            .unwrap_or(f64::INFINITY)
            .into())
    }

    /// Returns the maximum of `items`.
    pub(super) fn max<'s>(_: &GraphNode<'_, '_>, items: &[VmValue<'s>]) -> Result<VmValue<'s>> {
        Ok(items
            .iter()
            .map(ValueExt::to_f64)
            .reduce(f64::max)
            .unwrap_or(f64::NEG_INFINITY)
            .into())
    }

    /// A wrapper for calls which can operate on either a string or an array.
    fn array_or_string<'s, S, A1, A2, T>(
        item: &VmValue<'s>,
        s: S,
        vm_array: A1,
        json_array: A2,
    ) -> T
    where
        S: FnOnce(&str) -> T,
        A1: FnOnce(&[VmValue<'s>]) -> T,
        A2: FnOnce(&[Value<'s>]) -> T,
    {
        match item {
            VmValue::Array(array) => vm_array(array.as_slice()),
            VmValue::Value(Value::Array(array)) => json_array(array.as_slice()),
            VmValue::Value(Value::Str(string)) => s(string),
            value => s(&value.to_string()),
        }
    }

    /// Returns the length of the item.
    #[expect(
        clippy::cast_precision_loss,
        reason = "if there are ever ≥2**53 items, something sure happened"
    )]
    pub(super) fn length(item: &VmValue<'_>) -> f64 {
        array_or_string(item, |s| s.encode_utf16().count(), <[_]>::len, <[_]>::len) as f64
    }

    /// Finds an item in a string or array, returning the index, or -1 if no
    /// item is found.
    pub(super) fn find<'s, const BACKWARDS: bool>(
        _: &GraphNode<'_, '_>,
        items: &[VmValue<'s>],
    ) -> Result<VmValue<'s>> {
        let index = if let [haystack, needle, ..] = items {
            array_or_string(
                haystack,
                |s| {
                    let needle = needle.to_string();
                    if BACKWARDS {
                        s.rfind(&*needle)
                    } else {
                        s.find(&*needle)
                    }
                    .map(|index| s[..index].encode_utf16().count())
                },
                |a| {
                    let mut iter = a.iter();
                    if BACKWARDS {
                        iter.rposition(|i| i == needle)
                    } else {
                        iter.position(|i| i == needle)
                    }
                },
                |a| {
                    let needle = needle.clone().unwrap_value();
                    let mut iter = a.iter();
                    if BACKWARDS {
                        iter.rposition(|i| *i == needle)
                    } else {
                        iter.position(|i| *i == needle)
                    }
                },
            )
        } else {
            None
        };

        #[expect(
            clippy::cast_precision_loss,
            reason = "if there are ever ≥2**53 items, something sure happened"
        )]
        Ok(VmValue::from(index.map_or(-1.0, |index| index as f64)))
    }

    /// Replaces text in a string.
    pub(super) fn replace<'s>(_: &GraphNode<'_, '_>, items: &[VmValue<'s>]) -> Result<VmValue<'s>> {
        let Some(haystack) = items.first().map(ValueExt::to_string) else {
            return Ok(<_>::default());
        };

        let pattern = items.get(1).unwrap_or_default();
        let replacement = items.get(2).map_or(<_>::default(), ValueExt::to_string);

        Ok(match pattern {
            VmValue::Regex(regex, flags) => {
                let result = if flags.contains('g') {
                    regex.replace_all(&haystack, &*replacement)
                } else {
                    regex.replace(&haystack, &*replacement)
                };
                result.owned().unwrap_or(haystack)
            }
            pattern => {
                let pattern = ValueExt::to_string(pattern);
                strtr(&haystack, &[(&pattern, &replacement)])
                    .owned()
                    .unwrap_or(haystack)
            }
        }
        .into())
    }

    /// Equivalent to the ternary operator.
    pub(super) fn if_cond<'s>(_: &GraphNode<'_, '_>, items: &[VmValue<'s>]) -> Result<VmValue<'s>> {
        let test = items.first().is_some_and(ValueExt::to_bool);
        Ok(items
            .get(if test { 1 } else { 2 })
            .unwrap_or_default()
            .clone())
    }

    /// Creates a new regular expression from the given string and optional
    /// flags.
    pub(super) fn regexp<'s>(_: &GraphNode<'_, '_>, items: &[VmValue<'s>]) -> Result<VmValue<'s>> {
        let pattern = items.first().map_or(<_>::default(), ValueExt::to_string);
        let flags = items.get(1).map_or(<_>::default(), ValueExt::to_string);
        Ok(make_regex(&pattern, &flags)?)
    }

    /// Tests a value against a regular expression.
    pub(super) fn test<'s>(_: &GraphNode<'_, '_>, items: &[VmValue<'s>]) -> Result<VmValue<'s>> {
        if let Some(value) = items.first() {
            let needle = items.get(1).map_or("undefined".into(), ValueExt::to_string);
            Ok(value.to_regex()?.is_match(&needle).into())
        } else {
            Err(Error::RequiredArg)
        }
    }
}

/// Built-in functions.
const FUNCTIONS: phf::Map<&'static str, &'static dyn Callable> = phf::phf_map! {
    "isNaN" => &wrap(f64::is_nan),
    "isFinite" => &wrap(f64::is_finite),
    "abs" => &wrap(f64::abs),
    "acos" => &wrap(f64::acos),
    "asin" => &wrap(f64::asin),
    "atan" => &wrap(f64::atan),
    "atan2" => &wrap(|(x, y)| f64::atan2(x, y)),
    "ceil" => &wrap(f64::ceil),
    "cos" => &wrap(f64::cos),
    "exp" => &wrap(f64::exp),
    "floor" => &wrap(f64::floor),
    "log" => &wrap(f64::ln),
    "max" => &built_ins::max,
    "min" => &built_ins::min,
    "pow" => &wrap(|(val, pow)| f64::powf(val, pow)),
    "random" => &|e, _| Ok(e.rng.borrow_mut().random::<f64>().into()),
    "round" => &wrap(f64::round),
    "sin" => &wrap(f64::sin),
    "sqrt" => &wrap(f64::sqrt),
    "tan" => &wrap(f64::tan),
    "clamp" => &wrap(|(v, min, max)| f64::clamp(v, min, max)),
    "inrange" => &wrap(built_ins::in_range),
    "now" => &|e, _| Ok(e.now.into()),
    "datetime" => &built_ins::new_date::<false>,
    "date" => &wrap_date::<false, _, _>(|date| date.day()),
    "day" => &wrap_date::<false, _, _>(|date| date.weekday() as u8),
    "year" => &wrap_date::<false, _, _>(|v| v.year()),
    "month" => &wrap_date::<false, _, _>(|v| v.month() as u8 - 1),
    "hours" => &wrap_date::<false, _, _>(|v| v.hour()),
    "minutes" => &wrap_date::<false, _, _>(|v| v.minute()),
    "seconds" => &wrap_date::<false, _, _>(|v| v.second()),
    "milliseconds" => &wrap_date::<false, _, _>(|v| v.millisecond()),
    "time" => &wrap_date::<false, _, _>(|v| v.to_f64()),
    "timezoneoffset" => &wrap_date::<false, _, _>(|v| v.offset().whole_seconds()),
    "utc" => &built_ins::new_date::<true>,
    "utcdate" => &wrap_date::<true, _, _>(|date| date.day()),
    "utcday" => &wrap_date::<true, _, _>(|date| date.weekday() as u8),
    "utcyear" => &wrap_date::<true, _, _>(|v| v.year()),
    "utcmonth" => &wrap_date::<true, _, _>(|v| v.month() as u8 - 1),
    "utchours" => &wrap_date::<true, _, _>(|v| v.hour()),
    "utcminutes" => &wrap_date::<true, _, _>(|v| v.minute()),
    "utcseconds" => &wrap_date::<true, _, _>(|v| v.second()),
    "utcmilliseconds" => &wrap_date::<true, _, _>(|v| v.millisecond()),
    "length" => &wrap_single(built_ins::length),
    "indexof" => &built_ins::find::<false>,
    "lastindexof" => &built_ins::find::<true>,
    "parseFloat" => &wrap_str(|value, ()| floatval(value).map_or(f64::NAN, |(v, _)| v)),
    "parseInt" => &wrap_str(|value, radix: Option<f64>| {
        #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "if radix is out of range, the input was bogus, and the result will be NaN, which is the correct output")]
        let radix = radix.and_then(|radix| {
            if radix.is_nan() { None } else { Some(radix as u32) }
        });
        #[expect(clippy::cast_precision_loss, reason = "matches ES2026")]
        intval(value, radix).map_or(f64::NAN, |(v, _)| v as f64)
    }),
    "upper" => &wrap_str(|value, ()| value.to_uppercase()),
    "lower" => &wrap_str(|value, ()| value.to_lowercase()),
    "replace" => &built_ins::replace,
    "slice" => &|_, _| todo!(),
    "substring" => &|_, _| todo!(),
    "format" => &|_, _| todo!(),
    "timeFormat" => &|_, _| todo!(),
    "utcFormat" => &|_, _| todo!(),
    "regexp" => &built_ins::regexp,
    "test" => &built_ins::test,
    "if" => &built_ins::if_cond,
    "open" => &|_, _| Ok(<_>::default()),
};

/// An expression VM value.
#[derive(Clone, Debug)]
enum VmValue<'s> {
    /// An array.
    Array(Vec<VmValue<'s>>),
    /// A callable function.
    Callable(&'static dyn Callable),
    /// A date.
    Date(DateTime),
    /// An object.
    Object(HashMap<serde_json_borrow::KeyStrType<'s>, VmValue<'s>>),
    /// A regular expression.
    Regex(Regex, Cow<'s, str>),
    /// A more primitive type representable by [`Value`].
    Value(Value<'s>),
}

impl<'s> VmValue<'s> {
    /// If the Value is a number, represent it as f64 if possible. Returns None
    /// otherwise.
    fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Value(value) => value.as_f64(),
            _ => None,
        }
    }

    /// Gets a value from an array or object value.
    fn get(&self, key: &VmValue<'s>) -> VmValue<'s> {
        match self {
            Self::Array(values) => {
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "wacky keys will just return null"
                )]
                let key = key.to_f64() as usize;
                values.get(key).map_or(<_>::default(), Clone::clone)
            }
            Self::Object(object) => {
                let key = key.to_string();
                object.get(&key.into()).map_or(<_>::default(), Clone::clone)
            }
            Self::Value(Value::Array(values)) => {
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "wacky keys will just return null"
                )]
                let key = key.to_f64() as usize;
                values.get(key).cloned().map_or(<_>::default(), Self::Value)
            }
            Self::Value(Value::Object(values)) => {
                let key = key.to_string();
                values
                    .get(&key)
                    .cloned()
                    .map_or(<_>::default(), Self::Value)
            }
            _ => <_>::default(),
        }
    }

    /// Converts this value into an i32 according to ECMAScript 2026 §7.1.6.
    #[expect(clippy::cast_possible_truncation, reason = "intentional")]
    #[inline]
    fn into_i32(self) -> i32 {
        self.into_f64() as i32
    }

    /// Converts this value to an i32 according to ECMAScript 2026 §7.1.6.
    #[expect(clippy::cast_possible_truncation, reason = "intentional")]
    #[inline]
    fn to_i32(&self) -> i32 {
        self.to_f64() as i32
    }

    /// Converts this value into a non-object [`Value`] using the DEFAULT
    /// hint per the rules in ECMAScript 2026 §7.1.1.
    fn to_primitive(&self) -> Cow<'_, Value<'s>> {
        match self {
            Self::Value(value) => match value {
                Value::Null | Value::Bool(_) => Cow::Owned(value.to_f64().into()),
                value @ (Value::Number(_) | Value::Str(_)) => Cow::Borrowed(value),
                Value::Array(_) | Value::Object(_) => Cow::Owned(ValueExt::to_string(self).into()),
            },
            _ => Cow::Owned(ValueExt::to_string(self).into()),
        }
    }

    /// Converts this value into a non-object [`Value`] using the NUMBER
    /// hint per the rules in ECMAScript 2026 §7.1.1.
    #[inline]
    fn to_primitive_number(&self) -> Cow<'_, Value<'s>> {
        match self {
            Self::Date(date) => Cow::Owned(Value::Number(date.to_f64().into())),
            Self::Value(value) => Cow::Borrowed(value),
            _ => Cow::Owned(ValueExt::to_string(self).into()),
        }
    }

    /// Tries to convert this value to a [`Regex`].
    fn to_regex(&self) -> Result<Cow<'_, Regex>> {
        if let Self::Regex(regex, _) = self {
            Ok(Cow::Borrowed(regex))
        } else {
            Regex::new(&self.to_string())
                .map(Cow::Owned)
                .map_err(Into::into)
        }
    }

    /// Converts this value into an object [`Value`].
    fn unwrap_value(self) -> Value<'s> {
        match self {
            Self::Array(values) => {
                Value::Array(values.into_iter().map(Self::unwrap_value).collect())
            }
            Self::Object(_) => todo!(),
            Self::Value(value) => value,
            _ => Value::Str(ValueExt::into_string(self)),
        }
    }
}

impl From<bool> for VmValue<'_> {
    fn from(value: bool) -> Self {
        Self::Value(value.into())
    }
}

impl From<&'static dyn Callable> for VmValue<'_> {
    fn from(value: &'static dyn Callable) -> Self {
        Self::Callable(value)
    }
}

impl From<DateTime> for VmValue<'_> {
    fn from(value: DateTime) -> Self {
        Self::Date(value)
    }
}

impl From<f64> for VmValue<'_> {
    fn from(value: f64) -> Self {
        Self::Value(value.into())
    }
}

impl<'s> From<&'s str> for VmValue<'s> {
    fn from(value: &'s str) -> Self {
        Self::Value(value.into())
    }
}

impl From<String> for VmValue<'_> {
    fn from(value: String) -> Self {
        Self::Value(value.into())
    }
}

impl<'s> From<Cow<'s, str>> for VmValue<'s> {
    fn from(value: Cow<'s, str>) -> Self {
        Self::Value(value.into())
    }
}

impl<'s> From<Vec<VmValue<'s>>> for VmValue<'s> {
    fn from(value: Vec<VmValue<'s>>) -> Self {
        Self::Array(value)
    }
}

impl<'s> From<Value<'s>> for VmValue<'s> {
    fn from(value: Value<'s>) -> Self {
        Self::Value(value)
    }
}

impl<'s, K, V> FromIterator<(K, V)> for VmValue<'s>
where
    K: Into<serde_json_borrow::KeyStrType<'s>>,
    V: Into<VmValue<'s>>,
{
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        Self::Object(
            iter.into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        )
    }
}

impl Default for VmValue<'_> {
    fn default() -> Self {
        Self::Value(Value::Null)
    }
}

impl Default for &VmValue<'_> {
    fn default() -> Self {
        &VmValue::Value(Value::Null)
    }
}

impl PartialEq for VmValue<'_> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            // TODO: Strict object equality means that arrays and objects
            // actually need to be `Rc` or similar.
            (Self::Array(lhs), Self::Array(rhs)) => core::ptr::eq(lhs, rhs),
            (Self::Callable(lhs), Self::Callable(rhs)) => core::ptr::eq(lhs, rhs),
            (Self::Date(lhs), Self::Date(rhs)) => core::ptr::eq(lhs, rhs),
            (Self::Object(lhs), Self::Object(rhs)) => core::ptr::eq(lhs, rhs),
            (Self::Regex(lhs, _), Self::Regex(rhs, _)) => core::ptr::eq(lhs, rhs),
            (Self::Value(lhs), Self::Value(rhs)) => match (lhs, rhs) {
                (Value::Null, Value::Null) => true,
                (Value::Bool(lhs), Value::Bool(rhs)) => lhs == rhs,
                (Value::Number(lhs), Value::Number(rhs)) => lhs == rhs,
                (Value::Str(lhs), Value::Str(rhs)) => lhs == rhs,
                (Value::Array(lhs), Value::Array(rhs)) => core::ptr::eq(lhs, rhs),
                (Value::Object(lhs), Value::Object(rhs)) => core::ptr::eq(lhs, rhs),
                _ => false,
            },
            _ => false,
        }
    }
}

impl<'s> ValueExt<'s> for VmValue<'s> {
    fn as_cow(&self) -> Option<&Cow<'s, str>> {
        match self {
            Self::Value(value) => value.as_cow(),
            _ => None,
        }
    }

    fn fuzzy_cmp(&self, other: &Self) -> Option<Ordering> {
        self.to_primitive_number()
            .fuzzy_cmp(&other.to_primitive_number())
    }

    fn fuzzy_eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Array(lhs), Self::Array(rhs)) => core::ptr::eq(lhs, rhs),
            (Self::Date(lhs), Self::Date(rhs)) => core::ptr::eq(lhs, rhs),
            (Self::Object(lhs), Self::Object(rhs)) => core::ptr::eq(lhs, rhs),
            (Self::Regex(lhs, _), Self::Regex(rhs, _)) => core::ptr::eq(lhs, rhs),
            (Self::Value(Value::Number(n)), other) | (other, Self::Value(Value::Number(n))) => {
                other.to_f64() == n.as_f64().unwrap_or(f64::NAN)
            }
            (Self::Value(lhs), Self::Value(rhs)) => lhs.fuzzy_eq(rhs),
            (lhs, rhs) => ValueExt::to_string(lhs) == ValueExt::to_string(rhs),
        }
    }

    fn fuzzy_total_cmp(&self, other: &Self) -> Ordering {
        self.to_primitive_number()
            .fuzzy_total_cmp(&other.to_primitive_number())
    }

    #[inline]
    fn get_mut<'a>(&'a mut self, key: &str) -> Option<&'a mut Value<'s>> {
        if let Self::Value(value) = self {
            value.as_object_mut().and_then(|object| object.get_mut(key))
        } else {
            None
        }
    }

    fn insert<K, V>(&mut self, key: K, value: V) -> Option<Self>
    where
        K: Into<serde_json_borrow::KeyStrType<'s>>,
        V: Into<Self>,
        Self: Sized,
    {
        match self {
            Self::Object(inner) => inner.insert(key.into(), value.into()),
            Self::Value(inner) => match value.into() {
                Self::Value(value) => inner.insert(key, value),
                value => inner.insert(key, value.to_primitive_number().into_owned()),
            }
            .map(Self::Value),
            _ => None,
        }
    }

    fn into_bool(self) -> bool {
        match self {
            Self::Value(value) => value.into_bool(),
            _ => true,
        }
    }

    fn into_f64(self) -> f64 {
        match self {
            Self::Value(value) => value.into_f64(),
            _ => f64::NAN,
        }
    }

    fn into_string(self) -> Cow<'s, str> {
        match self {
            Self::Value(value) => value.into_string(),
            _ => ValueExt::to_string(&self),
        }
    }

    fn to_bool(&self) -> bool {
        match self {
            Self::Value(value) => value.to_bool(),
            _ => true,
        }
    }

    fn to_date(&self, utc: bool) -> Result<DateTime> {
        Ok(if let Self::Date(date) = self {
            *date
        } else if let Some(n) = self.as_f64() {
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
            Self::Value(value) => value.to_f64(),
            _ => f64::NAN,
        }
    }

    fn to_string(&self) -> Cow<'s, str> {
        match self {
            Self::Array(values) => {
                let mut out = String::new();
                let _ = array_to_str(&mut out, values);
                out.into()
            }
            Self::Callable(_) => Cow::Borrowed("function () { [native code] }"),
            Self::Date(date) => date.format(r"D M Y H:i:s \G\M\TO").unwrap().into(),
            Self::Object(_) => Cow::Borrowed("[object Object]"),
            Self::Regex(regex, flags) => {
                // TODO: Check if the compiled regex retains escapes or not.
                format!("/{}/{flags}", regex.as_str().replace('/', "\\/")).into()
            }
            Self::Value(value) => ValueExt::to_string(value),
        }
    }
}

/// Converts a list of [`VmValue`]s to a string.
fn array_to_str(out: &mut String, values: &[VmValue<'_>]) -> core::fmt::Result {
    let mut first = true;
    for value in values {
        if first {
            first = false;
        } else {
            out.push(',');
        }
        match value {
            VmValue::Array(values) => array_to_str(out, values)?,
            _ => write!(out, "{}", ValueExt::to_string(value))?,
        }
    }
    Ok(())
}

/// Returns true if the given character is a valid non-ASCII non-start ident
/// character.
fn is_ident_rest(c: char) -> bool {
    const ZWNJ: char = '\u{200c}';
    const ZWJ: char = '\u{200d}';
    c == ZWNJ
        || c == ZWJ
        || matches!(
            get_general_category(c),
            SpacingMark | NonspacingMark | DecimalNumber | ConnectorPunctuation
        )
}

/// Returns true if the given character is a valid non-ASCII start ident
/// character.
#[inline]
fn is_ident_start(c: char) -> bool {
    matches!(
        get_general_category(c),
        LowercaseLetter
            | ModifierLetter
            | OtherLetter
            | TitlecaseLetter
            | UppercaseLetter
            | LetterNumber
    )
}

/// Creates a regular expression using the given pattern and flags.
fn make_regex(pattern: &str, flags: &str) -> Result<VmValue<'static>, regex::Error> {
    let mut regex = RegexBuilder::new(pattern);
    if flags.contains('i') {
        regex.case_insensitive(true);
    }
    if flags.contains('m') {
        regex.multi_line(true);
    }
    // TODO: Uhhh. The options here in Rust-land are either bytes or Unicode
    // code points, not surrogate pairs with wacky escapes or Unicode code
    // points.
    // if f.contains("u") {}
    if flags.contains('y') {
        todo!("sticky")
    }

    regex
        .build()
        .map(|r| VmValue::Regex(r, Cow::Owned(flags.into())))
}
