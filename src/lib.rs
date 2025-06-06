use std::collections::BTreeSet;

use logos::{Lexer, Logos, Span};
use serde::{Deserialize, Serialize};

use thiserror::Error;

/// A selector expression with existing operations
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum Expression {
    /// Key exists and in set
    In(String, BTreeSet<String>),

    /// Key does not exists or not in set
    NotIn(String, BTreeSet<String>),

    /// Key exists and is equal
    Equal(String, String),

    /// Key does not exists or is not equal
    NotEqual(String, String),

    /// Key exists
    Exists(String),

    /// Key does not exist
    DoesNotExist(String),
}

#[cfg(feature = "kube-rs")]
impl Into<kube::core::Expression> for Expression {
    fn into(self) -> kube::core::Expression {
        match self {
            Expression::In(key, btree_set) => kube::core::Expression::In(key, btree_set),
            Expression::NotIn(key, btree_set) => kube::core::Expression::NotIn(key, btree_set),
            Expression::Equal(key, value) => kube::core::Expression::Equal(key, value),
            Expression::NotEqual(key, value) => kube::core::Expression::NotEqual(key, value),
            Expression::Exists(key) => kube::core::Expression::Exists(key),
            Expression::DoesNotExist(key) => kube::core::Expression::DoesNotExist(key),
        }
    }
}

/// Indicates failure of conversion to Expression
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("failed to parse value as expression: '{0}' at {1:?}")]
    StringParse(String, Span),
}

type Result<T> = std::result::Result<T, ParseError>;

pub struct Expressions(Vec<ParsedExpression>);

impl IntoIterator for Expressions {
    type Item = ParsedExpression;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[derive(Logos, Debug, PartialEq)]
#[logos(skip r"[, \t\n\f]+")]
pub enum ParsedExpression {
    #[regex(r"[-./\w]+\s+in\s+\([-.\w\s,]+\)", |lex| parse_set(lex.slice()))]
    #[regex(r"[-./\w]+\s+notin\s+\([-.\w\s,]+\)", |lex| parse_set(lex.slice()))]
    #[regex(r"\![-./\w]+", |lex| parse_set(lex.slice()))]
    #[regex(r"[-./\w]+", |lex| parse_set(lex.slice()))]
    #[regex(r"[-./\w]+\s*=\s*[-.\w]+", |lex| parse_equality(lex.slice()))]
    #[regex(r"[-./\w]+\s*==\s*[-.\w]+", |lex| parse_equality(lex.slice()))]
    #[regex(r"[-./\w]+\s*!=\s*[-.\w]+", |lex| parse_equality(lex.slice()))]
    Expression(Expression),
}

impl TryFrom<String> for Expressions {
    type Error = ParseError;

    fn try_from(selector: String) -> Result<Self> {
        let mut lexer = ParsedExpression::lexer(selector.as_str());
        let mut expressions = vec![];
        while let Some(value) = parse_expression(&mut lexer)? {
            expressions.push(value);
        }

        Ok(Expressions(expressions))
    }
}

#[derive(Logos, Debug, PartialEq)]
#[logos(skip r"[ \t\n\f]+")]
enum EqualityToken {
    #[token("=")]
    #[token("==")]
    Equal,
    #[token("!=")]
    NotEqual,
    #[regex(r"[-./\w]+", |lex| lex.slice().to_owned())]
    Value(String),
}

#[derive(Logos, Debug, PartialEq)]
#[logos(skip r"[, \t\n\f]+")]
enum SetToken {
    #[token("!")]
    Not,

    #[regex(r"\([\w\s,]+\)", |lex| parse_value_list(lex.slice()))]
    ValuesList(Vec<String>),

    #[token("in")]
    In,

    #[token("notin")]
    NotIn,

