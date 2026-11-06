/*
 * Copyright 2009 Google Inc.  All Rights Reserved.
 * DO NOT ALTER OR REMOVE COPYRIGHT NOTICES OR THIS FILE HEADER.
 *
 * This code is free software; you can redistribute it and/or modify it
 * under the terms of the GNU General Public License version 2 only, as
 * published by the Free Software Foundation.  Sun designates this
 * particular file as subject to the "Classpath" exception as provided
 * by Sun in the LICENSE file that accompanied this code.
 *
 * This code is distributed in the hope that it will be useful, but WITHOUT
 * ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or
 * FITNESS FOR A PARTICULAR PURPOSE.  See the GNU General Public License
 * version 2 for more details (a copy is included in the LICENSE file that
 * accompanied this code).
 *
 * You should have received a copy of the GNU General Public License version
 * 2 along with this work; if not, write to the Free Software Foundation,
 * Inc., 51 Franklin St, Fifth Floor, Boston, MA 02110-1301 USA.
 *
 * Please contact Sun Microsystems, Inc., 4150 Network Circle, Santa Clara,
 * CA 95054 USA or visit www.sun.com if you need additional information or
 * have any questions.
 */

//! A stable, adaptive, iterative mergesort that requires far fewer than
//! n lg(n) comparisons when running on partially sorted arrays, while
//! offering performance comparable to a traditional mergesort when run
//! on random arrays. Like all proper mergesorts, this sort is stable and
//! runs O(n log n) time (worst case). In the worst case, this sort requires
//! temporary storage space for n/2 object references; in the best case,
//! it requires only a small constant amount of space.
//!
//! This implementation was adapted from Tim Peters's list sort for Python,
//! which is described in detail here:
//!   <http://svn.python.org/projects/python/trunk/Objects/listsort.txt>
//!
//! Tim's C code may be found here:
//!   <http://svn.python.org/projects/python/trunk/Objects/listobject.c>
//!
//! The underlying techniques are described in this paper (and may have
//! even earlier origins):
//!   "Optimistic Sorting and Information Theoretic Complexity"
//!   Peter McIlroy
//!   SODA (Fourth Annual ACM-SIAM Symposium on Discrete Algorithms),
//!   pp 467-474, Austin, Texas, 25-27 January 1993.
//!
//! Ported from Java/C# implementation by Josh Bloch.

use std::cmp::Ordering;
use std::marker::PhantomData;

// ============================================================================
// IComparer Trait - Similar to C#'s IComparer<T>
// ============================================================================

/// Trait for comparing two values, similar to C#'s `IComparer<T>`.
///
/// This trait allows creating stateful or stateless comparers that can be
/// passed to sorting functions.
pub trait IComparer<T> {
    /// Compares two values and returns their ordering.
    ///
    /// Returns:
    /// - `Ordering::Less` if `x < y`
    /// - `Ordering::Equal` if `x == y`
    /// - `Ordering::Greater` if `x > y`
    fn compare(&self, x: &T, y: &T) -> Ordering;
}

/// A comparer that uses the default `Ord` implementation.
#[derive(Debug, Clone, Copy, Default)]
pub struct OrdComparer<T>(PhantomData<T>);

impl<T> OrdComparer<T> {
    /// Creates a new `OrdComparer`.
    pub fn new() -> Self {
        OrdComparer(PhantomData)
    }
}

impl<T: Ord> IComparer<T> for OrdComparer<T> {
    fn compare(&self, x: &T, y: &T) -> Ordering {
        x.cmp(y)
    }
}

/// A comparer that reverses the ordering of another comparer.
#[derive(Debug, Clone, Copy)]
pub struct ReverseComparer<C>(pub C);

impl<C> ReverseComparer<C> {
    /// Creates a new `ReverseComparer` wrapping another comparer.
    pub fn new(comparer: C) -> Self {
        ReverseComparer(comparer)
    }
}

impl<T, C: IComparer<T>> IComparer<T> for ReverseComparer<C> {
    fn compare(&self, x: &T, y: &T) -> Ordering {
        self.0.compare(y, x)
    }
}

/// A comparer that wraps a closure.
pub struct FnComparer<F>(pub F);

impl<F> FnComparer<F> {
    /// Creates a new `FnComparer` wrapping a closure.
    pub fn new(f: F) -> Self {
        FnComparer(f)
    }
}

impl<T, F> IComparer<T> for FnComparer<F>
where
    F: Fn(&T, &T) -> Ordering,
{
    fn compare(&self, x: &T, y: &T) -> Ordering {
        (self.0)(x, y)
    }
}

// ============================================================================
// TimSort Constants
// ============================================================================

/// This is the minimum sized sequence that will be merged. Shorter
/// sequences will be lengthened by calling binarySort. If the entire
/// array is less than this length, no merges will be performed.
///
/// This constant should be a power of two. It was 64 in Tim Peter's C
/// implementation, but 32 was empirically determined to work better in
/// this implementation.
const MIN_MERGE: usize = 32;

/// When we get into galloping mode, we stay there until both runs win less
/// often than MIN_GALLOP consecutive times.
const MIN_GALLOP: usize = 7;

/// Maximum initial size of tmp array, which is used for merging.
/// The array can grow to accommodate demand.
const INITIAL_TMP_STORAGE_LENGTH: usize = 256;

