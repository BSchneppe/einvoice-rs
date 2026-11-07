use crate::error::{resource, size_overflow};
use crate::{Error, Failure, Resource, Result};
use std::cell::Cell;
use std::rc::Rc;

/// Explicit logical allowances. There are no defaults or unlimited sentinels.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub(crate) numeric: einvoice_numeric::Limits,
    pub(crate) numeric_slot: usize,
    pub(crate) input: usize,
    pub(crate) strings: usize,
    pub(crate) output: usize,
    pub(crate) live_values: usize,
    pub(crate) live_bytes: usize,
    pub(crate) work: u64,
}
impl Limits {
    /// Numeric fuel is a fixed independent suballowance, never reset per operation.
    /// `numeric_bits` also determines a conservative full-size slot for every exact atom.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        input_bytes: usize,
        numeric_bits: u64,
        absolute_scale: u64,
        numeric_work: u64,
        runtime_work: u64,
        string_bytes: usize,
        output_bytes: usize,
        live_values: usize,
        live_bytes: usize,
    ) -> Result<Self> {
        let safe = |x: usize| x > 0 && x <= isize::MAX as usize / 16;
        if ![
            input_bytes,
            string_bytes,
            output_bytes,
            live_values,
            live_bytes,
        ]
        .into_iter()
        .all(safe)
            || runtime_work == 0
        {
            return Err(resource(Resource::InvalidLimits));
        }
        let numeric = einvoice_numeric::Limits::new(
            input_bytes,
            numeric_bits,
            absolute_scale,
            output_bytes,
            numeric_work,
        )?;
        let numeric_slot = usize::try_from(numeric_bits.div_ceil(8))
            .map_err(|_| size_overflow())?
            .checked_add(128)
            .ok_or_else(size_overflow)?;
        Ok(Self {
            numeric,
            numeric_slot,
            input: input_bytes,
            strings: string_bytes,
            output: output_bytes,
            live_values,
            live_bytes,
            work: runtime_work,
        })
    }
}

/// Read-only accounting snapshot. Bytes are logical bounds, not allocator/RSS metrics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Usage {
    /// Remaining abstract runtime fuel.
    pub runtime_work_remaining: u64,
    /// Remaining independent exact-kernel fuel.
    pub numeric_work_remaining: u64,
    /// Remaining conservative cumulative string allocation allowance.
    pub string_bytes_remaining: usize,
    /// Remaining conservative cumulative external rendering allowance.
    pub output_bytes_remaining: usize,
    /// Currently retained atoms and rendered texts.
    pub live_values: usize,
    /// Currently reserved conservative live storage bytes.
    pub live_bytes: usize,
}

#[derive(Debug)]
pub(crate) struct Ledger {
    pub limits: Limits,
    work: Cell<u64>,
    strings: Cell<usize>,
    output: Cell<usize>,
    count: Cell<usize>,
    bytes: Cell<usize>,
}
impl Ledger {
    pub fn new(limits: Limits) -> Rc<Self> {
        Rc::new(Self {
            limits,
            work: Cell::new(limits.work),
            strings: Cell::new(limits.strings),
            output: Cell::new(limits.output),
            count: Cell::new(0),
            bytes: Cell::new(0),
        })
    }
    pub fn snapshot(&self, numeric: u64) -> Usage {
        Usage {
            runtime_work_remaining: self.work.get(),
            numeric_work_remaining: numeric,
            string_bytes_remaining: self.strings.get(),
            output_bytes_remaining: self.output.get(),
            live_values: self.count.get(),
            live_bytes: self.bytes.get(),
        }
    }
    pub fn work(&self, n: u64) -> Result<()> {
        match self.work.get().checked_sub(n.max(1)) {
            Some(left) => {
                self.work.set(left);
                Ok(())
            }
            None => {
                self.work.set(0);
                Err(resource(Resource::Work))
            }
        }
    }
    pub fn input(&self, n: usize) -> Result<()> {
        if n > self.limits.input {
            return Err(resource(Resource::InputBytes));
        }
        self.work(n as u64)
    }
    pub fn string(&self, n: usize) -> Result<()> {
        spend(&self.strings, n, Resource::StringBytes)?;
        self.work(n as u64)
    }
    pub fn output(&self, n: usize) -> Result<()> {
        spend(&self.output, n, Resource::OutputBytes)?;
        self.string(n)
    }
    pub fn lease(self: &Rc<Self>, bytes: usize) -> Result<Lease> {
        self.work(1)?;
        let count = self.count.get().checked_add(1).ok_or_else(size_overflow)?;
        let total = self
            .bytes
            .get()
            .checked_add(bytes)
            .ok_or_else(size_overflow)?;
        if count > self.limits.live_values || total > self.limits.live_bytes {
            return Err(resource(Resource::LiveValues));
        }
        self.count.set(count);
        self.bytes.set(total);
        Ok(Lease {
            ledger: self.clone(),
            bytes,
        })
    }
}
fn spend(cell: &Cell<usize>, n: usize, cause: Resource) -> Result<()> {
    match cell.get().checked_sub(n) {
        Some(left) => {
            cell.set(left);
            Ok(())
        }
        None => {
            cell.set(0);
            Err(resource(cause))
        }
    }
}
#[derive(Debug)]
pub(crate) struct Lease {
    pub ledger: Rc<Ledger>,
    bytes: usize,
}
impl Lease {
    pub fn check(&self, ledger: &Rc<Ledger>) -> Result<()> {
        if Rc::ptr_eq(&self.ledger, ledger) {
            Ok(())
        } else {
            Err(Error::Fatal(Failure::ForeignScope))
        }
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.ledger.count.set(self.ledger.count.get() - 1);
        self.ledger.bytes.set(self.ledger.bytes.get() - self.bytes);
    }
}
