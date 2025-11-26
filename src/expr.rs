//! Basic floating point mathematical expression evaluation engine.

// Adapted from ExprParser.php in the MediaWiki ParserFunctions extension
// <https://github.com/wikimedia/mediawiki-extensions-ParserFunctions/>
// SPDX-License-Identifier: GPL-2.0-or-later

#![allow(clippy::too_many_lines)]

use crate::php::strtr;
use arrayvec::ArrayVec;
use std::{
    borrow::Cow,
    f64::consts::{E, PI},
    num::ParseFloatError,
};

/// An expression evaluation error.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum Error {
    /// Someone tried to do too much arithmetic at once.
    #[error("stack exhausted at {0}")]
    StackExhausted(usize),
    /// Encountered an unknown token.
    #[error("unknown token '{1}' at {0}")]
    UnknownToken(usize, Cow<'static, str>),
    /// Encountered a number where it shouldn’t’ve been.
    #[error("unexpected number {1} at {0}")]
    UnexpectedNumber(usize, f64),
    /// Encountered an operator where it shouldn’t’ve been.
    #[error("unexpected operator '{1}' at {0}")]
    UnexpectedOperator(usize, Cow<'static, str>),
    /// Encountered a close bracket where it shouldn’t’ve been.
    #[error("unexpected closing bracket at {0}")]
    UnexpectedCloseBracket(usize),
    /// Never encountered a close bracket where it should’ve been.
    #[error("unclosed bracket")]
    MissingCloseBracket,
    /// A required operand was missing.
    #[error("missing operand for '{0}'")]
    MissingOperand(Cow<'static, str>),
    /// Someone tried to do that thing you’re not supposed to do with numbers.
    #[error("division by zero in operator '{0}'")]
    DivisionByZero(Cow<'static, str>),
    /// Encountered a number where it shouldn’t’ve been.
    #[error("invalid argument to operator '{0}'")]
    InvalidArgument(Cow<'static, str>),
    /// Someone tried to invent new maths, and it did not work.
    #[error("result of operator '{0}' is not a number")]
    NotANumber(Cow<'static, str>),
    /// A number that should have been a float turned out to not be a float.
    #[error("could not parse number at {0}: {1}")]
    ParseFloat(usize, ParseFloatError),
}

/// Evaluates a mathematical expression.
///
/// The algorithm here is based on the infix to RPN algorithm by Robert Montante:
/// <https://web.archive.org/web/20100417210626/http://montcs.bloomu.edu/~bobmon/Information/RPN/infix2rpn.shtml>
///
/// It’s essentially the same as Dijkstra’s shunting yard algorithm.
pub fn do_expression(expr: &str) -> Result<Option<f64>, Error> {
    let mut operands = ArrayVec::<f64, MAX_STACK_SIZE>::new_const();
    let mut operators = ArrayVec::<Token, MAX_STACK_SIZE>::new_const();

    let expr = strtr(
        expr,
        &[("&minus;", "-"), ("&lt;", "<"), ("&gt;", ">"), ("−", "-")],
    );

    let mut iter = expr.char_indices().peekable();

    let mut expecting = Kind::Operand;

    while let Some((pos, char)) = iter.next() {
        if operands.len() == MAX_STACK_SIZE || operators.len() == MAX_STACK_SIZE {
            return Err(Error::StackExhausted(pos));
        }

        let op;
        if EXPR_WHITE_CLASS.contains(char) {
            while iter
                .next_if(|(_, char)| EXPR_WHITE_CLASS.contains(*char))
                .is_some()
            {}
            continue;
        } else if EXPR_NUMBER_CLASS.contains(char) {
            let mut sep_count = 0;
            let mut end = pos;
            while let Some((pos, char)) =
                iter.next_if(|(_, char)| EXPR_NUMBER_CLASS.contains(*char))
            {
                // MW will parse "1.2.3.4.5" as 1.2 so it is necessary to
                // track state and ignore everything beyond one separator
                if char == '.' {
                    sep_count += 1;
                }
                if sep_count < 2 {
                    end = pos;
                }
            }

            let number = expr[pos..=end]
                .parse::<f64>()
                .map_err(|err| Error::ParseFloat(pos, err))?;

            if expecting != Kind::Operand {
                return Err(Error::UnexpectedNumber(pos, number));
            }

            operands.push(number);
            expecting = Kind::Operator;
            continue;
        } else if char.is_ascii_alphabetic() {
            let mut end = pos;
            while let Some((pos, _)) = iter.next_if(|(_, char)| char.is_alphabetic()) {
                end = pos;
            }

            let word = &expr[pos..=end];
            if let Some(word_op) = words(&word.to_ascii_lowercase()) {
                op = word_op;
            } else {
                return Err(Error::UnknownToken(pos, word.to_string().into()));
            }

            match op {
                // constant
                Token::Exponent => {
                    if expecting == Kind::Operand {
                        operands.push(1.0_f64.exp());
                        expecting = Kind::Operator;
                        continue;
                    }
                }
                Token::Pi => {
                    if expecting != Kind::Operand {
                        return Err(Error::UnexpectedNumber(pos, PI));
                    }
                    operands.push(PI);
                    expecting = Kind::Operator;
                    continue;
                }

                // Unary operator
                Token::Not
                | Token::Sine
                | Token::Cosine
                | Token::Tangent
                | Token::ArcSine
                | Token::ArcCos
                | Token::ArcTan
                | Token::Exp
                | Token::Ln
                | Token::Abs
                | Token::Floor
                | Token::Trunc
                | Token::Ceil
                | Token::Sqrt => {
                    if expecting != Kind::Operand {
                        return Err(Error::UnexpectedOperator(pos, word.to_string().into()));
                    }
                    operators.push(op);
                    continue;
                }
                _ => {
                    // Binary operator, fall through
                }
            }
        } else if char == '+' {
            if expecting == Kind::Operand {
                // Unary plus
                operators.push(Token::Positive);
                continue;
            }
            // Binary plus
            op = Token::Plus;
        } else if char == '-' {
            if expecting == Kind::Operand {
                // Unary minus
                operators.push(Token::Negative);
                continue;
            }
            // Binary minus
            op = Token::Minus;
        } else if char == '*' {
            op = Token::Times;
        } else if char == '/' {
            op = Token::Divide;
        } else if char == '^' {
            op = Token::Pow;
        } else if char == '(' {
            if expecting == Kind::Operator {
                return Err(Error::UnexpectedOperator(pos, "(".into()));
            }
            operators.push(Token::Open);
            continue;
        } else if char == ')' {
            let mut last_op = operators.last().copied();
            while let Some(op) = last_op
                && op != Token::Open
            {
                do_operation(op, &mut operands)?;
                operators.pop();
                last_op = operators.last().copied();
            }
            if last_op.is_some() {
                operators.pop();
            } else {
                return Err(Error::UnexpectedCloseBracket(pos));
            }
            expecting = Kind::Operator;
            continue;
        } else if char == '=' {
            op = Token::Equality;
        } else if char == '<' {
            if iter.next_if(|(_, char)| *char == '=').is_some() {
                op = Token::LessEq;
            } else if iter.next_if(|(_, char)| *char == '>').is_some() {
                op = Token::NotEq;
            } else {
                op = Token::Less;
            }
        } else if char == '>' {
            if iter.next_if(|(_, char)| *char == '=').is_some() {
                op = Token::GreaterEq;
            } else {
                op = Token::Greater;
            }
        } else if char == '!' && iter.next_if(|(_, char)| *char == '=').is_some() {
            op = Token::NotEq;
        } else {
            return Err(Error::UnknownToken(pos, char.to_string().into()));
        }

        // Binary operator processing
        if expecting == Kind::Operand {
            return Err(Error::UnexpectedOperator(pos, names(op)));
        }

        // Shunting yard magic
        let mut last_op = operators.last().copied();
        while let Some(lop) = last_op
            && precedence(op) <= precedence(lop)
        {
            do_operation(lop, &mut operands)?;
            operators.pop();
            last_op = operators.last().copied();
        }
        operators.push(op);
        expecting = Kind::Operand;
    }

    // Finish off the operator array
    while let Some(op) = operators.pop() {
        if op == Token::Open {
            return Err(Error::MissingCloseBracket);
        }
        do_operation(op, &mut operands)?;
    }

    assert!(
        operands.len() < 2,
        "'{expr}' evaluated to bad number of operands"
    );
    Ok(operands.pop())
}

/// Valid white space characters.
const EXPR_WHITE_CLASS: &str = " \t\r\n";
/// Valid number characters.
const EXPR_NUMBER_CLASS: &str = "0123456789.";

/// Operator tokens.
// Clippy: See [`names`] to learn which token corresponds to which input.
#[allow(clippy::missing_docs_in_private_items)]
#[derive(Clone, Copy, Eq, PartialEq)]
enum Token {
    Negative,
    Positive,
    Plus,
    Minus,
    Times,
    Divide,
    Mod,
    Open,
    And,
    Or,
    Not,
    Equality,
    Less,
    Greater,
    LessEq,
    GreaterEq,
    NotEq,
    Round,
    Exponent,
    Sine,
    Cosine,
    Tangent,
    ArcSine,
    ArcCos,
    ArcTan,
    Exp,
    Ln,
    Abs,
    Floor,
    Trunc,
    Ceil,
    Pow,
    Pi,
    FMod,
    Sqrt,
}

/// Maximum allowed number of in-flight operators or operands.
const MAX_STACK_SIZE: usize = 100;

/// Returns the precdence of the given token.
const fn precedence(t: Token) -> i32 {
    match t {
        Token::Negative | Token::Positive | Token::Exponent => 10,
        Token::Sine
        | Token::Cosine
        | Token::Tangent
        | Token::ArcSine
        | Token::ArcCos
        | Token::ArcTan
        | Token::Exp
        | Token::Ln
        | Token::Abs
        | Token::Floor
        | Token::Trunc
        | Token::Ceil
        | Token::Not
        | Token::Sqrt => 9,
        Token::Pow => 8,
        Token::Times | Token::Divide | Token::Mod | Token::FMod => 7,
        Token::Plus | Token::Minus => 6,
        Token::Round => 5,
        Token::Equality
        | Token::Less
        | Token::Greater
        | Token::LessEq
        | Token::GreaterEq
        | Token::NotEq => 4,
        Token::And => 3,
        Token::Or => 2,
        Token::Pi => 0,
        Token::Open => -1,
    }
}

/// Returns the name of the given token.
const fn names(t: Token) -> Cow<'static, str> {
    Cow::Borrowed(match t {
        Token::Not => "not",
        Token::Times => "*",
        Token::Divide => "/",
        Token::Mod => "mod",
        Token::FMod => "fmod",
        Token::Positive | Token::Plus => "+",
        Token::Negative | Token::Minus => "-",
        Token::Round => "round",
        Token::Equality => "=",
        Token::Less => "<",
        Token::Greater => ">",
        Token::LessEq => "<=",
        Token::GreaterEq => ">=",
        Token::NotEq => "<>",
        Token::And => "and",
        Token::Or => "or",
        Token::Exponent => "e",
        Token::Sine => "sin",
        Token::Cosine => "cos",
        Token::Tangent => "tan",
        Token::ArcSine => "asin",
        Token::ArcCos => "acos",
        Token::ArcTan => "atan",
        Token::Ln => "ln",
        Token::Exp => "exp",
        Token::Abs => "abs",
        Token::Floor => "floor",
        Token::Trunc => "trunc",
        Token::Ceil => "ceil",
        Token::Pow => "^",
        Token::Pi => "pi",
        Token::Sqrt => "sqrt",
        Token::Open => "(",
    })
}

/// Returns a token corresponding to the given word, or `None` if the token is
/// not a known word.
fn words(input: &str) -> Option<Token> {
    Some(match input {
        "mod" => Token::Mod,
        "fmod" => Token::FMod,
        "and" => Token::And,
        "or" => Token::Or,
        "not" => Token::Not,
        "round" => Token::Round,
        "div" => Token::Divide,
        "e" => Token::Exponent,
        "sin" => Token::Sine,
        "cos" => Token::Cosine,
        "tan" => Token::Tangent,
        "asin" => Token::ArcSine,
        "acos" => Token::ArcCos,
        "atan" => Token::ArcTan,
        "exp" => Token::Exp,
        "ln" => Token::Ln,
        "abs" => Token::Abs,
        "trunc" => Token::Trunc,
        "floor" => Token::Floor,
        "ceil" => Token::Ceil,
        "pi" => Token::Pi,
        "sqrt" => Token::Sqrt,
        _ => return None,
    })
}

/// A subexpression kind.
#[derive(PartialEq, Eq)]
enum Kind {
    /// An operand.
    Operand,
    /// An operator.
    Operator,
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::float_cmp,
    clippy::cast_sign_loss
)]
fn do_operation(op: Token, stack: &mut ArrayVec<f64, MAX_STACK_SIZE>) -> Result<(), Error> {
    match op {
        Token::Negative => {
            if let Some(arg) = stack.pop() {
                stack.push(-arg);
            } else {
                return Err(Error::MissingOperand(names(op)));
            }
        }
        Token::Positive => {
            if stack.is_empty() {
                return Err(Error::MissingOperand(names(op)));
            }
        }
        Token::Times => {
            if let (Some(right), Some(left)) = (stack.pop(), stack.pop()) {
                stack.push(left * right);
            } else {
                return Err(Error::MissingOperand(names(op)));
            }
        }
        Token::Divide => {
            if let (Some(right), Some(left)) = (stack.pop(), stack.pop()) {
                if right == 0.0 {
                    return Err(Error::DivisionByZero(names(op)));
                }
                stack.push(left / right);
            } else {
                return Err(Error::MissingOperand(names(op)));
            }
        }
        Token::Mod => {
            if let (Some(right), Some(left)) = (stack.pop(), stack.pop()) {
                if right == 0.0 {
                    return Err(Error::DivisionByZero(names(op)));
                }
                stack.push(((left as i64) % (right as i64)) as f64);
            } else {
                return Err(Error::MissingOperand(names(op)));
            }
        }
        Token::FMod => {
            if let (Some(right), Some(left)) = (stack.pop(), stack.pop()) {
                if right == 0.0 {
                    return Err(Error::DivisionByZero(names(op)));
                }
                stack.push(left.rem_euclid(right));
            } else {
                return Err(Error::MissingOperand(names(op)));
            }
        }
        Token::Plus => {
            if let (Some(right), Some(left)) = (stack.pop(), stack.pop()) {
                stack.push(left + right);
            } else {
                return Err(Error::MissingOperand(names(op)));
            }
        }
        Token::Minus => {
            if let (Some(right), Some(left)) = (stack.pop(), stack.pop()) {
                stack.push(left - right);
            } else {
                return Err(Error::MissingOperand(names(op)));
            }
        }
        Token::And => {
            if let (Some(right), Some(left)) = (stack.pop(), stack.pop()) {
                stack.push(f64::from(left != 0.0 && right != 0.0));
            } else {
                return Err(Error::MissingOperand(names(op)));
            }
        }
        Token::Or => {
            if let (Some(right), Some(left)) = (stack.pop(), stack.pop()) {
                stack.push(f64::from(left != 0.0 || right != 0.0));
            } else {
                return Err(Error::MissingOperand(names(op)));
            }
        }
        Token::Equality => {
            if let (Some(right), Some(left)) = (stack.pop(), stack.pop()) {
                stack.push(f64::from(left == right));
            } else {
                return Err(Error::MissingOperand(names(op)));
            }
        }
        Token::Not => {
            if let Some(arg) = stack.pop() {
                stack.push(f64::from(arg == 0.0));
            } else {
                return Err(Error::MissingOperand(names(op)));
            }
        }
        Token::Round => {
            if let (Some(digits), Some(value)) = (stack.pop(), stack.pop()) {
                let digits = digits as u32;
                // “Rounding to a very large number leads to infinity. Hence,
                // the original value without the infinity is given as the
                // answer.”
                if let Some(y) = 10_i32.checked_pow(digits) {
                    let y = f64::from(y);
                    stack.push((value * y).round() / y);
                } else {
                    stack.push(value);
                }
            } else {
                return Err(Error::MissingOperand(names(op)));
            }
        }
        Token::Less => {
            if let (Some(right), Some(left)) = (stack.pop(), stack.pop()) {
                stack.push(f64::from(left < right));
            } else {
                return Err(Error::MissingOperand(names(op)));
            }
        }
        Token::Greater => {
            if let (Some(right), Some(left)) = (stack.pop(), stack.pop()) {
                stack.push(f64::from(left > right));
            } else {
                return Err(Error::MissingOperand(names(op)));
            }
        }
        Token::LessEq => {
            if let (Some(right), Some(left)) = (stack.pop(), stack.pop()) {
                stack.push(f64::from(left <= right));
            } else {
                return Err(Error::MissingOperand(names(op)));
            }
        }
        Token::GreaterEq => {
            if let (Some(right), Some(left)) = (stack.pop(), stack.pop()) {
                stack.push(f64::from(left >= right));
            } else {
                return Err(Error::MissingOperand(names(op)));
            }
        }
        Token::NotEq => {
            if let (Some(right), Some(left)) = (stack.pop(), stack.pop()) {
                stack.push(f64::from(left != right));
            } else {
                return Err(Error::MissingOperand(names(op)));
            }
        }
        Token::Exponent => {
            if let (Some(right), Some(left)) = (stack.pop(), stack.pop()) {
                stack.push(left * 10.0_f64.powf(right));
            } else {
                return Err(Error::MissingOperand(names(op)));
            }
        }
        Token::Sine => {
            if let Some(arg) = stack.pop() {
                stack.push(arg.sin());
            } else {
                return Err(Error::MissingOperand(names(op)));
            }
        }
        Token::Cosine => {
            if let Some(arg) = stack.pop() {
                stack.push(arg.cos());
            } else {
                return Err(Error::MissingOperand(names(op)));
            }
        }
        Token::Tangent => {
            if let Some(arg) = stack.pop() {
                stack.push(arg.tan());
            } else {
                return Err(Error::MissingOperand(names(op)));
            }
        }
        Token::ArcSine => {
            if let Some(arg) = stack.pop() {
                if !(-1.0..=1.0).contains(&arg) {
                    return Err(Error::InvalidArgument(names(op)));
                }
                stack.push(arg.asin());
            } else {
                return Err(Error::MissingOperand(names(op)));
            }
        }
        Token::ArcCos => {
            if let Some(arg) = stack.pop() {
                if !(-1.0..=1.0).contains(&arg) {
                    return Err(Error::InvalidArgument(names(op)));
                }
                stack.push(arg.acos());
            } else {
                return Err(Error::MissingOperand(names(op)));
            }
        }
        Token::ArcTan => {
            if let Some(arg) = stack.pop() {
                stack.push(arg.atan());
            } else {
                return Err(Error::MissingOperand(names(op)));
            }
        }
        Token::Exp => {
            if let Some(arg) = stack.pop() {
                stack.push(arg.exp());
            } else {
                return Err(Error::MissingOperand(names(op)));
            }
        }
        Token::Ln => {
            if let Some(arg) = stack.pop() {
                if arg <= 0.0 {
                    return Err(Error::InvalidArgument(names(op)));
                }
                stack.push(arg.log(E));
            } else {
                return Err(Error::MissingOperand(names(op)));
            }
        }
        Token::Abs => {
            if let Some(arg) = stack.pop() {
                stack.push(arg.abs());
            } else {
                return Err(Error::MissingOperand(names(op)));
            }
        }
        Token::Floor => {
            if let Some(arg) = stack.pop() {
                stack.push(arg.floor());
            } else {
                return Err(Error::MissingOperand(names(op)));
            }
        }
        Token::Trunc => {
            if let Some(arg) = stack.pop() {
                stack.push(arg.trunc());
            } else {
                return Err(Error::MissingOperand(names(op)));
            }
        }
        Token::Ceil => {
            if let Some(arg) = stack.pop() {
                stack.push(arg.ceil());
            } else {
                return Err(Error::MissingOperand(names(op)));
            }
        }
        Token::Pow => {
            if let (Some(right), Some(left)) = (stack.pop(), stack.pop()) {
                let result = left.powf(right);
                stack.push(result);
            } else {
                return Err(Error::MissingOperand(names(op)));
            }
        }
        Token::Sqrt => {
            if let Some(arg) = stack.pop() {
                let result = arg.sqrt();
                if result.is_nan() {
                    return Err(Error::NotANumber(names(op)));
                }
                stack.push(result);
            } else {
                return Err(Error::MissingOperand(names(op)));
            }
        }
        _ => unreachable!(),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expr() {
        assert_eq!(do_expression("1 or 0"), Ok(Some(1.0)));
        assert_eq!(do_expression("not (1 and 0)"), Ok(Some(1.0)));
        assert_eq!(do_expression("not 0"), Ok(Some(1.0)));
        assert_eq!(do_expression("4 < 5"), Ok(Some(1.0)));
        assert_eq!(do_expression("-5 < 2"), Ok(Some(1.0)));
        assert_eq!(do_expression("-2 <= -2"), Ok(Some(1.0)));
        assert_eq!(do_expression("4 > 3"), Ok(Some(1.0)));
        assert_eq!(do_expression("4 > -3"), Ok(Some(1.0)));
        assert_eq!(do_expression("5 >= 2"), Ok(Some(1.0)));
        assert_eq!(do_expression("2 >= 2"), Ok(Some(1.0)));
        assert_eq!(do_expression("1 != 2"), Ok(Some(1.0)));
        assert_eq!(do_expression("-4 * -4 = 4 * 4"), Ok(Some(1.0)));
        assert_eq!(do_expression("not (1 != 1)"), Ok(Some(1.0)));
        assert_eq!(do_expression("1 + 1"), Ok(Some(2.0)));
        assert_eq!(do_expression("-1 + 1"), Ok(Some(0.0)));
        assert_eq!(do_expression("+1 + 1"), Ok(Some(2.0)));
        assert_eq!(do_expression("4 * 4"), Ok(Some(16.0)));
        assert_eq!(do_expression("(1/3) * 3"), Ok(Some(1.0)));
        assert_eq!(do_expression("3 / 1.5"), Ok(Some(2.0)));
        assert_eq!(do_expression("3 / 0.2"), Ok(Some(15.0)));
        assert_eq!(do_expression("3 / ( 2.0 * 0.1 )"), Ok(Some(15.0)));
        assert_eq!(do_expression("3 / ( 2.0 / 10 )"), Ok(Some(15.0)));
        assert_eq!(do_expression("3 / (- 0.2 )"), Ok(Some(-15.0)));
        assert_eq!(do_expression("3 / abs( 0.2 )"), Ok(Some(15.0)));
        assert_eq!(do_expression("3 mod 2"), Ok(Some(1.0)));
        assert_eq!(do_expression("1e4"), Ok(Some(10000.0)));
        assert_eq!(do_expression("1e-2"), Ok(Some(0.01)));
        assert_eq!(do_expression("4.0 round 0"), Ok(Some(4.0)));
        assert_eq!(do_expression("ceil 4"), Ok(Some(4.0)));
        assert_eq!(do_expression("floor 4"), Ok(Some(4.0)));
        assert_eq!(do_expression("4.5 round 0"), Ok(Some(5.0)));
        assert_eq!(do_expression("4.2 round 0"), Ok(Some(4.0)));
        assert_eq!(do_expression("-4.2 round 0"), Ok(Some(-4.0)));
        assert_eq!(do_expression("-4.5 round 0"), Ok(Some(-5.0)));
        assert_eq!(do_expression("-2.0 round 0"), Ok(Some(-2.0)));
        assert_eq!(do_expression("ceil -3"), Ok(Some(-3.0)));
        assert_eq!(do_expression("floor -6.0"), Ok(Some(-6.0)));
        assert_eq!(do_expression("ceil 4.2"), Ok(Some(5.0)));
        assert_eq!(do_expression("ceil -4.5"), Ok(Some(-4.0)));
        assert_eq!(do_expression("floor -4.5"), Ok(Some(-5.0)));
        assert_eq!(do_expression("abs(-2)"), Ok(Some(2.0)));
        assert_eq!(do_expression("ln(exp(1))"), Ok(Some(1.0)));
        assert_eq!(do_expression("trunc(4.5)"), Ok(Some(4.0)));
        assert_eq!(do_expression("trunc(-4.5)"), Ok(Some(-4.0)));
        assert_eq!(do_expression("123 fmod (2^64-1)"), Ok(Some(123.0)));
        assert_eq!(do_expression("5.7 mod 1.3"), Ok(Some(0.0)));
        assert_eq!(do_expression("5.7 fmod 1.3"), Ok(Some(0.5)));
        // We are apparently slightly more precise than PHP here, to two extra
        // decimal places. Does it matter?
        assert_eq!(do_expression("pi + 1"), Ok(Some(4.141_592_653_589_793)));
        // assert_eq!(do_expression("pi + 1"), Ok(Some(4.141_592_653_589_8)));
        assert_eq!(do_expression("sin(0)"), Ok(Some(0.0)));
        assert_eq!(do_expression("cos(0)"), Ok(Some(1.0)));
        assert_eq!(do_expression("tan(0)"), Ok(Some(0.0)));
        assert_eq!(do_expression("asin(0)"), Ok(Some(0.0)));
        assert_eq!(do_expression("acos(1)"), Ok(Some(0.0)));
        assert_eq!(do_expression("atan(0)"), Ok(Some(0.0)));
        assert_eq!(do_expression("sqrt(4)"), Ok(Some(2.0)));
    }

    #[test]
    fn test_expr_errors() {
        let long_expr = str::repeat("ln(", 1001) + "1" + &str::repeat(")", 1001);

        assert_eq!(Err(Error::StackExhausted(150)), do_expression(&long_expr));
        assert_eq!(Err(Error::UnexpectedNumber(2, 2.0)), do_expression("1 2"));
        assert_eq!(
            Err(Error::UnknownToken(0, "foo".into())),
            do_expression("foo")
        );
        assert_eq!(Err(Error::UnexpectedNumber(2, PI)), do_expression("1 pi"));
        assert_eq!(
            Err(Error::UnexpectedOperator(2, "sin".into())),
            do_expression("1 sin")
        );
        assert_eq!(
            Err(Error::UnexpectedOperator(2, "(".into())),
            do_expression("1 (")
        );
        assert_eq!(
            Err(Error::UnexpectedCloseBracket(5)),
            do_expression("1 + 1)")
        );
        assert_eq!(
            Err(Error::UnknownToken(1, ",".into())),
            do_expression("1, 2")
        );
        assert_eq!(
            Err(Error::UnexpectedOperator(0, "<".into())),
            do_expression("<1")
        );
        assert_eq!(Err(Error::MissingCloseBracket), do_expression("(1"));
        assert_eq!(Err(Error::MissingOperand("-".into())), do_expression("-"));
        assert_eq!(Err(Error::MissingOperand("+".into())), do_expression("+"));
        assert_eq!(Err(Error::MissingOperand("*".into())), do_expression("1*"));
        assert_eq!(Err(Error::MissingOperand("/".into())), do_expression("1/"));
        assert_eq!(Err(Error::DivisionByZero("/".into())), do_expression("1/0"));
        assert_eq!(
            Err(Error::MissingOperand("mod".into())),
            do_expression("1 mod")
        );
        assert_eq!(
            Err(Error::DivisionByZero("mod".into())),
            do_expression("1 mod 0")
        );
        assert_eq!(
            Err(Error::MissingOperand("fmod".into())),
            do_expression("1 fmod")
        );
        assert_eq!(
            Err(Error::DivisionByZero("fmod".into())),
            do_expression("1 fmod 0")
        );
        assert_eq!(Err(Error::MissingOperand("+".into())), do_expression("1+"));
        assert_eq!(Err(Error::MissingOperand("-".into())), do_expression("1-"));
        assert_eq!(
            Err(Error::MissingOperand("and".into())),
            do_expression("1 and")
        );
        assert_eq!(
            Err(Error::MissingOperand("or".into())),
            do_expression("1 or")
        );
        assert_eq!(Err(Error::MissingOperand("=".into())), do_expression("1 ="));
        assert_eq!(
            Err(Error::UnexpectedOperator(2, "not".into())),
            do_expression("1 not")
        );
        assert_eq!(
            Err(Error::MissingOperand("round".into())),
            do_expression("1 round")
        );
        assert_eq!(Err(Error::MissingOperand("<".into())), do_expression("1<"));
        assert_eq!(Err(Error::MissingOperand(">".into())), do_expression("1>"));
        assert_eq!(
            Err(Error::MissingOperand("<=".into())),
            do_expression("1<=")
        );
        assert_eq!(
            Err(Error::MissingOperand(">=".into())),
            do_expression("1>=")
        );
        assert_eq!(
            Err(Error::MissingOperand("<>".into())),
            do_expression("1<>")
        );
        assert_eq!(
            Err(Error::MissingOperand("sin".into())),
            do_expression("sin()")
        );
        assert_eq!(
            Err(Error::MissingOperand("cos".into())),
            do_expression("cos()")
        );
        assert_eq!(
            Err(Error::MissingOperand("tan".into())),
            do_expression("tan()")
        );
        assert_eq!(
            Err(Error::MissingOperand("asin".into())),
            do_expression("asin()")
        );
        assert_eq!(
            Err(Error::InvalidArgument("asin".into())),
            do_expression("asin(3) ")
        );
        assert_eq!(
            Err(Error::MissingOperand("acos".into())),
            do_expression("acos()")
        );
        assert_eq!(
            Err(Error::InvalidArgument("acos".into())),
            do_expression("acos(-1.1)")
        );
        assert_eq!(
            Err(Error::MissingOperand("atan".into())),
            do_expression("atan()")
        );
        assert_eq!(
            Err(Error::MissingOperand("exp".into())),
            do_expression("exp()")
        );
        assert_eq!(
            Err(Error::MissingOperand("ln".into())),
            do_expression("ln()")
        );
        assert_eq!(
            Err(Error::InvalidArgument("ln".into())),
            do_expression("ln(-1)")
        );
        assert_eq!(
            Err(Error::MissingOperand("abs".into())),
            do_expression("abs()")
        );
        assert_eq!(
            Err(Error::MissingOperand("floor".into())),
            do_expression("floor")
        );
        assert_eq!(
            Err(Error::MissingOperand("trunc".into())),
            do_expression("trunc")
        );
        assert_eq!(
            Err(Error::MissingOperand("ceil".into())),
            do_expression("ceil")
        );
        assert_eq!(Err(Error::MissingOperand("^".into())), do_expression("1 ^"));
        assert_eq!(
            Err(Error::MissingOperand("sqrt".into())),
            do_expression("sqrt")
        );
        assert_eq!(
            Err(Error::NotANumber("sqrt".into())),
            do_expression("sqrt(-1)")
        );
    }

    #[test]
    fn test_expr_wiki_rs() {
        assert_eq!(do_expression(""), Ok(None));
        assert_eq!(do_expression(" "), Ok(None));
        assert_eq!(do_expression("1.2.3.4.5"), Ok(Some(1.2)));
        assert_eq!(do_expression("1.9.2 > 1.10.9"), Ok(Some(1.0)));
        assert_eq!(do_expression("1 <> 2"), Ok(Some(1.0)));
        assert_eq!(do_expression("((-1) * 1e10)"), Ok(Some(-10_000_000_000.0)));
        assert_eq!(do_expression("10 round 100"), Ok(Some(10.0)));
    }
}