/// TimSort state for an ongoing sort operation.
pub struct TimSort<T, F>
where
    F: FnMut(&T, &T) -> Ordering,
{
    /// The array being sorted
    a: Vec<T>,
    /// The comparator for this sort
    c: F,
    /// This controls when we get *into* galloping mode.
    /// It is initialized to MIN_GALLOP. The mergeLo and
    /// mergeHi methods nudge it higher for random data,
    /// and lower for highly structured data.
    min_gallop: usize,
    /// Temp storage for merges
    tmp: Vec<T>,
    /// Number of pending runs on stack
    stack_size: usize,
    /// Stack of pending run bases
    run_base: Vec<usize>,
    /// Stack of pending run lengths
    run_len: Vec<usize>,
}

impl<T, F> TimSort<T, F>
where
    T: Clone,
    F: FnMut(&T, &T) -> Ordering,
{
    /// Creates a TimSort instance to maintain the state of an ongoing sort.
    fn new(a: Vec<T>, c: F) -> Self {
        let len = a.len();

        // Allocate temp storage (which may be increased later if necessary)
        let tmp_len = if len < 2 * INITIAL_TMP_STORAGE_LENGTH {
            len / 2
        } else {
            INITIAL_TMP_STORAGE_LENGTH
        };

        // Allocate runs-to-be-merged stack (which cannot be expanded). The
        // stack length requirements are described in listsort.txt. The C
        // version always uses the same stack length (85), but this was
        // measured to be too expensive when sorting "mid-sized" arrays (e.g.,
        // 100 elements) in Java. Therefore, we use smaller (but sufficiently
        // large) stack lengths for smaller arrays.
        let stack_len = if len < 120 {
            5
        } else if len < 1542 {
            10
        } else if len < 119151 {
            19
        } else {
            40
        };

        TimSort {
            a,
            c,
            min_gallop: MIN_GALLOP,
            tmp: Vec::with_capacity(tmp_len),
            stack_size: 0,
            run_base: vec![0; stack_len],
            run_len: vec![0; stack_len],
        }
    }

    /// Sort the array in place using TimSort algorithm.
    pub fn sort(mut a: Vec<T>, mut c: F) -> Vec<T> {
        let len = a.len();
        if len < 2 {
            return a;
        }

        // If array is small, do a "mini-TimSort" with no merges
        if len < MIN_MERGE {
            let init_run_len = count_run_and_make_ascending(&mut a, 0, len, &mut c);
            binary_sort(&mut a, 0, len, init_run_len, &mut c);
            return a;
        }

        // March over the array once, left to right, finding natural runs,
        // extending short natural runs to minRun elements, and merging runs
        // to maintain stack invariant.
        let mut ts = TimSort::new(a, c);
        let min_run = min_run_length(len);
        let mut lo = 0;
        let mut n_remaining = len;

        loop {
            // Identify next run
            let mut run_len = count_run_and_make_ascending(&mut ts.a, lo, len, &mut ts.c);

            // If run is short, extend to min(minRun, nRemaining)
            if run_len < min_run {
                let force = if n_remaining <= min_run {
                    n_remaining
                } else {
                    min_run
                };
                binary_sort(&mut ts.a, lo, lo + force, lo + run_len, &mut ts.c);
                run_len = force;
            }

            // Push run onto pending-run stack, and maybe merge
            ts.push_run(lo, run_len);
            ts.merge_collapse();

            // Advance to find next run
            lo += run_len;
            n_remaining -= run_len;

            if n_remaining == 0 {
                break;
            }
        }

        // Merge all remaining runs to complete sort
        ts.merge_force_collapse();
        ts.a
    }

    /// Pushes the specified run onto the pending-run stack.
    fn push_run(&mut self, run_base: usize, run_len: usize) {
        self.run_base[self.stack_size] = run_base;
        self.run_len[self.stack_size] = run_len;
        self.stack_size += 1;
    }

    /// Examines the stack of runs waiting to be merged and merges adjacent runs
    /// until the stack invariants are reestablished:
    ///
    /// 1. `runLen[i - 3] > runLen[i - 2] + runLen[i - 1]`
    /// 2. `runLen[i - 2] > runLen[i - 1]`
    ///
    /// This method is called each time a new run is pushed onto the stack,
    /// so the invariants are guaranteed to hold for i < stackSize upon
    /// entry to the method.
    fn merge_collapse(&mut self) {
        while self.stack_size > 1 {
            let mut n = self.stack_size - 2;
            if n > 0 && self.run_len[n - 1] <= self.run_len[n] + self.run_len[n + 1] {
                if self.run_len[n - 1] < self.run_len[n + 1] {
                    n -= 1;
                }
                self.merge_at(n);
            } else if self.run_len[n] <= self.run_len[n + 1] {
                self.merge_at(n);
            } else {
                break; // Invariant is established
            }
        }
    }

    /// Merges all runs on the stack until only one remains.
    /// This method is called once, to complete the sort.
    fn merge_force_collapse(&mut self) {
        while self.stack_size > 1 {
            let mut n = self.stack_size - 2;
            if n > 0 && self.run_len[n - 1] < self.run_len[n + 1] {
                n -= 1;
            }
            self.merge_at(n);
        }
    }

    /// Merges the two runs at stack indices i and i+1. Run i must be
    /// the penultimate or antepenultimate run on the stack. In other words,
    /// i must be equal to stackSize-2 or stackSize-3.
    fn merge_at(&mut self, i: usize) {
        debug_assert!(self.stack_size >= 2);
        debug_assert!(i == self.stack_size - 2 || i == self.stack_size - 3);

        let mut base1 = self.run_base[i];
        let mut len1 = self.run_len[i];
        let base2 = self.run_base[i + 1];
        let mut len2 = self.run_len[i + 1];
        debug_assert!(len1 > 0 && len2 > 0);
        debug_assert!(base1 + len1 == base2);

        // Record the length of the combined runs; if i is the 3rd-last
        // run now, also slide over the last run (which isn't involved
        // in this merge). The current run (i+1) goes away in any case.
        self.run_len[i] = len1 + len2;
        if i + 3 == self.stack_size {
            self.run_base[i + 1] = self.run_base[i + 2];
            self.run_len[i + 1] = self.run_len[i + 2];
        }
        self.stack_size -= 1;

        // Find where the first element of run2 goes in run1. Prior elements
        // in run1 can be ignored (because they're already in place).
        let k = gallop_right(&self.a[base2], &self.a, base1, len1, 0, &mut self.c);
        base1 += k;
        len1 -= k;
        if len1 == 0 {
            return;
        }

        // Find where the last element of run1 goes in run2. Subsequent elements
        // in run2 can be ignored (because they're already in place).
        len2 = gallop_left(
            &self.a[base1 + len1 - 1],
            &self.a,
            base2,
            len2,
            len2 - 1,
            &mut self.c,
        );
        if len2 == 0 {
            return;
        }

        // Merge remaining runs, using tmp array with min(len1, len2) elements
        if len1 <= len2 {
            self.merge_lo(base1, len1, base2, len2);
        } else {
            self.merge_hi(base1, len1, base2, len2);
        }
    }

    /// Ensures that the external array tmp has at least the specified
    /// number of elements, increasing its size if necessary. The size
    /// increases exponentially to ensure amortized linear time complexity.
    fn ensure_capacity(&mut self, min_capacity: usize) {
        if self.tmp.capacity() < min_capacity {
            // Compute smallest power of 2 > minCapacity
            let mut new_size = min_capacity;
            new_size |= new_size >> 1;
            new_size |= new_size >> 2;
            new_size |= new_size >> 4;
            new_size |= new_size >> 8;
            new_size |= new_size >> 16;
            new_size = new_size.wrapping_add(1);

            if new_size == 0 {
                // overflow
                new_size = min_capacity;
            } else {
                new_size = new_size.min(self.a.len() / 2);
            }

            self.tmp = Vec::with_capacity(new_size);
        }
        self.tmp.clear();
    }

    /// Merges two adjacent runs in place, in a stable fashion. The first
    /// element of the first run must be greater than the first element of the
    /// second run (a[base1] > a[base2]), and the last element of the first run
    /// (a[base1 + len1-1]) must be greater than all elements of the second run.
    ///
    /// For performance, this method should be called only when len1 <= len2;
    /// its twin, merge_hi should be called if len1 >= len2. (Either method
    /// may be called if len1 == len2.)
    fn merge_lo(&mut self, base1: usize, mut len1: usize, base2: usize, mut len2: usize) {
        debug_assert!(len1 > 0 && len2 > 0 && base1 + len1 == base2);

        // Copy first run into temp array
        self.ensure_capacity(len1);
        for i in 0..len1 {
            self.tmp.push(self.a[base1 + i].clone());
        }

        let mut cursor1 = 0usize; // Indexes into tmp array
        let mut cursor2 = base2; // Indexes into a
        let mut dest = base1; // Indexes into a

        // Move first element of second run and deal with degenerate cases
        self.a[dest] = self.a[cursor2].clone();
        dest += 1;
        cursor2 += 1;
        len2 -= 1;
        if len2 == 0 {
            for i in 0..len1 {
                self.a[dest + i] = self.tmp[cursor1 + i].clone();
            }
            return;
        }
        if len1 == 1 {
            for i in 0..len2 {
                self.a[dest + i] = self.a[cursor2 + i].clone();
            }
            self.a[dest + len2] = self.tmp[cursor1].clone();
            return;
        }

        let mut min_gallop = self.min_gallop;

        'outer: loop {
            let mut count1 = 0usize; // Number of times in a row that first run won
            let mut count2 = 0usize; // Number of times in a row that second run won

            // Do the straightforward thing until (if ever) one run starts
            // winning consistently.
            loop {
                debug_assert!(len1 > 1 && len2 > 0);
                if (self.c)(&self.a[cursor2], &self.tmp[cursor1]) == Ordering::Less {
                    self.a[dest] = self.a[cursor2].clone();
                    dest += 1;
                    cursor2 += 1;
                    count2 += 1;
                    count1 = 0;
                    len2 -= 1;
                    if len2 == 0 {
                        break 'outer;
                    }
                } else {
                    self.a[dest] = self.tmp[cursor1].clone();
                    dest += 1;
                    cursor1 += 1;
                    count1 += 1;
                    count2 = 0;
                    len1 -= 1;
                    if len1 == 1 {
                        break 'outer;
                    }
                }
                if (count1 | count2) >= min_gallop {
                    break;
                }
            }

            // One run is winning so consistently that galloping may be a
            // huge win. So try that, and continue galloping until (if ever)
            // neither run appears to be winning consistently anymore.
            loop {
                debug_assert!(len1 > 1 && len2 > 0);
                count1 = gallop_right(&self.a[cursor2], &self.tmp, cursor1, len1, 0, &mut self.c);
                if count1 != 0 {
                    for i in 0..count1 {
                        self.a[dest + i] = self.tmp[cursor1 + i].clone();
                    }
                    dest += count1;
                    cursor1 += count1;
                    len1 -= count1;
                    if len1 <= 1 {
                        break 'outer;
                    }
                }
                self.a[dest] = self.a[cursor2].clone();
                dest += 1;
                cursor2 += 1;
                len2 -= 1;
                if len2 == 0 {
                    break 'outer;
                }

                count2 = gallop_left(&self.tmp[cursor1], &self.a, cursor2, len2, 0, &mut self.c);
                if count2 != 0 {
                    for i in 0..count2 {
                        self.a[dest + i] = self.a[cursor2 + i].clone();
                    }
                    dest += count2;
                    cursor2 += count2;
                    len2 -= count2;
                    if len2 == 0 {
                        break 'outer;
                    }
                }
                self.a[dest] = self.tmp[cursor1].clone();
                dest += 1;
                cursor1 += 1;
                len1 -= 1;
                if len1 == 1 {
                    break 'outer;
                }
                min_gallop = min_gallop.saturating_sub(1);
                if count1 < MIN_GALLOP && count2 < MIN_GALLOP {
                    break;
                }
            }
            min_gallop += 2; // Penalize for leaving gallop mode
        }

        self.min_gallop = if min_gallop < 1 { 1 } else { min_gallop };

        if len1 == 1 {
            debug_assert!(len2 > 0);
            for i in 0..len2 {
                self.a[dest + i] = self.a[cursor2 + i].clone();
            }
            self.a[dest + len2] = self.tmp[cursor1].clone();
        } else if len1 == 0 {
            panic!("Comparison method violates its general contract!");
        } else {
            debug_assert!(len2 == 0);
            debug_assert!(len1 > 1);
            for i in 0..len1 {
                self.a[dest + i] = self.tmp[cursor1 + i].clone();
            }
        }
    }

    /// Like merge_lo, except that this method should be called only if
    /// len1 >= len2; merge_lo should be called if len1 <= len2. (Either method
    /// may be called if len1 == len2.)
    ///
    /// The cursors walk **backwards**. When run 1 starts at index 0 and is
    /// exhausted before run 2, `cursor1` (and, in the `len2 == 1` tails,
    /// `dest`) step one position past the front of the array — the `-1` the
    /// reference implementation reaches with a signed `int`. The value is
    /// never used as an index afterwards: either the loop exits on
    /// `len1 == 0`, or `+ 1` is added back first. With `usize` that needs
    /// wrapping arithmetic; an ordinary subtraction panics in a debug build
    /// and wraps silently in a release build.
    fn merge_hi(&mut self, base1: usize, mut len1: usize, base2: usize, mut len2: usize) {
        debug_assert!(len1 > 0 && len2 > 0 && base1 + len1 == base2);

        // Copy second run into temp array
        self.ensure_capacity(len2);
        for i in 0..len2 {
            self.tmp.push(self.a[base2 + i].clone());
        }

        let mut cursor1 = base1 + len1 - 1; // Indexes into a
        let mut cursor2 = len2 - 1; // Indexes into tmp array (use isize for potential underflow)
        let mut dest = base2 + len2 - 1; // Indexes into a

        // Move last element of first run and deal with degenerate cases
        self.a[dest] = self.a[cursor1].clone();
        dest = dest.wrapping_sub(1);
        cursor1 = cursor1.wrapping_sub(1);
        len1 -= 1;
        if len1 == 0 {
            let start = dest - (len2 - 1);
            for i in 0..len2 {
                self.a[start + i] = self.tmp[i].clone();
            }
            return;
        }
        if len2 == 1 {
            dest = dest.wrapping_sub(len1);
            cursor1 = cursor1.wrapping_sub(len1);
            for i in (0..len1).rev() {
                self.a[dest.wrapping_add(1).wrapping_add(i)] =
                    self.a[cursor1.wrapping_add(1).wrapping_add(i)].clone();
            }
            self.a[dest] = self.tmp[cursor2].clone();
            return;
        }

        let mut min_gallop = self.min_gallop;

        'outer: loop {
            let mut count1 = 0usize;
            let mut count2 = 0usize;

            // Do the straightforward thing until (if ever) one run
            // appears to win consistently.
            loop {
                debug_assert!(len1 > 0 && len2 > 1);
                if (self.c)(&self.tmp[cursor2], &self.a[cursor1]) == Ordering::Less {
                    self.a[dest] = self.a[cursor1].clone();
                    dest = dest.wrapping_sub(1);
                    cursor1 = cursor1.wrapping_sub(1);
                    count1 += 1;
                    count2 = 0;
                    len1 -= 1;
                    if len1 == 0 {
                        break 'outer;
                    }
                } else {
                    self.a[dest] = self.tmp[cursor2].clone();
                    dest = dest.wrapping_sub(1);
                    cursor2 = cursor2.wrapping_sub(1);
                    count2 += 1;
                    count1 = 0;
                    len2 -= 1;
                    if len2 == 1 {
                        break 'outer;
                    }
                }
                if (count1 | count2) >= min_gallop {
                    break;
                }
            }

            // One run is winning so consistently that galloping may be a
            // huge win. So try that, and continue galloping until (if ever)
            // neither run appears to be winning consistently anymore.
            loop {
                debug_assert!(len1 > 0 && len2 > 1);
                count1 = len1
                    - gallop_right(
                        &self.tmp[cursor2],
                        &self.a,
                        base1,
                        len1,
                        len1 - 1,
                        &mut self.c,
                    );
                if count1 != 0 {
                    dest = dest.wrapping_sub(count1);
                    cursor1 = cursor1.wrapping_sub(count1);
                    len1 -= count1;
                    for i in (0..count1).rev() {
                        self.a[dest.wrapping_add(1).wrapping_add(i)] =
                            self.a[cursor1.wrapping_add(1).wrapping_add(i)].clone();
                    }
                    if len1 == 0 {
                        break 'outer;
                    }
                }
                self.a[dest] = self.tmp[cursor2].clone();
                dest = dest.wrapping_sub(1);
                cursor2 = cursor2.wrapping_sub(1);
                len2 -= 1;
                if len2 == 1 {
                    break 'outer;
                }

                count2 =
                    len2 - gallop_left(&self.a[cursor1], &self.tmp, 0, len2, len2 - 1, &mut self.c);
                if count2 != 0 {
                    dest = dest.wrapping_sub(count2);
                    cursor2 = cursor2.wrapping_sub(count2);
                    len2 -= count2;
                    for i in 0..count2 {
                        self.a[dest + 1 + i] = self.tmp[cursor2.wrapping_add(1) + i].clone();
                    }
                    if len2 <= 1 {
                        break 'outer;
                    }
                }
                self.a[dest] = self.a[cursor1].clone();
                // `len2 > 1` here, so `dest` stays inside the array; `cursor1`
                // may be the first element of a run at index 0.
                dest -= 1;
                cursor1 = cursor1.wrapping_sub(1);
                len1 -= 1;
                if len1 == 0 {
                    break 'outer;
                }
                min_gallop = min_gallop.saturating_sub(1);
                if count1 < MIN_GALLOP && count2 < MIN_GALLOP {
                    break;
                }
            }
            min_gallop += 2; // Penalize for leaving gallop mode
        }

        self.min_gallop = if min_gallop < 1 { 1 } else { min_gallop };

        if len2 == 1 {
            debug_assert!(len1 > 0);
            dest = dest.wrapping_sub(len1);
            cursor1 = cursor1.wrapping_sub(len1);
            for i in (0..len1).rev() {
                self.a[dest.wrapping_add(1).wrapping_add(i)] =
                    self.a[cursor1.wrapping_add(1).wrapping_add(i)].clone();
            }
            self.a[dest] = self.tmp[cursor2].clone();
        } else if len2 == 0 {
            panic!("Comparison method violates its general contract!");
        } else {
            debug_assert!(len1 == 0);
            debug_assert!(len2 > 0);
            let start = dest - (len2 - 1);
            for i in 0..len2 {
                self.a[start + i] = self.tmp[i].clone();
            }
        }
    }
}

