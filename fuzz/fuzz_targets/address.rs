#![no_main]

use libfuzzer_sys::fuzz_target;
use tsip_parser::Address;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(addr) = Address::parse(s) else {
        return;
    };
    let rendered = addr.to_string();
    let reparsed = Address::parse(&rendered).expect("round-trip parse must succeed");
    let rerendered = reparsed.to_string();
    assert_eq!(rendered, rerendered, "round-trip to_string must be stable");
});
