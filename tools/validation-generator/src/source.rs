//! Explicit offline archives. Verification precedes XML ingestion; nothing is extracted.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read},
    path::PathBuf,
};
use validation_ir::Location;
pub const ORACLE_LOCK: &str = include_str!("../../oracle/artifacts.lock.json");
const MAX_ARCHIVE: u64 = 128 * 1024 * 1024;
const MAX_MEMBER: u64 = 64 * 1024 * 1024;
const MAX_TOTAL: u64 = 512 * 1024 * 1024;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pin {
    pub file: String,
    pub version: String,
    pub sha256: String,
    pub members: Option<BTreeMap<String, String>>,
}
#[derive(Deserialize)]
struct Lock {
    format: u32,
    artifacts: Vec<Pin>,
}
#[derive(Debug)]
pub struct Archive {
    pub(crate) pin: Pin,
    pub(crate) members: BTreeMap<String, Vec<u8>>,
}
#[derive(Debug)]
pub struct Sources {
    pub(crate) archives: BTreeMap<String, Archive>,
}
pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn safe_member(name: &str) -> Result<()> {
    if name.is_empty()
        || name.starts_with('/')
        || name.contains(['\\', ':', '\0'])
        || name
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
    {
        return Err(Error::new(format!("unsafe archive member {name:?}")));
    }
    Ok(())
}
/// Verify an authored fixture archive or an exact authoritative production pin.
pub fn verify_archive(bytes: Vec<u8>, pin: Pin) -> Result<Archive> {
    if bytes.len() as u64 > MAX_ARCHIVE || hash(&bytes) != pin.sha256 {
        return Err(Error::new(format!(
            "archive hash/size mismatch: {}",
            pin.file
        )));
    }
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes))?;
    if zip.len() > 20_000 {
        return Err(Error::new("archive member budget exceeded"));
    }
    let mut members = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut total = 0u64;
    for index in 0..zip.len() {
        let mut member = zip.by_index(index)?;
        let name = member.name().to_owned();
        let canonical_name = if member.is_dir() {
            name.strip_suffix('/').unwrap_or(&name)
        } else {
            &name
        };
        safe_member(canonical_name)?;
        if !seen.insert(canonical_name.to_owned()) {
            return Err(Error::new(format!("duplicate archive entry {name}")));
        }
        if member.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
            return Err(Error::new("symlink archive member"));
        }
        if member.is_dir() {
            if member.size() != 0 {
                return Err(Error::new("nonempty archive directory"));
            }
            continue;
        }
        if member.size() > MAX_MEMBER {
            return Err(Error::new("member size budget exceeded"));
        }
        total = total
            .checked_add(member.size())
            .ok_or_else(|| Error::new("archive size overflow"))?;
        if total > MAX_TOTAL {
            return Err(Error::new("uncompressed archive budget exceeded"));
        }
        if let Some(pins) = &pin.members {
            if !pins.contains_key(&name) {
                return Err(Error::new(format!("unlocked member {name}")));
            }
        }
        let mut data = Vec::new();
        (&mut member).take(MAX_MEMBER + 1).read_to_end(&mut data)?;
        if data.len() as u64 != member.size() || data.len() as u64 > MAX_MEMBER {
            return Err(Error::new("member length mismatch"));
        }
        if let Some(pins) = &pin.members {
            if pins[&name] != hash(&data) {
                return Err(Error::new(format!("member hash mismatch: {name}")));
            }
        }
        members.insert(name, data);
    }
    if let Some(pins) = &pin.members {
        if pins.keys().ne(members.keys()) {
            return Err(Error::new(format!("missing member in {}", pin.file)));
        }
    }
    Ok(Archive { pin, members })
}
impl Sources {
    pub fn load(paths: &BTreeMap<String, PathBuf>) -> Result<Self> {
        let lock: Lock = serde_json::from_str(ORACLE_LOCK)?;
        if lock.format != 1 {
            return Err(Error::new("unsupported oracle lock format"));
        }
        let required = [
            "cen.zip",
            "xr.zip",
            "config.zip",
            "validator.jar",
            "cen-source.zip",
        ];
        if paths.len() != required.len() || required.iter().any(|key| !paths.contains_key(*key)) {
            return Err(Error::new("supply exactly cen.zip, xr.zip, config.zip, validator.jar, cen-source.zip offline paths"));
        }
        let mut archives = BTreeMap::new();
        for name in required {
            let pin = lock
                .artifacts
                .iter()
                .find(|p| p.file == name)
                .ok_or_else(|| Error::new("missing authoritative archive pin"))?
                .clone();
            let path = &paths[name];
            if std::fs::metadata(path)?.len() > MAX_ARCHIVE {
                return Err(Error::new("archive input budget exceeded"));
            }
            let mut bytes = Vec::new();
            std::fs::File::open(path)?
                .take(MAX_ARCHIVE + 1)
                .read_to_end(&mut bytes)?;
            archives.insert(name.to_owned(), verify_archive(bytes, pin)?);
        }
        Ok(Self { archives })
    }
    pub fn bytes(&self, archive: &str, member: &str) -> Result<&[u8]> {
        self.archives
            .get(archive)
            .and_then(|a| a.members.get(member))
            .map(Vec::as_slice)
            .ok_or_else(|| Error::new(format!("missing allowed resource {archive}:{member}")))
    }
}
pub fn xml(text: &str) -> Result<roxmltree::Document<'_>> {
    Ok(roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: 250_000,
        },
    )?)
}
pub fn location(node: roxmltree::Node<'_, '_>, resource: &str, attr: Option<&str>) -> Location {
    let range = attr
        .and_then(|a| node.attribute_node(a).map(|a| a.range()))
        .unwrap_or_else(|| node.range());
    let pos = node.document().text_pos_at(range.start);
    Location {
        resource: resource.into(),
        xml_byte_start: range.start,
        xml_byte_end: range.end,
        line: pos.row,
        column: pos.col,
        attribute: attr.map(str::to_owned),
    }
}
/// Relative references resolve only inside an explicit resource allowlist.
/// The first component of a qualified resource ID is its archive/bundle boundary.
pub fn relative(from: &str, href: &str, allowed: &BTreeSet<String>) -> Result<String> {
    if href.is_empty() || href.starts_with('/') || href.contains([':', '\\', '?', '#', '\0']) {
        return Err(Error::new(format!("forbidden resource reference {href}")));
    }
    let mut parts: Vec<&str> = from.split('/').collect();
    parts.pop();
    for p in href.split('/') {
        match p {
            "" | "." => (),
            ".." => {
                if parts.len() <= 1 {
                    return Err(Error::new("resource escapes archive/bundle boundary"));
                }
                parts.pop();
            }
            _ => parts.push(p),
        }
    }
    let result = parts.join("/");
    safe_member(&result)?;
    if !allowed.contains(&result) {
        return Err(Error::new(format!(
            "reference outside explicit closure: {result}"
        )));
    }
    Ok(result)
}