/// Sorts the specified portion of the specified array using a binary
/// insertion sort. This is the best method for sorting small numbers
/// of elements. It requires O(n log n) compares, but O(n^2) data
/// movement (worst case).
///
/// If the initial part of the specified range is already sorted,
/// this method can take advantage of it: the method assumes that the
/// elements from index `lo`, inclusive, to `start`, exclusive are already sorted.
fn binary_sort<T, F>(a: &mut [T], lo: usize, hi: usize, mut start: usize, c: &mut F)
where
    T: Clone,
    F: FnMut(&T, &T) -> Ordering,
{
    debug_assert!(lo <= start && start <= hi);
    if start == lo {
        start += 1;
    }
    while start < hi {
        let pivot = a[start].clone();

        // Set left (and right) to the index where a[start] (pivot) belongs
        let mut left = lo;
        let mut right = start;

        // Invariants:
        //   pivot >= all in [lo, left).
        //   pivot <  all in [right, start).
        while left < right {
            let mid = (left + right) / 2;
            if c(&pivot, &a[mid]) == Ordering::Less {
                right = mid;
            } else {
                left = mid + 1;
            }
        }

        // The invariants still hold: pivot >= all in [lo, left) and
        // pivot < all in [left, start), so pivot belongs at left. Note
        // that if there are elements equal to pivot, left points to the
        // first slot after them -- that's why this sort is stable.
        // Slide elements over to make room for pivot.
        let n = start - left;
        match n {
            2 => {
                a[left + 2] = a[left + 1].clone();
                a[left + 1] = a[left].clone();
            }
            1 => {
                a[left + 1] = a[left].clone();
            }
            _ if n > 0 => {
                for i in (0..n).rev() {
                    a[left + i + 1] = a[left + i].clone();
                }
            }
            _ => {}
        }
        a[left] = pivot;
        start += 1;
    }
}

