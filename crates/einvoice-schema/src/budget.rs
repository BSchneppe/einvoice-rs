//! Explicit logical admission, not hard RSS or backend instruction accounting.
use crate::{model::Event, SourceDocument};
use serde::Serialize;
use std::{
    cell::RefCell,
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Failure {
    InvalidLimits,
    Limit {
        dimension: &'static str,
        limit: usize,
    },
    ArithmeticOverflow,
    Allocation,
    Cancelled,
    Deadline,
    EpochRetired,
    UnsupportedEvidence,
    UnsupportedCapability {
        capability: &'static str,
    },
    SourceJoin,
    Consistency,
    /// Out-of-band backend operational/finalization result, never an invoice finding.
    Backend {
        phase: &'static str,
    },
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Failure {}

/// All values are explicit, finite and nonzero. No calibrated production defaults.
#[derive(Debug, Clone, Serialize)]
pub struct RequestLimits {
    pub input_bytes: usize,
    pub nodes: usize,
    pub depth: usize,
    pub attributes_per_element: usize,
    pub declarations_per_element: usize,
    pub namespaces_per_element: usize,
    pub name_bytes: usize,
    pub uri_bytes: usize,
    pub value_bytes: usize,
    pub temporary_bytes: usize,
    pub work: usize,
    pub diagnostic_count: usize,
    pub diagnostic_bytes: usize,
    pub terminal_bytes: usize,
}
impl RequestLimits {
    pub(crate) fn check(&self) -> Result<(), Failure> {
        let values = [
            self.input_bytes,
            self.nodes,
            self.depth,
            self.attributes_per_element,
            self.declarations_per_element,
            self.namespaces_per_element,
            self.name_bytes,
            self.uri_bytes,
            self.value_bytes,
            self.temporary_bytes,
            self.work,
            self.diagnostic_count,
            self.diagnostic_bytes,
            self.terminal_bytes,
        ];
        if values
            .into_iter()
            .any(|n| n == 0 || n > isize::MAX as usize)
        {
            Err(Failure::InvalidLimits)
        } else {
            Ok(())
        }
    }
}
#[derive(Debug, Default, Clone, Copy)]
pub struct RequestControl<'a> {
    pub cancellation: Option<&'a AtomicBool>,
    pub deadline: Option<Instant>,
}
#[derive(Default)]
pub(crate) struct State {
    pub events: Vec<Event>,
    pub failure: Option<Failure>,
    pub work: usize,
    pub bytes: usize,
    pub errors: usize,
    pub callbacks: usize,
}
pub(crate) struct Control<'a> {
    pub source: &'a SourceDocument,
    pub limits: &'a RequestLimits,
    pub signals: RequestControl<'a>,
    pub state: RefCell<State>,
}
impl Control<'_> {
    pub fn poll(&self) -> Result<(), Failure> {
        if let Some(f) = self.state.borrow().failure.as_ref() {
            return Err(f.clone());
        }
        if self
            .signals
            .cancellation
            .is_some_and(|c| c.load(Ordering::Relaxed))
        {
            return self.fail(Failure::Cancelled);
        }
        if self.signals.deadline.is_some_and(|d| Instant::now() >= d) {
            return self.fail(Failure::Deadline);
        }
        Ok(())
    }
    pub fn fail<T>(&self, f: Failure) -> Result<T, Failure> {
        let mut s = self.state.borrow_mut();
        Err(s.failure.get_or_insert(f).clone())
    }
    pub fn work(&self, n: usize) -> Result<(), Failure> {
        self.poll()?;
        let mut s = self.state.borrow_mut();
        let next = s.work.checked_add(n).ok_or(Failure::ArithmeticOverflow)?;
        if next > self.limits.work {
            drop(s);
            return self.fail(Failure::Limit {
                dimension: "work",
                limit: self.limits.work,
            });
        }
        s.work = next;
        Ok(())
    }
    pub fn cap(&self, dimension: &'static str, n: usize, limit: usize) -> Result<(), Failure> {
        self.poll()?;
        if n > limit {
            self.fail(Failure::Limit { dimension, limit })
        } else {
            Ok(())
        }
    }
}
/// Checked enumeration of logical retained fields before copying. Includes record
/// storage and payload lengths, not allocator capacity or serialized JSON bytes.
pub(crate) struct Size {
    pub bytes: usize,
    limit: usize,
}
impl Size {
    pub fn new(limit: usize) -> Self {
        Self { bytes: 0, limit }
    }
    pub fn add(&mut self, n: usize) -> Result<(), Failure> {
        self.bytes = self
            .bytes
            .checked_add(n)
            .ok_or(Failure::ArithmeticOverflow)?;
        if self.bytes > self.limit {
            Err(Failure::Limit {
                dimension: "diagnostic_bytes",
                limit: self.limit,
            })
        } else {
            Ok(())
        }
    }
}
pub(crate) fn mul(a: usize, b: usize) -> Result<usize, Failure> {
    a.checked_mul(b).ok_or(Failure::ArithmeticOverflow)
}
