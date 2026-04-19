use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    Empty,
    UnterminatedBracket,
    UnterminatedQuote,
    UnterminatedAngle,
    InvalidScheme,
    InvalidUtf8,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Empty => f.write_str("empty input"),
            ParseError::UnterminatedBracket => f.write_str("unterminated '[' in host"),
            ParseError::UnterminatedQuote => f.write_str("unterminated quoted string"),
            ParseError::UnterminatedAngle => f.write_str("unterminated '<' in address"),
            ParseError::InvalidScheme => f.write_str("invalid SIP URI scheme"),
            ParseError::InvalidUtf8 => f.write_str("pct-decoded bytes are not valid UTF-8"),
        }
    }
}

impl std::error::Error for ParseError {}
