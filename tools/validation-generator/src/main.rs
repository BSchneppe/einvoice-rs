//! Thin maintainer-only CLI; archive input is always explicit and offline.
use std::{collections::BTreeMap, path::PathBuf};
use validation_generator::{bundle, emit, source, Error, Result};
fn run() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let command = args.next().ok_or_else(||Error::new("usage: validation-generator inspect|runnable --out DIR --archive NAME=PATH (all five locked archives)"))?;
    if command != "inspect" && command != "runnable" {
        return Err(Error::new("unknown command; only inspect or runnable"));
    }
    let mut paths = BTreeMap::new();
    let mut out = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--archive" => {
                let value = args
                    .next()
                    .ok_or_else(|| Error::new("missing archive value"))?;
                let (name, path) = value
                    .split_once('=')
                    .ok_or_else(|| Error::new("archive must be NAME=PATH"))?;
                if paths.insert(name.into(), PathBuf::from(path)).is_some() {
                    return Err(Error::new("duplicate archive argument"));
                }
            }
            "--out" => {
                if out.is_some() {
                    return Err(Error::new("duplicate output argument"));
                }
                out = Some(PathBuf::from(
                    args.next()
                        .ok_or_else(|| Error::new("missing output directory"))?,
                ));
            }
            _ => return Err(Error::new("unknown argument")),
        }
    }
    let directory = out.ok_or_else(|| Error::new("missing --out"))?;
    let sources = source::Sources::load(&paths)?;
    let inspection = bundle::compile(&sources)?;
    if command == "runnable" {
        emit::runnable(&inspection, &directory)?;
    } else {
        println!(
            "non-executable inspection IR sha256 {}",
            emit::inspect(&inspection, &directory)?
        );
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}