    #[regex(r"\w+", |lex| lex.slice().to_owned())]
    Value(String),
}

#[derive(Logos, Debug, PartialEq)]
#[logos(skip r"[, \(\)\t\n\f]+")]
enum ValuesListToken {
    #[regex(r"[a-zA-Z_-]+", |lex| lex.slice().to_owned())]
    Value(String),
}

/// Parse selector expression
pub fn parse_expression(
    lexer: &mut Lexer<'_, ParsedExpression>,
) -> Result<Option<ParsedExpression>> {
    lexer
        .next()
        .map(|token| match token {
            Ok(ex) => Ok(ex),
            _ => Err(ParseError::StringParse(
                lexer.slice().to_owned(),
                lexer.span(),
            )),
        })
        .transpose()
}

/// Parse an equality based expression.
fn parse_equality(source: &str) -> Option<Expression> {
    let mut lexer = EqualityToken::lexer(source);
    let key = lexer.next()?.ok()?;
    let op = lexer.next()?.ok()?;
    let value = lexer.next()?.ok()?;
    match (key, op, value) {
        (EqualityToken::Value(key), EqualityToken::Equal, EqualityToken::Value(value)) => {
            Some(Expression::Equal(key, value))
        }
        (EqualityToken::Value(key), EqualityToken::NotEqual, EqualityToken::Value(value)) => {
            Some(Expression::NotEqual(key, value))
        }
        _ => None,
    }
}

/// Parse a set based expression.
fn parse_set(source: &str) -> Option<Expression> {
    let mut lexer = SetToken::lexer(source);
    let key = lexer.next()?.ok()?;
    match key {
        SetToken::Not => match lexer.next()?.ok()? {
            SetToken::Value(value) => Some(Expression::DoesNotExist(value)),
            _ => None,
        },
        SetToken::Value(key) => {
            let op = match lexer.next() {
                Some(op) => op.ok()?,
                None => return Some(Expression::Exists(key)),
            };
            let value = lexer.next()?.ok()?;
            match (op, value) {
                (SetToken::In, SetToken::ValuesList(values)) => Some(Expression::In(
                    key,
                    values.into_iter().collect::<BTreeSet<String>>(),
                )),
                (SetToken::NotIn, SetToken::ValuesList(values)) => Some(Expression::NotIn(
                    key,
                    values.into_iter().collect::<BTreeSet<String>>(),
                )),
                (_, _) => None,
            }
        }
        SetToken::ValuesList(_) | SetToken::In | SetToken::NotIn => None,
    }
}

// Parse a list of values into vector
fn parse_value_list(source: &str) -> Option<Vec<String>> {
    let lexer = ValuesListToken::lexer(source);
    let mut values = vec![];
    for value in lexer {
        values.push(match value.ok()? {
            ValuesListToken::Value(value) => value,
        });
    }

    Some(values)
}

#[cfg(test)]
mod tests {
    use logos::Logos;

    use crate::ParseError;

    use super::Expression;

    use super::{ParsedExpression, parse_expression, parse_value_list};

    #[test]
    fn values_lexer() {
        assert_eq!(
            Some(vec!["a".into(), "b".into(), "c".into()]),
            parse_value_list(" (a,b, c)")
        );
        assert_eq!(Some(vec!["a".into()]), parse_value_list("(a)"));
        assert_eq!(Some(vec![]), parse_value_list("()"));
        assert_eq!(Some(vec![]), parse_value_list(""));
    }

    #[test]
    fn expression_lexer() {
        let data = "a==b,,foo.bar.baz/b-y_.6=c_8.-z,c!=d,a in (a,b, c), a notin (a), c,!a,a()d";
        let mut lexer = ParsedExpression::lexer(data);
        assert_eq!(
            Some(ParsedExpression::Expression(Expression::Equal(
                "a".into(),
                "b".into()
            ))),
            parse_expression(&mut lexer).unwrap()
        );
        assert_eq!(
            Some(ParsedExpression::Expression(Expression::Equal(
                "foo.bar.baz/b-y_.6".into(),
                "c_8.-z".into()
            ))),
            parse_expression(&mut lexer).unwrap()
        );
        assert_eq!(
            Some(ParsedExpression::Expression(Expression::NotEqual(
                "c".into(),
                "d".into()
            ))),
            parse_expression(&mut lexer).unwrap()
        );
        assert_eq!(
            Some(ParsedExpression::Expression(Expression::In(
                "a".into(),
                ["a".into(), "b".into(), "c".into()].into()
            ))),
            parse_expression(&mut lexer).unwrap()
        );
        assert_eq!(
            Some(ParsedExpression::Expression(Expression::NotIn(
                "a".into(),
                ["a".into()].into()
            ))),
            parse_expression(&mut lexer).unwrap()
        );
        assert_eq!(
            Some(ParsedExpression::Expression(Expression::Exists("c".into()))),
            parse_expression(&mut lexer).unwrap()
        );
        assert_eq!(
            Some(ParsedExpression::Expression(Expression::DoesNotExist(
                "a".into()
            ))),
            parse_expression(&mut lexer).unwrap()
        );
        assert_eq!(
            Some(ParsedExpression::Expression(Expression::Exists("a".into()))),
            parse_expression(&mut lexer).unwrap()
        );
        assert_eq!(
            Err(ParseError::StringParse("(".into(), 71..72)),
            parse_expression(&mut lexer)
        );
        assert_eq!(
            Err(ParseError::StringParse(")".into(), 72..73)),
            parse_expression(&mut lexer)
        );
        assert_eq!(
            Some(ParsedExpression::Expression(Expression::Exists("d".into()))),
            parse_expression(&mut lexer).unwrap()
        );
        assert_eq!(None, parse_expression(&mut lexer).unwrap());
    }
}
