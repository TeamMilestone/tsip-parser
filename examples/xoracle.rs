use tsip_parser::{Address, Uri};

fn main() {
    let inputs: &[&str] = &[
        "sip:%40alice@host",
        "sip:%3Calice@host",
        "sip:al%25ice@host",
        "sip:alice@host;=val",
        "sip:alice@host;",
        "sip:alice@host;foo=",
        "sip:alice@host?subject=hi%20there",
        "sip:alice@host?=",
        "<sip:alice@host>;tag=",
        "  sip:alice@[::1]:5060  ",
        "sip:alice@host;transport= TCP",
        "sip:alice@host?key= val",
        "sip:alice@host;<evil>=1",
        "<sip:alice@host>;?=bad",
    ];

    for inp in inputs {
        if inp.trim_start().starts_with('<') {
            match Address::parse(inp) {
                Ok(a) => {
                    let rendered = a.to_string();
                    let rt = Address::parse(&rendered);
                    let rt_stable = match &rt {
                        Ok(a2) => a2.to_string() == rendered,
                        Err(_) => false,
                    };
                    println!("RUST_OK  {:?} (via Address)", inp);
                    println!(
                        "  => display={:?} uri={:?} params={:?}",
                        a.display_name, a.uri, a.params
                    );
                    println!("  => to_s={:?}", rendered);
                    println!("  => rt_stable={}", rt_stable);
                }
                Err(e) => println!("RUST_ERR {:?} (via Address) => {:?}", inp, e),
            }
        } else {
            match Uri::parse(inp) {
                Ok(u) => {
                    let rendered = u.to_string();
                    let rt = Uri::parse(&rendered);
                    let rt_stable = match &rt {
                        Ok(u2) => u2.to_string() == rendered,
                        Err(_) => false,
                    };
                    println!("RUST_OK  {:?}", inp);
                    println!(
                        "  => user={:?} host={:?} params={:?} headers={:?}",
                        u.user, u.host, u.params, u.headers
                    );
                    println!("  => to_s={:?}", rendered);
                    println!("  => rt_stable={}", rt_stable);
                }
                Err(e) => println!("RUST_ERR {:?} => {:?}", inp, e),
            }
        }
    }
}