/// Returns the length of the run beginning at the specified position in
/// the specified array and reverses the run if it is descending (ensuring
/// that the run will always be ascending when the method returns).
///
/// A run is the longest ascending sequence with:
///    a[lo] <= a[lo + 1] <= a[lo + 2] <= ...
///
/// or the longest descending sequence with:
///    a[lo] >  a[lo + 1] >  a[lo + 2] >  ...
///
/// For its intended use in a stable mergesort, the strictness of the
/// definition of "descending" is needed so that the call can safely
/// reverse a descending sequence without violating stability.
fn count_run_and_make_ascending<T, F>(a: &mut [T], lo: usize, hi: usize, c: &mut F) -> usize
where
    T: Clone,
    F: FnMut(&T, &T) -> Ordering,
{
    debug_assert!(lo < hi);
    let mut run_hi = lo + 1;
    if run_hi == hi {
        return 1;
    }

    // Find end of run, and reverse range if descending
    if c(&a[run_hi], &a[lo]) == Ordering::Less {
        // Descending
        run_hi += 1;
        while run_hi < hi && c(&a[run_hi], &a[run_hi - 1]) == Ordering::Less {
            run_hi += 1;
        }
        reverse_range(a, lo, run_hi);
    } else {
        // Ascending
        run_hi += 1;
        while run_hi < hi && c(&a[run_hi], &a[run_hi - 1]) != Ordering::Less {
            run_hi += 1;
        }
    }

    run_hi - lo
}

