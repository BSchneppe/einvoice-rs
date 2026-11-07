use crate::error::{limit, Result};
use crate::SourceError;
use std::mem::size_of;
use std::sync::atomic::{AtomicBool, Ordering};

/// Finite logical construction safeguards, not an RSS or hard deadline policy.
/// Counts include temporary admission records. Storage charges logical payloads
/// before reservation, not allocator capacity/overhead. Parser/Arc internals can
/// still abort on allocator failure. See the crate README for the contract.
#[derive(Debug, Clone, Copy)]
pub struct SourceLimits {
    /// Original input bytes, checked before copying (default 16 MiB).
    pub original_bytes: usize,
    /// Decoded UTF-8 bytes, excluding BOM.
    pub decoded_bytes: usize,
    /// Normalized UTF-8 and re-encoded parser bytes, each independently capped.
    pub normalized_bytes: usize,
    /// Header probe bytes, including any leading lexical material it consumes.
    pub header_bytes: usize,
    /// Maximum lexical token count (including declaration).
    pub lexical_tokens: usize,
    /// Maximum ordinary nodes, including document and text nodes.
    pub nodes: usize,
    /// Simultaneously open elements (default 128).
    pub depth: usize,
    /// Per lexical token bytes (CDATA included before parser allocation).
    pub token_bytes: usize,
    /// Per QName/prefix/local name UTF-8 bytes.
    pub name_bytes: usize,
    /// Per namespace URI, bounded lexically before expansion and semantically.
    pub uri_bytes: usize,
    /// Per raw or expanded value UTF-8 bytes, including a coalesced text node.
    pub value_bytes: usize,
    /// Attributes on one element INCLUDING namespace declarations.
    pub attributes: usize,
    /// Local namespace declarations per element.
    pub local_namespaces: usize,
    /// Effective namespace prefixes, conservatively including undeclarations.
    pub effective_namespaces: usize,
    /// Cumulative namespace frame walks and parser snapshot payload estimate.
    pub namespace_bytes: usize,
    /// Cumulative retained/copied string bytes (temporary views included).
    pub string_bytes: usize,
    /// Cumulative lexical piece associations.
    pub pieces: usize,
    /// Cumulative logical record/map/buffer payload bytes.
    pub storage_bytes: usize,
    /// Cumulative admitted expanded-value upper bounds.
    pub expanded_bytes: usize,
    /// Cumulative scanned bytes, tokens, events, comparisons and frame walks.
    pub work: usize,
}
impl Default for SourceLimits {
    fn default() -> Self {
        Self {
            original_bytes: 16 << 20,
            decoded_bytes: 32 << 20,
            normalized_bytes: 64 << 20,
            header_bytes: 64 << 10,
            lexical_tokens: 1_000_000,
            nodes: 500_000,
            depth: 128,
            token_bytes: 16 << 20,
            name_bytes: 4096,
            uri_bytes: 16 << 10,
            value_bytes: 16 << 20,
            attributes: 1024,
            local_namespaces: 256,
            effective_namespaces: 1024,
            namespace_bytes: 64 << 20,
            string_bytes: 192 << 20,
            pieces: 1_000_000,
            storage_bytes: 384 << 20,
            expanded_bytes: 64 << 20,
            work: 512 << 20,
        }
    }
}
impl SourceLimits {
    /// Reject zero/unlimited/platform-unsafe bounds before examining source.
    pub fn validate(&self) -> Result<()> {
        let fields = [
            self.original_bytes,
            self.decoded_bytes,
            self.normalized_bytes,
            self.header_bytes,
            self.lexical_tokens,
            self.nodes,
            self.depth,
            self.token_bytes,
            self.name_bytes,
            self.uri_bytes,
            self.value_bytes,
            self.attributes,
            self.local_namespaces,
            self.effective_namespaces,
            self.namespace_bytes,
            self.string_bytes,
            self.pieces,
            self.storage_bytes,
            self.expanded_bytes,
            self.work,
        ];
        if fields.iter().any(|n| *n == 0 || *n > isize::MAX as usize) {
            return Err(SourceError::InvalidLimits(
                "bounds must be nonzero and <= isize::MAX",
            ));
        }
        if self.nodes > u32::MAX as usize || self.lexical_tokens > u32::MAX as usize {
            return Err(SourceError::InvalidLimits("node/token IDs exceed u32"));
        }
        Ok(())
    }
}

