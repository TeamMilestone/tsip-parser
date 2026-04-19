use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::panic;
use std::path::Path;

use tsip_parser::{Address, Uri};

#[derive(Debug, Clone, Copy)]
enum Kind {
    Uri,
    Address,
}

#[derive(Debug)]
enum Outcome {
    Stable,
    InitialParseFail,
    ReparseFail { rendered: String },
    Unstable { first: String, second: String },
    Panic,
}

fn probe(kind: Kind, data: &[u8]) -> Outcome {
    let Ok(input) = std::str::from_utf8(data) else {
        return Outcome::Stable;
    };
    let step = panic::catch_unwind(panic::AssertUnwindSafe(|| match kind {
        Kind::Uri => {
            let Ok(uri) = Uri::parse(input) else {
                return Outcome::InitialParseFail;
            };
            let rendered = uri.to_string();
            let Ok(reparsed) = Uri::parse(&rendered) else {
                return Outcome::ReparseFail { rendered };
            };
            let rerendered = reparsed.to_string();
            if rendered == rerendered {
                Outcome::Stable
            } else {
                Outcome::Unstable { first: rendered, second: rerendered }
            }
        }
        Kind::Address => {
            let Ok(addr) = Address::parse(input) else {
                return Outcome::InitialParseFail;
            };
            let rendered = addr.to_string();
            let Ok(reparsed) = Address::parse(&rendered) else {
                return Outcome::ReparseFail { rendered };
            };
            let rerendered = reparsed.to_string();
            if rendered == rerendered {
                Outcome::Stable
            } else {
                Outcome::Unstable { first: rendered, second: rerendered }
            }
        }
    }));
    match step {
        Ok(o) => o,
        Err(_) => Outcome::Panic,
    }
}

fn diff_signature(a: &str, b: &str) -> String {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let prefix = a_bytes
        .iter()
        .zip(b_bytes.iter())
        .take_while(|(x, y)| x == y)
        .count();
    let suffix = a_bytes[prefix..]
        .iter()
        .rev()
        .zip(b_bytes[prefix..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let a_mid = &a[prefix..a_bytes.len() - suffix];
    let b_mid = &b[prefix..b_bytes.len() - suffix];
    format!("[{}|{}] -> [{}|{}]", prefix, a_bytes.len() - prefix - suffix, escape(a_mid), escape(b_mid))
        + &format!(
            " | second-first: {:?}",
            diff_kind(a_mid, b_mid)
        )
}

fn diff_kind(a_mid: &str, b_mid: &str) -> &'static str {
    if a_mid.is_empty() && !b_mid.is_empty() {
        "added"
    } else if !a_mid.is_empty() && b_mid.is_empty() {
        "removed"
    } else {
        "replaced"
    }
}

fn escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_control() || c == '\\' || c == '"' {
            out.push_str(&format!("\\u{{{:x}}}", c as u32));
        } else {
            out.push(c);
        }
    }
    out
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: cluster_crashes <uri|address> <artifacts_dir>");
        std::process::exit(2);
    }
    let kind = match args[1].as_str() {
        "uri" => Kind::Uri,
        "address" => Kind::Address,
        other => {
            eprintln!("unknown kind: {}", other);
            std::process::exit(2);
        }
    };
    let dir = Path::new(&args[2]);

    let mut clusters: BTreeMap<String, (usize, String, String)> = BTreeMap::new();
    let mut total = 0usize;
    let mut stable = 0usize;
    let mut initial_fail = 0usize;
    let mut panics = 0usize;

    for entry in fs::read_dir(dir).expect("read artifacts dir") {
        let entry = entry.expect("entry");
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if !name.starts_with("crash-") {
            continue;
        }
        total += 1;
        let data = fs::read(&path).expect("read");
        let outcome = probe(kind, &data);
        match outcome {
            Outcome::Stable => stable += 1,
            Outcome::InitialParseFail => initial_fail += 1,
            Outcome::Panic => panics += 1,
            Outcome::ReparseFail { rendered } => {
                let sig = format!("REPARSE_FAIL: rendered={:?}", escape(&rendered));
                let sig_key = format!("ZZZ_{}", sig);
                clusters
                    .entry(sig_key)
                    .and_modify(|c| c.0 += 1)
                    .or_insert((1, name.clone(), format!("rendered=\"{}\"", escape(&rendered))));
            }
            Outcome::Unstable { first, second } => {
                let sig = diff_signature(&first, &second);
                clusters
                    .entry(sig)
                    .and_modify(|c| c.0 += 1)
                    .or_insert((
                        1,
                        name.clone(),
                        format!("first=\"{}\" second=\"{}\"", escape(&first), escape(&second)),
                    ));
            }
        }
    }

    let mut sorted: Vec<_> = clusters.into_iter().collect();
    sorted.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));

    println!("== summary ==");
    println!("total      = {}", total);
    println!("stable     = {} (no bug on replay)", stable);
    println!("init_fail  = {} (parse rejected input; no bug)", initial_fail);
    println!("panics     = {}", panics);
    println!("unstable clusters = {}", sorted.len());
    println!();
    println!("== clusters (top by count) ==");
    for (sig, (count, example, detail)) in sorted.iter().take(20) {
        println!("[{:3}] {}", count, sig);
        println!("      example  : {}", example);
        println!("      {}", detail);
    }
}