/// Reverse the specified range of the specified array.
fn reverse_range<T>(a: &mut [T], mut lo: usize, mut hi: usize) {
    hi -= 1;
    while lo < hi {
        a.swap(lo, hi);
        lo += 1;
        hi -= 1;
    }
}

/// Returns the minimum acceptable run length for an array of the specified
/// length. Natural runs shorter than this will be extended with binary_sort.
///
/// Roughly speaking, the computation is:
///   If n < MIN_MERGE, return n (it's too small to bother with fancy stuff).
///   Else if n is an exact power of 2, return MIN_MERGE/2.
///   Else return an int k, MIN_MERGE/2 <= k <= MIN_MERGE, such that n/k
///    is close to, but strictly less than, an exact power of 2.
fn min_run_length(mut n: usize) -> usize {
    let mut r = 0; // Becomes 1 if any 1 bits are shifted off
    while n >= MIN_MERGE {
        r |= n & 1;
        n >>= 1;
    }
    n + r
}

/// Locates the position at which to insert the specified key into the
/// specified sorted range; if the range contains an element equal to key,
/// returns the index of the leftmost equal element.
fn gallop_left<T, F>(key: &T, a: &[T], base: usize, len: usize, hint: usize, c: &mut F) -> usize
where
    F: FnMut(&T, &T) -> Ordering,
{
    debug_assert!(len > 0 && hint < len);
    let mut last_ofs = 0usize;
    let mut ofs = 1usize;

    if c(key, &a[base + hint]) == Ordering::Greater {
        // Gallop right until a[base+hint+lastOfs] < key <= a[base+hint+ofs]
        let max_ofs = len - hint;
        while ofs < max_ofs && c(key, &a[base + hint + ofs]) == Ordering::Greater {
            last_ofs = ofs;
            ofs = (ofs << 1) + 1;
            if ofs == 0 {
                // int overflow
                ofs = max_ofs;
            }
        }
        if ofs > max_ofs {
            ofs = max_ofs;
        }

        // Make offsets relative to base (and pre-increment last_ofs)
        last_ofs += hint + 1;
        ofs += hint;
    } else {
        // key <= a[base + hint]
        // Gallop left until a[base+hint-ofs] < key <= a[base+hint-lastOfs]
        let max_ofs = hint + 1;
        while ofs < max_ofs && c(key, &a[base + hint - ofs]) != Ordering::Greater {
            last_ofs = ofs;
            ofs = (ofs << 1) + 1;
            if ofs == 0 {
                // int overflow
                ofs = max_ofs;
            }
        }
        if ofs > max_ofs {
            ofs = max_ofs;
        }

        // Make offsets relative to base (and pre-increment last_ofs)
        // Note: ofs can be hint+1 so hint-ofs would underflow for usize;
        // folding the +1 avoids this since (hint+1)-ofs >= 0
        let tmp = last_ofs;
        last_ofs = (hint + 1) - ofs;
        ofs = hint - tmp;
    }
    debug_assert!(last_ofs <= ofs && ofs <= len);

    // Now a[base+lastOfs-1] < key <= a[base+ofs], so key belongs somewhere
    // to the right of lastOfs but no farther right than ofs. Do a binary
    // search, with invariant a[base + lastOfs - 1] < key <= a[base + ofs].
    while last_ofs < ofs {
        let m = last_ofs + ((ofs - last_ofs) / 2);

        if c(key, &a[base + m]) == Ordering::Greater {
            last_ofs = m + 1; // a[base + m] < key
        } else {
            ofs = m; // key <= a[base + m]
        }
    }
    debug_assert!(last_ofs == ofs);
    ofs
}

