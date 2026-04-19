//! RFC 3261 SIP URI (§19.1) and Address (§25.1) parser/serializer.
//!
//! Ported from tsip-core's Ruby byte-scan implementation. Pure-Rust, no
//! external dependencies, no unsafe code. Intended as the native backend
//! for future Ruby (magnus/rb-sys) or other FFI bindings.

pub mod address;
pub mod error;
pub mod message;
pub mod scan;
pub mod uri;

pub use address::{Address, ADDRESS_PARAMS};
pub use error::ParseError;
pub use message::{Message, StartLine};
pub use uri::Uri;
