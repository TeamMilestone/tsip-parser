use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    Empty,
    UnterminatedBracket,
    UnterminatedQuote,
    UnterminatedAngle,
    InvalidScheme,
    InvalidUtf8,
    InvalidHost,
    MessageTooLarge,
    EmptyMessage,
    InvalidStartLine,
    InvalidStatusCode,
    HeaderMissingColon,
    NegativeContentLength,
    OversizeContentLength,
    BadContentLength,
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
            ParseError::InvalidHost => f.write_str("host contains forbidden character"),
            ParseError::MessageTooLarge => f.write_str("SIP message exceeds MAX_SIZE"),
            ParseError::EmptyMessage => f.write_str("empty SIP message"),
            ParseError::InvalidStartLine => f.write_str("invalid SIP start line"),
            ParseError::InvalidStatusCode => f.write_str("invalid SIP status code"),
            ParseError::HeaderMissingColon => f.write_str("header line missing ':'"),
            ParseError::NegativeContentLength => f.write_str("negative Content-Length"),
            ParseError::OversizeContentLength => f.write_str("Content-Length exceeds MAX_SIZE"),
            ParseError::BadContentLength => f.write_str("malformed Content-Length value"),
        }
    }
}

impl std::error::Error for ParseError {}
