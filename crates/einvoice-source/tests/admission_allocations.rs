//! Allocation-order regression; counts requested payload, never RSS/live heap.
use einvoice_source::{SourceDocument, SourceError, SourceLimits};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
struct Count;
static ENABLED: AtomicBool = AtomicBool::new(false);
static SIZE: AtomicUsize = AtomicUsize::new(0);
static HITS: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Count {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        if ENABLED.load(Ordering::Relaxed) && l.size() == SIZE.load(Ordering::Relaxed) {
            HITS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: forward the allocator's unchanged layout to System.
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        // SAFETY: p and l originate from this forwarding allocator.
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        if ENABLED.load(Ordering::Relaxed) && n == SIZE.load(Ordering::Relaxed) {
            HITS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: preserve System's pointer/layout contract and requested size.
        unsafe { System.realloc(p, l, n) }
    }
}
#[global_allocator]
static ALLOCATOR: Count = Count;

#[test]
fn refuse_namespace_fanout_before_parser_payload_allocations() {
    // This integration binary has one test, so instrumentation is isolated.
    let header = format!("<?{}?><r/>", "n".repeat(4000));
    let mut variants = vec![
        header.as_bytes().to_vec(),
        [b"\xef\xbb\xbf".as_slice(), header.as_bytes()].concat(),
    ];
    for little in [true, false] {
        let mut raw = if little {
            vec![255, 254]
        } else {
            vec![254, 255]
        };
        for c in header.encode_utf16() {
            raw.extend(if little {
                c.to_le_bytes()
            } else {
                c.to_be_bytes()
            });
        }
        variants.push(raw);
    }
    for raw in variants {
        for limits in [
            SourceLimits {
                string_bytes: 1,
                ..SourceLimits::default()
            },
            SourceLimits {
                storage_bytes: 1,
                ..SourceLimits::default()
            },
        ] {
            SIZE.store(4096, Ordering::Relaxed);
            HITS.store(0, Ordering::Relaxed);
            ENABLED.store(true, Ordering::Relaxed);
            let result = SourceDocument::parse_with_limits(&raw, &limits);
            ENABLED.store(false, Ordering::Relaxed);
            assert!(matches!(result, Err(SourceError::Resource(_))));
            assert_eq!(
                HITS.load(Ordering::Relaxed),
                0,
                "header target allocated before admission"
            );
        }
        SourceDocument::parse(&raw).unwrap();
    }
    for size in [8191, 8193] {
        let uri = format!("urn:{}", "u".repeat(size - 4));
        for inherited in [false, true] {
            let mut xml = if inherited {
                format!("<r xmlns:p='{uri}'><p:c")
            } else {
                format!("<r xmlns:p='{uri}'")
            };
            for i in 0..512 {
                xml.push_str(&format!(" p:a{i}=''"));
            }
            xml.push_str(if inherited { "/></r>" } else { "/>" });
            for limits in [
                SourceLimits {
                    string_bytes: 100000,
                    ..SourceLimits::default()
                },
                SourceLimits {
                    work: 200000,
                    ..SourceLimits::default()
                },
                SourceLimits {
                    storage_bytes: 100000,
                    ..SourceLimits::default()
                },
            ] {
                SIZE.store(size, Ordering::Relaxed);
                HITS.store(0, Ordering::Relaxed);
                ENABLED.store(true, Ordering::Relaxed);
                let result = SourceDocument::parse_with_limits(xml.as_bytes(), &limits);
                ENABLED.store(false, Ordering::Relaxed);
                assert!(matches!(result, Err(SourceError::Resource(_))));
                assert_eq!(
                    HITS.load(Ordering::Relaxed),
                    0,
                    "allocated URI payload before refusal"
                );
            }
            SourceDocument::parse(xml.as_bytes()).unwrap();
        }
    }
}