/// Like gallop_left, except that if the range contains an element equal to
/// key, gallop_right returns the index after the rightmost equal element.
fn gallop_right<T, F>(key: &T, a: &[T], base: usize, len: usize, hint: usize, c: &mut F) -> usize
where
    F: FnMut(&T, &T) -> Ordering,
{
    debug_assert!(len > 0 && hint < len);

    let mut ofs = 1usize;
    let mut last_ofs = 0usize;

    if c(key, &a[base + hint]) == Ordering::Less {
        // Gallop left until a[b+hint - ofs] <= key < a[b+hint - lastOfs]
        let max_ofs = hint + 1;
        while ofs < max_ofs && c(key, &a[base + hint - ofs]) == Ordering::Less {
            last_ofs = ofs;
            ofs = (ofs << 1) + 1;
            if ofs == 0 {
                // int overflow
                ofs = max_ofs;
            }
        }
        if ofs > max_ofs {
            ofs = max_ofs;
        }

        // Make offsets relative to b (and pre-increment last_ofs)
        // Note: ofs can be hint+1 so hint-ofs would underflow for usize;
        // folding the +1 avoids this since (hint+1)-ofs >= 0
        let tmp = last_ofs;
        last_ofs = (hint + 1) - ofs;
        ofs = hint - tmp;
    } else {
        // a[b + hint] <= key
        // Gallop right until a[b+hint + lastOfs] <= key < a[b+hint + ofs]
        let max_ofs = len - hint;
        while ofs < max_ofs && c(key, &a[base + hint + ofs]) != Ordering::Less {
            last_ofs = ofs;
            ofs = (ofs << 1) + 1;
            if ofs == 0 {
                // int overflow
                ofs = max_ofs;
            }
        }
        if ofs > max_ofs {
            ofs = max_ofs;
        }

        // Make offsets relative to b (and pre-increment last_ofs)
        last_ofs += hint + 1;
        ofs += hint;
    }
    debug_assert!(last_ofs <= ofs && ofs <= len);

    // Now a[b + lastOfs - 1] <= key < a[b + ofs], so key belongs somewhere to
    // the right of lastOfs but no farther right than ofs. Do a binary
    // search, with invariant a[b + lastOfs - 1] <= key < a[b + ofs].
    while last_ofs < ofs {
        let m = last_ofs + ((ofs - last_ofs) / 2);

        if c(key, &a[base + m]) == Ordering::Less {
            ofs = m; // key < a[b + m]
        } else {
            last_ofs = m + 1; // a[b + m] <= key
        }
    }
    debug_assert!(last_ofs == ofs);
    ofs
}

