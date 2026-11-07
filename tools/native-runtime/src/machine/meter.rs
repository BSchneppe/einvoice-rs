use crate::error::{resource, size_overflow};
use crate::{Resource, Result};

/// Finite preparation or run allowances. No defaults or unlimited sentinel.
#[derive(Clone, Copy, Debug)]
pub struct MachineLimits {
    /// AST nodes, recipes, items, environments and cursors admitted cumulatively.
    pub objects: usize,
    /// Maximum simultaneously retained logical machine bytes.
    pub live_bytes: usize,
    /// Cumulative bytes admitted, including released continuations and scratch.
    pub total_bytes: usize,
    /// Simultaneous explicit continuation or preparation depth.
    pub frames: usize,
    /// Abstract work, including pulls, node visits and order comparisons.
    pub work: u64,
}
/// Logical accounting; numeric and source-query allowances are independent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MachineUsage {
    /// Cumulative objects admitted.
    pub objects: usize,
    /// Conservative retention reservations (including admitted retired capacity).
    pub live_bytes: usize,
    /// All admitted bytes, including transient storage.
    pub total_bytes: usize,
    /// Remaining abstract work.
    pub work_remaining: u64,
}
pub(super) struct Meter {
    limits: MachineLimits,
    pub usage: MachineUsage,
}
impl Meter {
    pub fn new(limits: MachineLimits) -> Result<Self> {
        if [
            limits.objects,
            limits.live_bytes,
            limits.total_bytes,
            limits.frames,
        ]
        .iter()
        .any(|n| *n == 0 || *n > isize::MAX as usize / 16)
            || limits.work == 0
        {
            return Err(resource(Resource::InvalidLimits));
        }
        Ok(Self {
            limits,
            usage: MachineUsage {
                objects: 0,
                live_bytes: 0,
                total_bytes: 0,
                work_remaining: limits.work,
            },
        })
    }
    pub fn work(&mut self, n: usize) -> Result<()> {
        self.usage.work_remaining = self
            .usage
            .work_remaining
            .checked_sub(u64::try_from(n.max(1)).map_err(|_| size_overflow())?)
            .ok_or_else(|| resource(Resource::Machine("work")))?;
        Ok(())
    }
    pub fn bytes(&mut self, n: usize) -> Result<()> {
        let live = self
            .usage
            .live_bytes
            .checked_add(n)
            .ok_or_else(size_overflow)?;
        let total = self
            .usage
            .total_bytes
            .checked_add(n)
            .ok_or_else(size_overflow)?;
        if live > self.limits.live_bytes || total > self.limits.total_bytes {
            return Err(resource(Resource::Machine("storage")));
        }
        self.usage.live_bytes = live;
        self.usage.total_bytes = total;
        Ok(())
    }
    pub fn release(&mut self, n: usize) {
        self.usage.live_bytes -= n;
    }
    pub fn object(&mut self, n: usize) -> Result<()> {
        self.work(1)?;
        let objects = self
            .usage
            .objects
            .checked_add(1)
            .ok_or_else(size_overflow)?;
        if objects > self.limits.objects {
            return Err(resource(Resource::Machine("objects")));
        }
        self.bytes(n)?;
        self.usage.objects = objects;
        Ok(())
    }
    pub fn depth(&self, n: usize) -> Result<()> {
        if n > self.limits.frames {
            Err(resource(Resource::Machine("frames")))
        } else {
            Ok(())
        }
    }
    // Reserve explicit geometric capacity, including copying work, before growth.
    // Reservations for retired capacity remain charged until the arena is dropped.
    pub fn reserve<T>(&mut self, v: &mut Vec<T>) -> Result<()> {
        if v.len() == v.capacity() {
            let capacity = v
                .capacity()
                .max(1)
                .checked_mul(2)
                .ok_or_else(size_overflow)?;
            self.bytes(
                capacity
                    .checked_mul(size_of::<T>())
                    .ok_or_else(size_overflow)?,
            )?;
            self.work(v.len())?;
            v.try_reserve_exact(capacity.checked_sub(v.len()).ok_or_else(size_overflow)?)
                .map_err(|_| resource(Resource::Machine("allocation")))?;
        }
        Ok(())
    }
    pub fn push<T>(&mut self, v: &mut Vec<T>, value: T) -> Result<()> {
        self.bytes(size_of::<T>())?;
        self.reserve(v)?;
        v.push(value);
        Ok(())
    }
}
