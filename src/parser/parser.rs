#![allow(warnings)] // TODO: remove me

use nom::{
    branch::alt,
    bytes::complete::{is_not, tag},
    character::complete::{alpha1, alphanumeric1, char},
    combinator::recognize,
    multi::{many0, many1, separated_list1},
    sequence::{delimited, pair, tuple},
    IResult,
};

use super::ast::*;
use crate::error::{Error, Result};

pub fn parse_query(input: &str) -> Result<AST> {
    let (rest, m) =
        vector_selector(input).map_err(|_| (Error::new("couldn't parse PromQL query")))?;
    assert!(rest.len() == 0);
    Ok(AST::new(NodeKind::VectorSelector(m)))
}

fn vector_selector(input: &str) -> IResult<&str, VectorSelector> {
    // metric_identifier label_matchers | metric_identifier | label_matchers
    let (rest, matchers) = label_matchers(input)?;
    Ok((rest, VectorSelector::new(None, matchers).unwrap())) // TODO: handle unwrap
}

fn label_matchers(input: &str) -> IResult<&str, LabelMatchers> {
    // LEFT_BRACE label_match_list RIGHT_BRACE
    //   | LEFT_BRACE label_match_list COMMA RIGHT_BRACE
    //   | LEFT_BRACE RIGHT_BRACE
    let (rest, m) = alt((
        delimited(tag("{"), many0(label_match_list), tag("}")),
        delimited(tag("{"), many1(label_match_list), tag(",}")),
    ))(input)?;
    Ok((
        rest,
        LabelMatchers::new(m.into_iter().flatten().collect::<Vec<_>>()),
    ))
}

fn label_match_list(input: &str) -> IResult<&str, Vec<LabelMatcher>> {
    // label_match_list COMMA label_matcher | label_matcher
    separated_list1(tag(","), label_matcher)(input)
}

fn label_matcher(input: &str) -> IResult<&str, LabelMatcher> {
    // IDENTIFIER match_op STRING
    let (rest, (label, match_op, value)) =
        tuple((label_identifier, match_op, string_literal))(input)?;
    Ok((rest, LabelMatcher::new(label, match_op, value)))
}

// FIXME: this is way too simplistic... Doesn't even handle escaped chars.
// Use https://github.com/Geal/nom/blob/master/examples/string.rs
fn string_literal(input: &str) -> IResult<&str, String> {
    let (rest, m) = delimited(char('"'), is_not("\""), char('"'))(input)?;
    Ok((rest, String::from(m)))
}

fn label_identifier(input: &str) -> IResult<&str, String> {
    // [a-zA-Z_][a-zA-Z0-9_]*
    let (rest, m) = recognize(pair(
        alt((alpha1, tag("_"))),
        many0(alt((alphanumeric1, tag("_")))),
    ))(input)?;
    Ok((rest, String::from(m)))
}

fn metric_identifier(input: &str) -> IResult<&str, String> {
    // [a-zA-Z_:][a-zA-Z0-9_:]*
    let (rest, m) = recognize(pair(
        alt((alpha1, tag("_"), tag(":"))),
        many0(alt((alphanumeric1, tag("_"), tag(":")))),
    ))(input)?;
    Ok((rest, String::from(m)))
}

