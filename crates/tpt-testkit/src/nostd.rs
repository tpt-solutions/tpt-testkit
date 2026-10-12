//! Host-side helpers for testing `no_std` + `alloc` code under a fixed heap budget.
//!
//! Embedded code runs with a small, fixed heap. An allocation that does not fit there usually
//! panics or aborts. [`Budget`] lets a host test check the failure path instead: it caps how many
//! bytes a test allows the code under test to hold, and [`Budget::alloc_vec`] allocates fallibly,
//! so a request over budget becomes an [`OutOfBudget`] error.
//!
//! Limits: the budget only counts allocations made through it. It does not intercept the code
//! under test's own `Vec` or `Box` calls. Route the allocations you want to cap through it, and
//! pass the budget into the code under test.
//!
//! [`Budget`] is single-threaded (it uses `Rc`), which is enough for host unit tests.
//!
//! ```
//! use tpt_testkit::nostd::Budget;
//!
//! let budget = Budget::new(64);
//! let small = budget.alloc_vec::<u8>(32, 0).unwrap();
//! assert_eq!(budget.used(), 32);
//! assert!(budget.alloc_vec::<u8>(64, 0).is_err());
//! drop(small);
//! assert_eq!(budget.used(), 0);
//! ```

use std::cell::Cell;
use std::fmt;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

/// A request that does not fit in the remaining budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutOfBudget {
    /// Bytes requested, or `usize::MAX` when the size overflowed.
    pub requested: usize,
    /// Bytes still available when the request was made.
    pub available: usize,
}

impl fmt::Display for OutOfBudget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "requested {} bytes but only {} are available",
            self.requested, self.available
        )
    }
}

impl std::error::Error for OutOfBudget {}

#[derive(Debug)]
struct Shared {
    limit: usize,
    used: Cell<usize>,
}

/// A fixed byte budget that code under test allocates from.
#[derive(Debug, Clone)]
pub struct Budget {
    shared: Rc<Shared>,
}

impl Budget {
    /// Creates a budget of `limit` bytes.
    pub fn new(limit: usize) -> Self {
        Self {
            shared: Rc::new(Shared {
                limit,
                used: Cell::new(0),
            }),
        }
    }

    /// Total size of the budget in bytes.
    pub fn limit(&self) -> usize {
        self.shared.limit
    }

    /// Bytes currently reserved.
    pub fn used(&self) -> usize {
        self.shared.used.get()
    }

    /// Bytes that can still be reserved.
    pub fn available(&self) -> usize {
        self.shared.limit - self.shared.used.get()
    }

    /// Reserves `bytes` from the budget. The reservation is released when it is dropped.
    ///
    /// # Errors
    ///
    /// Returns [`OutOfBudget`] if fewer than `bytes` bytes are available.
    pub fn reserve(&self, bytes: usize) -> Result<Reservation, OutOfBudget> {
        let available = self.available();
        if bytes > available {
            return Err(OutOfBudget {
                requested: bytes,
                available,
            });
        }
        self.shared.used.set(self.shared.used.get() + bytes);
        Ok(Reservation {
            shared: Rc::clone(&self.shared),
            bytes,
        })
    }

    /// Allocates a `Vec` of `len` copies of `value`, charged against the budget.
    ///
    /// The allocation is fallible. If the host cannot provide the memory, the error is
    /// [`OutOfBudget`] as well, so no path panics.
    ///
    /// # Errors
    ///
    /// Returns [`OutOfBudget`] if `len * size_of::<T>()` overflows, exceeds the remaining budget,
    /// or cannot be allocated.
    pub fn alloc_vec<T: Clone>(&self, len: usize, value: T) -> Result<Budgeted<T>, OutOfBudget> {
        let available = self.available();
        let overflow = OutOfBudget {
            requested: usize::MAX,
            available,
        };
        let bytes = len.checked_mul(std::mem::size_of::<T>()).ok_or(overflow)?;
        let reservation = self.reserve(bytes)?;

        let mut items = Vec::new();
        items.try_reserve_exact(len).map_err(|_| OutOfBudget {
            requested: bytes,
            available,
        })?;
        items.resize(len, value);
        Ok(Budgeted {
            items,
            _reservation: reservation,
        })
    }
}

/// Bytes held against a [`Budget`]. Released on drop.
#[derive(Debug)]
pub struct Reservation {
    shared: Rc<Shared>,
    bytes: usize,
}

impl Reservation {
    /// Size of this reservation in bytes.
    pub fn bytes(&self) -> usize {
        self.bytes
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        self.shared.used.set(self.shared.used.get() - self.bytes);
    }
}

/// A `Vec` whose memory is charged to a [`Budget`]. Dereferences to a slice.
#[derive(Debug)]
pub struct Budgeted<T> {
    items: Vec<T>,
    _reservation: Reservation,
}

impl<T> Deref for Budgeted<T> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        &self.items
    }
}

impl<T> DerefMut for Budgeted<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        &mut self.items
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reservations_are_released_on_drop() {
        let budget = Budget::new(100);
        let a = budget.reserve(60).unwrap();
        assert_eq!(budget.available(), 40);
        drop(a);
        assert_eq!(budget.used(), 0);
    }

    #[test]
    fn over_budget_reservation_is_an_error_not_a_panic() {
        let budget = Budget::new(10);
        let err = budget.reserve(11).unwrap_err();
        assert_eq!(
            err,
            OutOfBudget {
                requested: 11,
                available: 10
            }
        );
        assert_eq!(budget.used(), 0);
    }

    #[test]
    fn alloc_vec_is_charged_and_released() {
        let budget = Budget::new(64);
        let mut v = budget.alloc_vec::<u32>(8, 7).unwrap();
        assert_eq!(budget.used(), 32);
        assert_eq!(v[3], 7);
        v[0] = 1;
        assert_eq!(v.len(), 8);
        drop(v);
        assert_eq!(budget.used(), 0);
    }

    #[test]
    fn alloc_vec_over_budget_fails_cleanly() {
        let budget = Budget::new(16);
        assert!(budget.alloc_vec::<u32>(5, 0).is_err());
        assert_eq!(budget.used(), 0);
    }

    #[test]
    fn alloc_vec_size_overflow_fails_cleanly() {
        let budget = Budget::new(usize::MAX);
        let err = budget.alloc_vec::<u64>(usize::MAX, 0).unwrap_err();
        assert_eq!(err.requested, usize::MAX);
    }

    #[test]
    fn clones_share_one_budget() {
        let budget = Budget::new(10);
        let other = budget.clone();
        let _held = other.reserve(10).unwrap();
        assert!(budget.reserve(1).is_err());
    }
}
