//! Generates the Chinese language conversion tables.

use indexmap::IndexMap;
use std::{
    collections::HashSet,
    env,
    fs::File,
    io::{BufRead as _, BufReader, BufWriter, Write as _},
    path::Path,
};

fn main() {
    let path = Path::new(&env::var("OUT_DIR").unwrap()).join("libwikitext_convert_zhtables.rs");
    let mut out = BufWriter::new(File::create(&path).unwrap());
    let input = BufReader::new(File::open("ZhConversion.txt").unwrap());

    let mut current = HashSet::new();
    let mut han_s = IndexMap::new();
    let mut han_t = IndexMap::new();
    let mut in_table = None::<String>;
    let mut iter = input.lines();
    while let Some(Ok(line)) = iter.next() {
        let line = line.trim_ascii();
        if let Some(name) = &in_table {
            if line == "];" {
                let bonus = if matches!(name.as_str(), "ZH_TO_HK" | "ZH_TO_TW") {
                    Some(&han_t)
                } else if name == "ZH_TO_CN" {
                    Some(&han_s)
                } else {
                    None
                };

                if let Some(bonus) = bonus {
                    for (k, v) in bonus {
                        if !current.contains(k) {
                            writeln!(out, r#"    ("{k}", "{v}"),"#).unwrap();
                        }
                    }
                }

                writeln!(out, "];").unwrap();
                in_table = None;
                current.clear();
            } else if let Some(line) = line
                .strip_prefix("'")
                .and_then(|line| line.strip_suffix("',"))
            {
                let (lhs, rhs) = line.split_once("' => '").unwrap();
                if name == "ZH_TO_HANS" {
                    han_s.insert(lhs.to_owned(), rhs.to_owned());
                } else if name == "ZH_TO_HANT" {
                    han_t.insert(lhs.to_owned(), rhs.to_owned());
                } else {
                    current.insert(lhs.to_owned());
                }
                writeln!(out, r#"    ("{lhs}", "{rhs}"),"#).unwrap();
            } else if !line.is_empty() {
                panic!("weird line: {line}")
            }
        } else if let Some(name) = line
            .strip_prefix("public const ")
            .and_then(|line| line.strip_suffix(" = ["))
        {
            writeln!(out, "pub static {name}: &[(&str, &str)] = &[").unwrap();
            in_table = Some(name.to_owned());
        }
    }

    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=ZhConversion.txt");
}