/// Sort a vector using the TimSort algorithm with a custom comparator.
pub fn timsort_by<T, F>(vec: Vec<T>, compare: F) -> Vec<T>
where
    T: Clone,
    F: FnMut(&T, &T) -> Ordering,
{
    TimSort::sort(vec, compare)
}

/// Sort a vector using the TimSort algorithm with the default ordering.
pub fn timsort<T>(vec: Vec<T>) -> Vec<T>
where
    T: Clone + Ord,
{
    TimSort::sort(vec, |a, b| a.cmp(b))
}

/// Sort a slice in place using the TimSort algorithm with a custom comparator.
pub fn timsort_slice_by<T, F>(slice: &mut [T], compare: F)
where
    T: Clone,
    F: FnMut(&T, &T) -> Ordering,
{
    let vec: Vec<T> = slice.to_vec();
    let sorted = TimSort::sort(vec, compare);
    for (i, item) in sorted.into_iter().enumerate() {
        slice[i] = item;
    }
}

/// Sort a slice in place using the TimSort algorithm with the default ordering.
pub fn timsort_slice<T>(slice: &mut [T])
where
    T: Clone + Ord,
{
    timsort_slice_by(slice, |a, b| a.cmp(b))
}

// ============================================================================
// IComparer-based Sorting Functions
// ============================================================================

/// Sort a vector using the TimSort algorithm with an IComparer.
pub fn timsort_with_comparer<T, C>(vec: Vec<T>, comparer: &C) -> Vec<T>
where
    T: Clone,
    C: IComparer<T>,
{
    TimSort::sort(vec, |a, b| comparer.compare(a, b))
}

