#![no_main]

use libfuzzer_sys::fuzz_target;
use tsip_parser::Uri;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(uri) = Uri::parse(s) else {
        return;
    };
    let rendered = uri.to_string();
    let reparsed = Uri::parse(&rendered).expect("round-trip parse must succeed");
    let rerendered = reparsed.to_string();
    assert_eq!(rendered, rerendered, "round-trip to_string must be stable");
});
