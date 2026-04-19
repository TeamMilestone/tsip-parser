#![no_main]

use libfuzzer_sys::fuzz_target;
use tsip_parser::Message;

fuzz_target!(|data: &[u8]| {
    let _ = Message::parse(data);
});
