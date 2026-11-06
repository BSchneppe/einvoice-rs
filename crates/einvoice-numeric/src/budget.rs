use crate::{Error, ResourceCause, Result};

/// Validated per-operation limits and initial cumulative work allowance.
///
/// No field is optional; zero and values unsafe for platform size estimates are rejected.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub(crate) input: usize,
    pub(crate) bits: u64,
    pub(crate) scale: u64,
    pub(crate) output: usize,
    pub(crate) work: u64,
}

impl Limits {
    /// Construct explicit limits. Scale is the maximum absolute signed scale.
    pub fn new(
        input_bytes: usize,
        coefficient_bits: u64,
        absolute_scale: u64,
        output_bytes: usize,
        cumulative_work: u64,
    ) -> Result<Self> {
        if input_bytes == 0
            || input_bytes > isize::MAX as usize / 16
            || coefficient_bits == 0
            || coefficient_bits > u64::from(u32::MAX) / 16
            || absolute_scale == 0
            || absolute_scale > i64::MAX as u64
            || output_bytes == 0
            || output_bytes > isize::MAX as usize / 16
            || cumulative_work == 0
        {
            return Err(Error::Resource(ResourceCause::InvalidLimits));
        }
        Ok(Self {
            input: input_bytes,
            bits: coefficient_bits,
            scale: absolute_scale,
            output: output_bytes,
            work: cumulative_work,
        })
    }
}

/// A caller-scoped, non-cloneable cumulative work budget.
///
/// Work is conservative fuel, not elapsed time or an allocator/RSS measurement.
/// Failed operations retain charges already made. Exhausting work leaves zero fuel.
#[derive(Debug)]
pub struct Budget {
    pub(crate) limits: Limits,
    remaining: u64,
}

impl Budget {
    /// Start an independent scope with the given validated limits.
    pub fn new(limits: Limits) -> Self {
        Self {
            remaining: limits.work,
            limits,
        }
    }

    /// Remaining abstract work units.
    pub fn remaining_work(&self) -> u64 {
        self.remaining
    }

    pub(crate) fn charge(&mut self, work: u64) -> Result<()> {
        match self.remaining.checked_sub(work.max(1)) {
            Some(left) => {
                self.remaining = left;
                Ok(())
            }
            None => {
                self.remaining = 0;
                Err(Error::Resource(ResourceCause::Work))
            }
        }
    }

    pub(crate) fn bits(&self, bits: u64) -> Result<()> {
        if bits > self.limits.bits {
            Err(Error::Resource(ResourceCause::CoefficientBits))
        } else {
            Ok(())
        }
    }

    pub(crate) fn scale(&self, scale: i64) -> Result<()> {
        if scale.unsigned_abs() > self.limits.scale {
            Err(Error::Resource(ResourceCause::Scale))
        } else {
            Ok(())
        }
    }

    pub(crate) fn output(&self, bytes: u64) -> Result<usize> {
        if bytes > self.limits.output as u64 {
            Err(Error::Resource(ResourceCause::OutputBytes))
        } else {
            Ok(bytes as usize)
        }
    }
}

pub(crate) fn overflow() -> Error {
    Error::Resource(ResourceCause::SizeOverflow)
}

pub(crate) fn sum(a: u64, b: u64) -> Result<u64> {
    a.checked_add(b).ok_or_else(overflow)
}

pub(crate) fn product(a: u64, b: u64) -> Result<u64> {
    a.checked_mul(b).ok_or_else(overflow)
}

pub(crate) fn difference(a: i64, b: i64) -> Result<u64> {
    u64::try_from(i128::from(a) - i128::from(b)).map_err(|_| overflow())
}