/// Sort a slice in place using the TimSort algorithm with an IComparer.
pub fn timsort_slice_with_comparer<T, C>(slice: &mut [T], comparer: &C)
where
    T: Clone,
    C: IComparer<T>,
{
    timsort_slice_by(slice, |a, b| comparer.compare(a, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_array() {
        let arr: Vec<i32> = vec![];
        let result = timsort(arr);
        assert!(result.is_empty());
    }

    #[test]
    fn test_single_element() {
        let arr = vec![42];
        let result = timsort(arr);
        assert_eq!(result, vec![42]);
    }

    #[test]
    fn test_already_sorted() {
        let arr = vec![1, 2, 3, 4, 5];
        let result = timsort(arr);
        assert_eq!(result, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_reverse_sorted() {
        let arr = vec![5, 4, 3, 2, 1];
        let result = timsort(arr);
        assert_eq!(result, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_random_order() {
        let arr = vec![3, 1, 4, 1, 5, 9, 2, 6, 5, 3, 5];
        let result = timsort(arr);
        assert_eq!(result, vec![1, 1, 2, 3, 3, 4, 5, 5, 5, 6, 9]);
    }

    #[test]
    fn test_duplicates() {
        let arr = vec![3, 3, 3, 1, 1, 2, 2];
        let result = timsort(arr);
        assert_eq!(result, vec![1, 1, 2, 2, 3, 3, 3]);
    }

    #[test]
    fn test_custom_comparator_reverse() {
        let arr = vec![1, 2, 3, 4, 5];
        let result = timsort_by(arr, |a, b| b.cmp(a));
        assert_eq!(result, vec![5, 4, 3, 2, 1]);
    }

    #[test]
    fn test_stability() {
        // Test that equal elements maintain their relative order
        #[derive(Clone, Debug)]
        struct Item {
            key: i32,
            index: usize,
        }

        let arr = vec![
            Item { key: 1, index: 0 },
            Item { key: 2, index: 1 },
            Item { key: 1, index: 2 },
            Item { key: 2, index: 3 },
            Item { key: 1, index: 4 },
        ];

        let result = timsort_by(arr, |a, b| a.key.cmp(&b.key));

        // Check that items with same key maintain original order
        let ones: Vec<_> = result.iter().filter(|x| x.key == 1).collect();
        assert_eq!(ones[0].index, 0);
        assert_eq!(ones[1].index, 2);
        assert_eq!(ones[2].index, 4);

        let twos: Vec<_> = result.iter().filter(|x| x.key == 2).collect();
        assert_eq!(twos[0].index, 1);
        assert_eq!(twos[1].index, 3);
    }

    #[test]
    fn test_large_array() {
        let arr: Vec<i32> = (0..1000).rev().collect();
        let expected: Vec<i32> = (0..1000).collect();
        let result = timsort(arr);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_strings() {
        let arr = vec!["banana", "apple", "cherry", "date"];
        let result = timsort(arr);
        assert_eq!(result, vec!["apple", "banana", "cherry", "date"]);
    }

    #[test]
    fn test_slice_sort() {
        let mut arr = [5, 2, 8, 1, 9];
        timsort_slice(&mut arr);
        assert_eq!(arr, [1, 2, 5, 8, 9]);
    }

    // IComparer tests

    #[test]
    fn test_ord_comparer() {
        let arr = vec![3, 1, 4, 1, 5, 9, 2, 6];
        let comparer = OrdComparer::<i32>::new();
        let result = timsort_with_comparer(arr, &comparer);
        assert_eq!(result, vec![1, 1, 2, 3, 4, 5, 6, 9]);
    }

    #[test]
    fn test_reverse_comparer() {
        let arr = vec![1, 2, 3, 4, 5];
        let comparer = ReverseComparer::new(OrdComparer::<i32>::new());
        let result = timsort_with_comparer(arr, &comparer);
        assert_eq!(result, vec![5, 4, 3, 2, 1]);
    }

    #[test]
    fn test_fn_comparer() {
        let arr = vec![3, 1, 4, 1, 5];
        let comparer = FnComparer::new(|a: &i32, b: &i32| a.cmp(b));
        let result = timsort_with_comparer(arr, &comparer);
        assert_eq!(result, vec![1, 1, 3, 4, 5]);
    }

    #[test]
    fn test_fn_comparer_custom() {
        // Sort by absolute value
        let arr = vec![-3, 1, -4, 1, 5, -9, 2, -6];
        let comparer = FnComparer::new(|a: &i32, b: &i32| a.abs().cmp(&b.abs()));
        let result = timsort_with_comparer(arr, &comparer);
        assert_eq!(result, vec![1, 1, 2, -3, -4, 5, -6, -9]);
    }

    #[test]
    fn test_slice_with_comparer() {
        let mut arr = [5, 2, 8, 1, 9];
        let comparer = OrdComparer::<i32>::new();
        timsort_slice_with_comparer(&mut arr, &comparer);
        assert_eq!(arr, [1, 2, 5, 8, 9]);
    }
    /// `merge_hi` walks its cursors backwards and lets them run one past the
    /// front of the array, which used to panic in a debug build. This input
    /// reaches that path: a long descending prefix (one run) followed by a
    /// shorter ascending tail, merged from base 0.
    #[test]
    fn merge_hi_does_not_overflow_at_the_front_of_the_array() {
        // A long descending prefix becomes run 1 at base 0; a shorter
        // ascending tail whose values are all *larger* becomes run 2. Merging
        // them backwards copies run 2 first and then walks run 1 down to its
        // first element, so `cursor1` and `dest` reach the `-1` sentinel.
        for (long, short) in [(100usize, 16usize), (257, 40), (1000, 100)] {
            let mut data: Vec<i64> = (1..=long as i64).rev().collect();
            data.extend((0..short as i64).map(|x| 10_000 + x));
            let mut expected = data.clone();
            expected.sort();

            let sorted = timsort(data);
            assert_eq!(sorted, expected, "{long} + {short}");
        }

        // The galloping branch: run 1 sits at base 0 and every one of its
        // values is *above* run 2's, so after seven straight wins the gallop
        // claims all of run 1 at once and `cursor1 -= count1` steps off the
        // front of the array.
        for (long, short) in [(60usize, 30usize), (200, 90), (1000, 400)] {
            let mut data: Vec<i64> = (0..long as i64).map(|i| 10_000 - i).collect();
            data.extend((0..short as i64).map(|i| 100 - i));
            let mut expected = data.clone();
            expected.sort();
            assert_eq!(timsort(data), expected, "gallop {long} + {short}");
        }

        // The last statement of the gallop loop: run 1 sits at base 0, its
        // first element is below most of run 2 but above a few of it. The
        // gallop first claims all of run 1 except that element, then claims
        // the part of run 2 above it, and finally moves the element itself —
        // stepping `cursor1` off the front while run 2 still has items left.
        for (high, low, mid) in [(100i64, 4i64, 40i64), (300, 2, 120), (1000, 9, 500)] {
            let mut data: Vec<i64> = vec![5];
            data.extend(1000..1000 + high);
            data.extend(0..low.min(5));
            data.extend(500..500 + mid);
            let mut expected = data.clone();
            expected.sort();
            assert_eq!(
                timsort(data),
                expected,
                "gallop tail {high} + {low} + {mid}"
            );
        }

        // A deterministic sweep of run shapes, which is what actually walks
        // every branch of `merge_hi` and `merge_lo`: a cheap LCG decides how
        // long each run is and whether it ascends or descends.
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for round in 0..200 {
            let mut data: Vec<i64> = Vec::new();
            let mut value: i64 = 0;
            while data.len() < 300 {
                let run = 1 + (next() % 70) as usize;
                let descending = next() % 2 == 0;
                let start = value;
                if descending {
                    data.extend((0..run as i64).map(|i| start + run as i64 - i));
                } else {
                    data.extend((0..run as i64).map(|i| start + i));
                }
                // Sometimes the next run is entirely above the previous one,
                // which is what exhausts run 1 before run 2 in `merge_hi`.
                value = if next() % 3 == 0 {
                    start + 10_000
                } else {
                    start
                };
            }
            let mut expected = data.clone();
            expected.sort();
            assert_eq!(timsort(data), expected, "round {round}");
        }
    }
}