fn match_op(input: &str) -> IResult<&str, MatchOp> {
    let (rest, m) = alt((tag("=~"), tag("!~"), tag("!="), tag("=")))(input)?;
    match m {
        "=" => Ok((rest, MatchOp::Eql)),
        "!=" => Ok((rest, MatchOp::Neq)),
        "=~" => Ok((rest, MatchOp::EqlRe)),
        "!~" => Ok((rest, MatchOp::NeqRe)),
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_label_identifier() {
        // valid labels
        assert_eq!(label_identifier("l"), Ok(("", String::from("l"))));
        assert_eq!(label_identifier("l123"), Ok(("", String::from("l123"))));
        assert_eq!(label_identifier("_"), Ok(("", String::from("_"))));
        assert_eq!(label_identifier("_1"), Ok(("", String::from("_1"))));
        assert_eq!(label_identifier("label"), Ok(("", String::from("label"))));
        assert_eq!(label_identifier("_label"), Ok(("", String::from("_label"))));
        assert_eq!(
            label_identifier("label_123"),
            Ok(("", String::from("label_123")))
        );
        assert_eq!(
            label_identifier("label_123_"),
            Ok(("", String::from("label_123_")))
        );
        assert_eq!(
            label_identifier("label_123_{}"),
            Ok(("{}", String::from("label_123_")))
        );

        // invalid labels
        assert!(label_identifier("1").is_err());
        assert!(label_identifier("1_").is_err());
        assert!(label_identifier("123label").is_err());
    }

    #[test]
    fn test_match_op() {
        assert_eq!(match_op("=\"foo\""), Ok(("\"foo\"", MatchOp::Eql)));
        assert_eq!(match_op("!=\"foo\""), Ok(("\"foo\"", MatchOp::Neq)));
        assert_eq!(match_op("=~\"foo\""), Ok(("\"foo\"", MatchOp::EqlRe)));
        assert_eq!(match_op("!~\"foo\""), Ok(("\"foo\"", MatchOp::NeqRe)));
    }

    #[test]
    fn test_label_matcher() {
        assert_eq!(
            label_matcher("foo=\"bar\""),
            Ok(("", LabelMatcher::new("foo", MatchOp::Eql, "bar")))
        );

        assert_eq!(
            label_matcher("foo!~\"123 qux\""),
            Ok(("", LabelMatcher::new("foo", MatchOp::NeqRe, "123 qux")))
        );
    }

    #[test]
    fn test_label_match_list() {
        // valid match lists
        assert_eq!(
            label_match_list("foo!~\"123 qux\""),
            Ok((
                "",
                vec![LabelMatcher::new("foo", MatchOp::NeqRe, "123 qux")]
            ))
        );

        assert_eq!(
            label_match_list("foo!~\"123 qux\","),
            Ok((
                ",",
                vec![LabelMatcher::new("foo", MatchOp::NeqRe, "123 qux")]
            ))
        );

        assert_eq!(
            label_match_list("foo!~\"123 qux\",bar=\"42\""),
            Ok((
                "",
                vec![
                    LabelMatcher::new("foo", MatchOp::NeqRe, "123 qux"),
                    LabelMatcher::new("bar", MatchOp::Eql, "42"),
                ]
            ))
        );

        // invalid match lists
        assert!(label_match_list("").is_err());
        assert!(label_match_list(",").is_err());
        assert!(label_match_list(",,").is_err());
        assert!(label_match_list(", ,").is_err());
    }

    #[test]
    fn test_label_matchers() {
        // valid matchers
        assert_eq!(label_matchers("{}"), Ok(("", LabelMatchers::new(vec![]))));
        assert_eq!(
            label_matchers("{} or"),
            Ok((" or", LabelMatchers::new(vec![])))
        );

        assert_eq!(
            label_matchers("{foo!~\"123 qux\"}"),
            Ok((
                "",
                LabelMatchers::new(vec![LabelMatcher::new("foo", MatchOp::NeqRe, "123 qux")])
            ))
        );

        assert_eq!(
            label_matchers("{foo!~\"123 qux\",}"),
            Ok((
                "",
                LabelMatchers::new(vec![LabelMatcher::new("foo", MatchOp::NeqRe, "123 qux")])
            ))
        );

        assert_eq!(
            label_matchers("{foo!~\"123 qux\",bar=\"42\"}"),
            Ok((
                "",
                LabelMatchers::new(vec![
                    LabelMatcher::new("foo", MatchOp::NeqRe, "123 qux"),
                    LabelMatcher::new("bar", MatchOp::Eql, "42"),
                ])
            ))
        );

        // invalid matchers
        assert!(label_matchers("{,}").is_err());
        assert!(label_matchers("{foo!~\"123 qux\",,}").is_err());
    }
}