pub(crate) struct Meter<'a> {
    pub limits: &'a SourceLimits,
    cancel: Option<&'a AtomicBool>,
    storage: usize,
    strings: usize,
    pieces: usize,
    expanded: usize,
    namespace: usize,
    work: usize,
}
impl<'a> Meter<'a> {
    pub fn new(limits: &'a SourceLimits, cancel: Option<&'a AtomicBool>) -> Self {
        Self {
            limits,
            cancel,
            storage: 0,
            strings: 0,
            pieces: 0,
            expanded: 0,
            namespace: 0,
            work: 0,
        }
    }
    pub fn check(&self) -> Result<()> {
        if self.cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            Err(SourceError::Cancelled)
        } else {
            Ok(())
        }
    }
    fn charge(total: &mut usize, amount: usize, cap: usize, dim: &'static str) -> Result<()> {
        *total = total
            .checked_add(amount)
            .filter(|n| *n <= cap)
            .ok_or_else(|| limit(dim, cap))?;
        Ok(())
    }
    pub fn work(&mut self, n: usize) -> Result<()> {
        self.check()?;
        Self::charge(&mut self.work, n, self.limits.work, "work")
    }
    pub fn storage(&mut self, n: usize) -> Result<()> {
        Self::charge(
            &mut self.storage,
            n,
            self.limits.storage_bytes,
            "storage_bytes",
        )
    }
    pub fn strings(&mut self, n: usize) -> Result<()> {
        Self::charge(
            &mut self.strings,
            n,
            self.limits.string_bytes,
            "string_bytes",
        )?;
        self.storage(n)
    }
    pub fn expanded(&mut self, n: usize) -> Result<()> {
        if n > self.limits.value_bytes {
            return Err(limit("value_bytes", self.limits.value_bytes));
        }
        Self::charge(
            &mut self.expanded,
            n,
            self.limits.expanded_bytes,
            "expanded_bytes",
        )?;
        // Reserve the parser value and a possible arena text copy before SAX.
        let payload = n
            .checked_mul(2)
            .ok_or_else(|| limit("string_bytes", self.limits.string_bytes))?;
        self.work(payload)?;
        self.strings(payload)
    }
    pub fn namespace(&mut self, n: usize) -> Result<()> {
        Self::charge(
            &mut self.namespace,
            n,
            self.limits.namespace_bytes,
            "namespace_bytes",
        )?;
        self.storage(n)
    }
    pub fn piece(&mut self) -> Result<()> {
        Self::charge(&mut self.pieces, 1, self.limits.pieces, "pieces")
    }
    pub fn push<T>(&mut self, v: &mut Vec<T>, value: T) -> Result<()> {
        self.storage(size_of::<T>())?;
        v.try_reserve(1)
            .map_err(|_| limit("allocation", self.limits.storage_bytes))?;
        v.push(value);
        Ok(())
    }
    pub fn copy(&mut self, s: &str) -> Result<String> {
        self.strings(s.len())?;
        let mut result = String::new();
        result
            .try_reserve_exact(s.len())
            .map_err(|_| limit("allocation", self.limits.storage_bytes))?;
        result.push_str(s);
        Ok(result)
    }
    pub fn append(&mut self, s: &mut String, value: &str) -> Result<()> {
        let len = s
            .len()
            .checked_add(value.len())
            .ok_or_else(|| limit("value_bytes", self.limits.value_bytes))?;
        if len > self.limits.value_bytes {
            return Err(limit("value_bytes", self.limits.value_bytes));
        }
        // Value payload was reserved in lexical expanded-value admission.
        s.try_reserve(value.len())
            .map_err(|_| limit("allocation", self.limits.storage_bytes))?;
        s.push_str(value);
        Ok(())
    }
}

/// Explicit work and UTF-8 output limits for value/path rendering.
#[derive(Debug)]
pub struct QueryBudget {
    work: usize,
    output: usize,
    initial_work: usize,
    initial_output: usize,
}
impl QueryBudget {
    /// Create a finite nonzero budget; no unlimited sentinel is accepted.
    pub fn new(work: usize, output_bytes: usize) -> Result<Self> {
        if work == 0
            || output_bytes == 0
            || work > isize::MAX as usize
            || output_bytes > isize::MAX as usize
        {
            return Err(SourceError::InvalidLimits("query budget"));
        }
        Ok(Self {
            work,
            output: output_bytes,
            initial_work: work,
            initial_output: output_bytes,
        })
    }
    pub(crate) fn allocation_error(&self) -> SourceError {
        limit("query_allocation", self.initial_output)
    }
    pub(crate) fn work(&mut self, n: usize) -> Result<()> {
        self.work = self
            .work
            .checked_sub(n)
            .ok_or_else(|| limit("query_work", self.initial_work))?;
        Ok(())
    }
    pub(crate) fn output(&mut self, n: usize) -> Result<()> {
        self.work(n)?;
        self.output = self
            .output
            .checked_sub(n)
            .ok_or_else(|| limit("query_output", self.initial_output))?;
        Ok(())
    }
}
