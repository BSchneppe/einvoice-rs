#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod budget;
mod conversion;
mod error;
mod ieee;
pub mod machine;
mod numeric;
mod value;

pub use budget::{Limits, Usage};
pub use error::{Error, Failure, Fault, Resource, Result, XQT_ERRORS};
pub use numeric::{Arithmetic, Comparison};
pub use validation_ir::NumericKind;
pub use value::{Atomic, AtomicType, Text};

use budget::Ledger;
use std::rc::Rc;
use value::Value;

/// One caller's non-cloneable atomic processing scope. No reset operation exists.
#[derive(Debug)]
pub struct Scope {
    ledger: Rc<Ledger>,
    numeric: einvoice_numeric::Budget,
}
impl Scope {
    /// Reserve independent fixed numeric and runtime fuel allowances.
    pub fn new(limits: Limits) -> Self {
        Self {
            ledger: Ledger::new(limits),
            numeric: einvoice_numeric::Budget::new(limits.numeric),
        }
    }
    /// Observe cumulative fuel/bytes and currently retained value slots.
    pub fn usage(&self) -> Usage {
        self.ledger.snapshot(self.numeric.remaining_work())
    }
    fn check(&self, value: &Atomic) -> Result<()> {
        value.lease.check(&self.ledger)?;
        self.ledger.work(1)
    }
    fn slot(&self) -> Result<budget::Lease> {
        self.ledger.lease(self.ledger.limits.numeric_slot)
    }
    /// Explicitly copy an atom handle with conservative live accounting and work.
    pub fn duplicate(&mut self, value: &Atomic) -> Result<Atomic> {
        self.check(value)?;
        let bytes = match &value.value {
            Value::String(s) | Value::UntypedAtomic(s) => {
                s.len().checked_add(128).ok_or_else(error::size_overflow)?
            }
            _ => self.ledger.limits.numeric_slot,
        };
        let lease = self.ledger.lease(bytes)?;
        Ok(Atomic {
            value: value.value.clone(),
            lease,
        })
    }
}
