//! Development-only supplied-resource runner. No archive ingestion in the library.
use einvoice_schema::{
    sha256, with_epoch, CiiBundle, RequestControl, RequestLimits, SourceDocument, CII_RESOURCES,
};
use serde::Serialize;
use std::{
    error::Error,
    fs::OpenOptions,
    io::{self, Read},
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::Arc,
};

/// Admit one regular file before materializing it. Unix opens are nonblocking
/// and do not follow symlinks, so a substituted FIFO cannot stall at open.
/// This is a byte bound and file-kind policy, not a deadline for filesystem I/O.
fn read_regular(path: &Path, limit: usize) -> io::Result<Vec<u8>> {
    let reject = || io::Error::new(io::ErrorKind::InvalidInput, "bounded regular file required");
    if !std::fs::symlink_metadata(path)?.file_type().is_file() {
        return Err(reject());
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > u64::try_from(limit).map_err(|_| reject())? {
        return Err(reject());
    }
    let admitted = limit.checked_add(1).ok_or_else(reject)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(admitted)
        .map_err(|_| io::Error::new(io::ErrorKind::OutOfMemory, "input reservation refused"))?;
    file.take(u64::try_from(admitted).map_err(|_| reject())?)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(reject());
    }
    Ok(bytes)
}
#[derive(Serialize)]
struct Loaded {
    member: &'static str,
    backend_uri: String,
    bytes: usize,
    sha256: String,
}
#[derive(Serialize)]
struct Artifact {
    format: &'static str,
    scope: &'static str,
    schemas: Vec<Loaded>,
    configured_limits: RequestLimits,
    observation: einvoice_schema::Observation,
    epoch_retired: bool,
    epoch_remaining_requests: usize,
    build_receipt: &'static str,
}
fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let directory = PathBuf::from(args.next().ok_or("resource directory required")?);
    let input = PathBuf::from(args.next().ok_or("input XML required")?);
    let count = args
        .next()
        .map(|s| s.into_string().map_err(|_| "non-UTF8 count"))
        .transpose()?
        .map(|s| s.parse::<usize>())
        .transpose()?
        .unwrap_or(1000);
    let work = args
        .next()
        .map(|s| s.into_string().map_err(|_| "non-UTF8 work"))
        .transpose()?
        .map(|s| s.parse::<usize>())
        .transpose()?
        .unwrap_or(100_000_000);
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    let mut schemas = Vec::new();
    let mut resources = Vec::new();
    for r in &CII_RESOURCES {
        let bytes: Arc<[u8]> = read_regular(&directory.join(r.member), r.bytes)?.into();
        schemas.push(Loaded {
            member: r.member,
            backend_uri: format!("bsc8://cii/{}", r.member),
            bytes: bytes.len(),
            sha256: sha256(&bytes),
        });
        resources.push((r.member, bytes));
    }
    let resources = resources
        .try_into()
        .map_err(|_| "four resources required")?;
    let bundle = CiiBundle::from_resources(resources)?;
    let source_limits = einvoice_source::SourceLimits::default();
    let input_bytes = read_regular(&input, source_limits.original_bytes)?;
    let source = SourceDocument::parse_with_limits(&input_bytes, &source_limits)?;
    let limits = RequestLimits {
        input_bytes: 16 * 1024 * 1024,
        nodes: 500_000,
        depth: 128,
        attributes_per_element: 1024,
        declarations_per_element: 256,
        namespaces_per_element: 1024,
        name_bytes: 4096,
        uri_bytes: 16384,
        value_bytes: 16 * 1024 * 1024,
        temporary_bytes: 1024 * 1024,
        work,
        diagnostic_count: count,
        diagnostic_bytes: 16 * 1024 * 1024,
        terminal_bytes: 1024 * 1024,
    };
    let (observation, retired, remaining) =
        with_epoch(&bundle, NonZeroUsize::new(2).expect("finite"), |e| {
            let o = e.validate(&source, &limits, RequestControl::default());
            (o, e.is_retired(), e.remaining_requests())
        })?;
    let artifact = Artifact {
        format: "einvoice-schema-development/1-draft",
        scope: "schema-only; no native-v1, package capability, rule execution or attestation",
        schemas,
        configured_limits: limits,
        observation: observation?,
        epoch_retired: retired,
        epoch_remaining_requests: remaining,
        build_receipt: "external receipt required; expected pins do not identify this executable",
    };
    serde_json::to_writer_pretty(std::io::stdout().lock(), &artifact)?;
    println!();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_exact_and_short_regular_files_but_refuses_oversize_and_nonregular() {
        let dir = std::env::temp_dir().join(format!("bsc8-reader-{}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        let p = dir.join("input");
        for n in [0, 2, 3, 4] {
            std::fs::write(&p, vec![b'x'; n]).unwrap();
            let read = read_regular(&p, 3);
            if n <= 3 {
                assert_eq!(read.unwrap(), vec![b'x'; n]);
            } else {
                assert!(read.is_err());
            }
        }
        assert!(read_regular(&dir, 3).is_err());
        #[cfg(unix)]
        {
            let link = dir.join("link");
            std::os::unix::fs::symlink(&p, &link).unwrap();
            assert!(read_regular(&link, 3).is_err());
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
