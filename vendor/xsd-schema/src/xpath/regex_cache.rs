//! Reuse of compiled regular expressions: within one evaluation run, and — when
//! the host asks for it — across runs.
//!
//! [`matches`](crate::xpath::functions::regex::matches),
//! [`replace`](crate::xpath::functions::regex::replace) and
//! [`tokenize`](crate::xpath::functions::regex::tokenize) each turn their
//! `$pattern` and `$flags` arguments into a compiled program before they do any
//! work. Compiling is not cheap: the backend parses the pattern and builds the
//! program, which for a pattern that names a Unicode category or a shorthand
//! that stands for one (`\p{…}`, `\d`, `\w`) holds its own copy of that class's
//! code-point set — and of its complement for `\P{…}` — although the engine
//! builds each category set only once per process. On a short input string the
//! compilation still costs more than the match.
//!
//! That would not matter if a pattern were compiled once per expression, but the
//! shape that matters most is
//!
//! ```text
//!     $items[matches(., '\p{Ll}')]
//! ```
//!
//! where **one** compiled expression is evaluated once and its predicate calls
//! `matches` once per item, with a pattern that is a literal and cannot change.
//! Compilation is then the whole cost of the filter, and it is paid again for
//! every item.
//!
//! This module keeps the compiled program of each distinct `(pattern, flags)`
//! pair, so that every later call with that pair costs a key comparison and
//! nothing else. It has two tiers.
//!
//! # The run's own cache (the default)
//!
//! In [`DynamicContext`](crate::xpath::context::DynamicContext), which is created
//! by [`XPathEvaluator::run`](crate::xpath::XPathEvaluator::run) and dropped when
//! that run returns: one cache per run, never in the compiled expression (which
//! is shared and immutable) and never global — so there is no lock on the path of
//! a per-item predicate, nothing outlives the run, and two runs cannot observe
//! each other. It is an empty `Vec` until the run's first regular-expression
//! call, so an expression that uses none pays a few words of context and no
//! allocation.
//!
//! # A cache the host keeps: [`RegexCache`]
//!
//! One cache per run is the right lifetime for a host that evaluates an
//! expression once over many items. It is the wrong one for a host that
//! evaluates the same expression *many times*, once per item — a host
//! evaluating the same test for every item of a loop, or a validator
//! evaluating an assertion per element: every run starts empty, so every run
//! compiles the pattern again. Such a host creates a [`RegexCache`], keeps it
//! for as long as it likes (one run of the host's own work, say) and lends it to
//! each run with
//! [`XPathEvaluator::with_regex_cache`](crate::xpath::XPathEvaluator::with_regex_cache)
//! or [`DynamicContext::set_regex_cache`](crate::xpath::DynamicContext::set_regex_cache).
//! A run that has one uses it *instead of* its own cache, so a pattern is
//! compiled once for the life of the host's cache rather than once per run.
//!
//! The host's cache is the host's state, not the process's: it is `Send` but not
//! `Sync`, so it can move to another thread but is never shared by two threads
//! at once, and two hosts — or two runs of one host's work — only share one
//! if they are handed the same value. It uses interior mutability so that runs
//! can borrow it by shared reference, and holds its inner borrow only while it
//! looks a key up or stores one, never while a pattern is matched — so a run
//! nested inside another run (an extension function that evaluates an
//! expression) can use the same cache.
//!
//! # Why one compiled program can serve every later call
//!
//! `regexml::Regex` owns a `ReProgram` of plain immutable data; every piece of
//! mutable matching state lives in the `ReMatcher` that each `is_match`,
//! `replace_all` or `tokenize` call builds for itself from `&ReProgram`. Reusing
//! one `Regex` therefore produces exactly what a freshly compiled one produces,
//! for any of the three functions in any order.
//!
//! The run's own cache hands it out as a plain `&Regex`, not as an `Rc`: `Regex`
//! is not `Clone`, so it has to be shared one way or the other, and a reference
//! costs no allocation at all — which matters because the *first* call for a key
//! is the one the cache can only make slower, and every allocation it adds there
//! shows up in an expression that calls `matches` exactly once. The price is that
//! the borrow of the context lasts as long as the regex is in use; the three
//! functions want nothing else from the context by then. The host's cache cannot
//! lend a reference past its inner borrow, so it holds each program in an `Arc`
//! and hands out a clone: one allocation per key for the life of the cache, and
//! a reference-count increment per call.
//!
//! The "does this pattern match the zero-length string" answer that `fn:replace`
//! and `fn:tokenize` raise FORX0003 from is a field of the compiled `Regex`,
//! computed once when it is built; reusing the program reuses that answer too.
//!
//! # The key
//!
//! `(pattern, flags)`, compared as strings. Nothing else enters the compilation:
//! the three functions compile through one private function that reads those two
//! strings and nothing from the static or the dynamic context — no mode, no base
//! URI, no collation, and one regular-expression dialect (XPath 2.0's). That is
//! also what makes one host cache safe to lend to runs of *different* expressions
//! under *different* static contexts.
//!
//! # Failures are cached as well
//!
//! What the compile step produces — an invalid-flags or invalid-pattern error —
//! is a pure function of `(pattern, flags)`: both errors are built from those two
//! strings and nothing else is read. Storing the error and replaying it makes the
//! first and the thousandth call identical *by construction*, instead of leaving
//! that to two code paths agreeing with each other.
//!
//! # The bound
//!
//! A pattern need not be a literal —
//!
//! ```text
//!     $rows[matches(@value, @pattern)]
//! ```
//!
//! computes a new one per item, and an unbounded cache would then hold every
//! pattern the input contains. Both tiers therefore have a fixed number of
//! entries — [`MAX_ENTRIES`] for a run, [`RegexCache::DEFAULT_CAPACITY`] (or
//! what the host chose) for a host cache — and the least recently used entry is
//! dropped to make room, so such a run keeps at most that many programs alive and
//! a run that walks through many patterns before settling on one still ends up
//! with the one it uses resident.

use std::cell::RefCell;
use std::fmt;
use std::sync::Arc;

use regexml::Regex;

use crate::xpath::error::XPathError;

/// How many distinct `(pattern, flags)` pairs one run keeps compiled.
///
/// A regular-expression argument is nearly always a literal, and an expression
/// with more than a handful of distinct ones is already unusual, so 32 is far
/// above what a real expression asks for — the same reasoning, and the same
/// number, as [`compare_cache::MAX_TRACKED_NODES`](crate::xpath::compare_cache).
/// It is chosen against the memory a compiled program costs: measured with a
/// counting allocator, 32 plain patterns hold ~26 KB and 32 patterns each
/// carrying a Unicode category class ~0.5 MB, which is the worst case this bound
/// admits, per run, for the run's lifetime.
pub(crate) const MAX_ENTRIES: usize = 32;

/// One cached compilation, with the key it was compiled from. `T` is what the
/// tier holds a program as: the program itself for a run, an `Arc` of it for a
/// host cache.
struct Entry<T> {
    pattern: String,
    flags: String,
    /// A program, or the error compiling it raised — which is the same error
    /// every time (see the module documentation).
    compiled: Result<T, XPathError>,
    /// The reading of [`Table::clock`] at this entry's last use, which is what
    /// makes "least recently used" answerable without moving entries about.
    used: u64,
}

/// The least-recently-used table both tiers are made of.
pub(crate) struct Table<T> {
    /// In the order the keys were first seen. Recency is a number on the entry
    /// rather than its position, so a hit writes one `u64` and an eviction
    /// overwrites one slot: entries are never shifted, which matters because a
    /// compiled program is a large value to move and the run that evicts most is
    /// the one that can least afford the work.
    entries: Vec<Entry<T>>,
    /// The most entries the table holds; at least 1.
    capacity: usize,
    /// Ticks once per call, so that a larger `used` means "more recently".
    /// A `u64` cannot wrap: one tick per regular-expression call.
    clock: u64,
    /// How many calls were answered from the table, and how many had to
    /// compile. Test-only, so that a test can tell "the behaviour is still
    /// right" from "the cache silently stopped engaging"; the shipped path pays
    /// nothing for them.
    #[cfg(test)]
    hits: u32,
    #[cfg(test)]
    compiles: u32,
}

/// The compiled regular expressions of one evaluation run.
///
/// Lives in [`DynamicContext`](crate::xpath::context::DynamicContext) and dies
/// with it. Empty and allocation-free until the run's first regular-expression
/// call.
pub(crate) type RunCache = Table<Regex>;

impl Default for Table<Regex> {
    fn default() -> Self {
        Table::with_capacity(MAX_ENTRIES)
    }
}

impl<T> Table<T> {
    /// An empty table of at most `capacity` entries (at least one). Allocates
    /// nothing until the first entry is stored.
    fn with_capacity(capacity: usize) -> Self {
        Table {
            entries: Vec::new(),
            capacity: capacity.max(1),
            clock: 0,
            #[cfg(test)]
            hits: 0,
            #[cfg(test)]
            compiles: 0,
        }
    }

    /// The program for `(pattern, flags)`, compiling it with `compile` the first
    /// time this table is asked for it — or the first time since it was evicted.
    ///
    /// `compile` is called at most once per resident key. Whatever it produces —
    /// a program or an error — is what every later call with that key gets back.
    pub(crate) fn get_or_compile(
        &mut self,
        pattern: &str,
        flags: &str,
        compile: impl FnOnce() -> Result<T, XPathError>,
    ) -> Result<&T, XPathError> {
        let now = self.clock;
        self.clock += 1;

        // One pass answers both questions: where this key is, and — if it is not
        // here at all — which entry is the one to give up. The victim is only
        // ever read when the loop ran to the end, so the early exit cannot leave
        // it half-computed.
        let mut found = None;
        let mut victim = 0usize;
        let mut victim_used = u64::MAX;
        for (index, entry) in self.entries.iter().enumerate() {
            if entry.pattern == pattern && entry.flags == flags {
                found = Some(index);
                break;
            }
            if entry.used < victim_used {
                victim_used = entry.used;
                victim = index;
            }
        }

        let index = match found {
            Some(index) => {
                self.entries[index].used = now;
                #[cfg(test)]
                {
                    self.hits += 1;
                }
                index
            }
            None => {
                #[cfg(test)]
                {
                    self.compiles += 1;
                }
                let compiled = compile();
                if self.entries.len() < self.capacity {
                    self.entries.push(Entry {
                        pattern: pattern.to_owned(),
                        flags: flags.to_owned(),
                        compiled,
                        used: now,
                    });
                    self.entries.len() - 1
                } else {
                    // The least recently used key makes room. A table that keeps
                    // being asked for fresh patterns therefore holds at most
                    // `capacity` programs, whatever its input — and, once it is
                    // full, stops allocating key strings altogether, because the
                    // evicted entry's buffers are written over instead of dropped
                    // and replaced.
                    let slot = &mut self.entries[victim];
                    slot.pattern.clear();
                    slot.pattern.push_str(pattern);
                    slot.flags.clear();
                    slot.flags.push_str(flags);
                    slot.compiled = compiled;
                    slot.used = now;
                    victim
                }
            }
        };

        // One borrow, taken after both arms are done with `&mut self`.
        match &self.entries[index].compiled {
            Ok(regex) => Ok(regex),
            // Only a failing key pays for a clone, and it pays it in place of a
            // second compile.
            Err(err) => Err(err.clone()),
        }
    }

    /// How many entries are resident. Never more than the capacity.
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    /// How many calls were answered from the table.
    #[cfg(test)]
    pub(crate) fn hits(&self) -> u32 {
        self.hits
    }

    /// How many calls had to compile.
    #[cfg(test)]
    pub(crate) fn compiles(&self) -> u32 {
        self.compiles
    }

    /// The resident keys, in the order the entries are stored.
    #[cfg(test)]
    pub(crate) fn keys(&self) -> Vec<(&str, &str)> {
        self.entries
            .iter()
            .map(|entry| (entry.pattern.as_str(), entry.flags.as_str()))
            .collect()
    }
}

/// Compiled regular expressions that a host keeps across evaluation runs.
///
/// Every [`XPathEvaluator::run`](crate::xpath::XPathEvaluator::run) already
/// compiles each distinct `(pattern, flags)` pair of `fn:matches`,
/// `fn:replace` and `fn:tokenize` only once, but it forgets them when it
/// returns. A host that evaluates the same expressions over and over — once per
/// item or per node, as when the same test is evaluated for every item of a
/// loop — would then compile the same pattern in every run. Keeping a
/// `RegexCache` and lending it to every run makes each pattern compile once for
/// the life of the cache:
///
/// ```
/// use xsd_schema::namespace::table::NameTable;
/// use xsd_schema::xpath::{RegexCache, RoXmlNavigator, XPathContext, XPathExpr};
///
/// let names = NameTable::new();
/// let ctx = XPathContext::new(&names);
/// let expr = XPathExpr::compile_with_vars(r"matches($s, '^\p{Lu}\p{Ll}+$')", &ctx, &["s"])
///     .unwrap();
///
/// // Kept by the host for as long as it likes — here, across 1,000 runs.
/// let regexes = RegexCache::new();
/// let mut capitalised = 0;
/// for i in 0..1_000 {
///     let word = if i % 2 == 0 { "Hello" } else { "hello" };
///     if expr
///         .evaluator(&ctx)
///         .with_regex_cache(&regexes)
///         .with_variable("s", word)
///         .unwrap()
///         .run_bool::<RoXmlNavigator<'static>>()
///         .unwrap()
///     {
///         capitalised += 1;
///     }
/// }
/// assert_eq!(capitalised, 500);
/// // One pattern, compiled once, resident for the next run.
/// assert_eq!(regexes.len(), 1);
/// ```
///
/// Inside a [`run_with`](crate::xpath::XPathEvaluator::run_with) setup callback,
/// where the dynamic context is at hand, the same is
/// [`DynamicContext::set_regex_cache`](crate::xpath::DynamicContext::set_regex_cache).
///
/// # What it holds, and for how long
///
/// The program of each `(pattern, flags)` pair — or the error compiling it
/// raised, which is a function of the two strings alone and is replayed
/// unchanged. Nothing from the expression, the static context or the dynamic
/// context enters a compilation, so one cache can serve runs of different
/// expressions under different static contexts. It holds at most
/// [`capacity`](Self::capacity) entries, dropping the least recently used one to
/// make room, and keeps them until it is dropped or [`clear`](Self::clear)ed.
///
/// # Threads
///
/// `RegexCache` is `Send` but not `Sync`: it can be handed to another thread, but
/// two threads never use one at the same time. There is no lock and no
/// process-global state; a host that evaluates on several threads gives each its
/// own cache.
///
/// # Memory
///
/// Measured with a counting allocator, a compiled program holds about 1 KB for a
/// pattern of plain characters and classes, about 16 KB for one Unicode category
/// (`\p{Ll}`, `\P{Ll}`) and about 40 KB for a pattern with several (`\w+@\w+`,
/// or four `\P{Ll}`); a pattern's program grows with the pattern. At the default
/// capacity of 64 entries that is a few megabytes at most for ordinary patterns.
pub struct RegexCache {
    table: RefCell<Table<Arc<Regex>>>,
}

impl RegexCache {
    /// The capacity of [`new`](Self::new) and [`Default`]: 64 entries.
    ///
    /// Twice the bound of a single run's own cache, because a host cache serves
    /// every expression of the host — all the regular-expression literals of
    /// its program, say — rather than the few of one expression; and small enough
    /// that its worst case for ordinary patterns (64 programs of a few category
    /// classes each, see [Memory](#memory)) stays at a few megabytes. A host that
    /// cycles through more distinct patterns than this pays one compilation per
    /// call for the ones that do not fit, which is what it pays without a cache.
    pub const DEFAULT_CAPACITY: usize = 64;

    /// An empty cache of [`DEFAULT_CAPACITY`](Self::DEFAULT_CAPACITY) entries.
    /// Allocates nothing until the first program is stored.
    pub fn new() -> Self {
        Self::with_capacity(Self::DEFAULT_CAPACITY)
    }

    /// An empty cache that holds at most `capacity` programs; a capacity of 0
    /// is taken as 1. Allocates nothing until the first program is stored.
    pub fn with_capacity(capacity: usize) -> Self {
        RegexCache {
            table: RefCell::new(Table::with_capacity(capacity)),
        }
    }

    /// The most programs this cache holds.
    pub fn capacity(&self) -> usize {
        self.table.borrow().capacity
    }

    /// How many `(pattern, flags)` pairs are resident — compiled programs and
    /// cached compile errors alike.
    pub fn len(&self) -> usize {
        self.table.borrow().len()
    }

    /// Whether nothing is resident.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Drop every resident program.
    ///
    /// Takes `&self`, like everything else here, because a host that lends the
    /// cache to its runs holds it by shared reference. A run still using a
    /// program it took from the cache keeps that program alive until it is done
    /// with it.
    pub fn clear(&self) {
        self.table.borrow_mut().entries.clear();
    }

    /// The program for `(pattern, flags)`, compiling it with `compile` when it is
    /// not resident.
    ///
    /// The inner borrow lasts for the lookup — and, on a miss, the compilation,
    /// which runs no host code — and ends before the caller matches anything.
    /// Should the table be borrowed already all the same, the program is
    /// compiled and not stored: slower, never wrong, never a panic.
    pub(crate) fn get_or_compile(
        &self,
        pattern: &str,
        flags: &str,
        compile: impl FnOnce() -> Result<Regex, XPathError>,
    ) -> Result<Arc<Regex>, XPathError> {
        match self.table.try_borrow_mut() {
            Ok(mut table) => table
                .get_or_compile(pattern, flags, || compile().map(Arc::new))
                .cloned(),
            Err(_) => compile().map(Arc::new),
        }
    }

    /// How many calls were answered from the cache.
    #[cfg(test)]
    pub(crate) fn hits(&self) -> u32 {
        self.table.borrow().hits()
    }

    /// How many calls had to compile.
    #[cfg(test)]
    pub(crate) fn compiles(&self) -> u32 {
        self.table.borrow().compiles()
    }
}

impl Default for RegexCache {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for RegexCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = f.debug_struct("RegexCache");
        match self.table.try_borrow() {
            Ok(table) => out
                .field("len", &table.len())
                .field("capacity", &table.capacity),
            Err(_) => out.field("len", &format_args!("<in use>")),
        };
        out.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real compilation of a pattern that is known to be valid — what the
    /// production closure does, minus the dialect pre-pass these tests are not
    /// about.
    fn ok(pattern: &str) -> Result<Regex, XPathError> {
        Regex::xpath(pattern, "").map_err(|_| XPathError::invalid_regex_pattern(pattern))
    }

    #[test]
    fn a_repeated_key_compiles_once() {
        let mut cache = RunCache::default();
        let mut compiles = 0;
        for _ in 0..50 {
            let result = cache.get_or_compile("a+", "", || {
                compiles += 1;
                ok("a+")
            });
            assert!(result.unwrap().is_match("aaa"));
        }
        assert_eq!(compiles, 1);
        assert_eq!(cache.compiles(), 1);
        assert_eq!(cache.hits(), 49);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn flags_are_part_of_the_key() {
        let mut cache = RunCache::default();
        let mut compiles = 0;
        for flags in ["", "i", "", "i"] {
            let _ = cache.get_or_compile("a", flags, || {
                compiles += 1;
                Regex::xpath("a", flags).map_err(|_| XPathError::invalid_regex_pattern("a"))
            });
        }
        assert_eq!(compiles, 2);
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn a_failure_is_replayed_unchanged() {
        let mut cache = RunCache::default();
        let mut compiles = 0;
        let mut seen = Vec::new();
        for _ in 0..5 {
            let result = cache.get_or_compile("[", "", || {
                compiles += 1;
                Err(XPathError::invalid_regex_pattern("["))
            });
            seen.push(format!("{:?}", result.map(|_| ()).unwrap_err()));
        }
        assert_eq!(compiles, 1);
        assert!(seen.windows(2).all(|pair| pair[0] == pair[1]), "{seen:?}");
    }

    #[test]
    fn the_table_never_grows_past_the_cap() {
        let mut cache = RunCache::default();
        let mut compiles = 0;
        for i in 0..MAX_ENTRIES * 4 {
            let pattern = format!("x{i}y");
            let result = cache.get_or_compile(&pattern, "", || {
                compiles += 1;
                ok(&pattern)
            });
            assert!(result.unwrap().is_match(&format!("x{i}y")));
            assert!(cache.len() <= MAX_ENTRIES, "{} entries", cache.len());
        }
        // Every key was new, so every call compiled and nothing was ever hit.
        assert_eq!(compiles, MAX_ENTRIES * 4);
        assert_eq!(cache.hits(), 0);
        assert_eq!(cache.len(), MAX_ENTRIES);
    }

    #[test]
    fn the_least_recently_used_key_is_the_one_dropped() {
        let mut cache = RunCache::default();
        let keys: Vec<String> = (0..MAX_ENTRIES).map(|i| format!("k{i}")).collect();
        for key in &keys {
            let _ = cache.get_or_compile(key, "", || ok(key));
        }
        assert_eq!(cache.len(), MAX_ENTRIES);

        // Touch the oldest entry: it becomes the most recently used, and the
        // one inserted just after it becomes the oldest.
        let _ = cache.get_or_compile(&keys[0], "", || panic!("must be a hit"));
        assert_eq!(cache.hits(), 1);

        // One new key evicts exactly one entry — the one now least recently
        // used, which is `k1`, not the key just touched.
        let _ = cache.get_or_compile("fresh", "", || ok("fresh"));
        assert_eq!(cache.len(), MAX_ENTRIES);
        let resident = cache.keys();
        assert!(resident.contains(&("fresh", "")));
        assert!(resident.contains(&(keys[0].as_str(), "")));
        assert!(!resident.contains(&(keys[1].as_str(), "")));
    }

    #[test]
    fn a_host_cache_compiles_once_across_calls() {
        let cache = RegexCache::new();
        let mut compiles = 0;
        for _ in 0..50 {
            let regex = cache
                .get_or_compile("a+", "", || {
                    compiles += 1;
                    ok("a+")
                })
                .unwrap();
            assert!(regex.is_match("aaa"));
        }
        assert_eq!(compiles, 1);
        assert_eq!(cache.compiles(), 1);
        assert_eq!(cache.hits(), 49);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn a_host_cache_keeps_its_bound_and_replays_failures() {
        let cache = RegexCache::with_capacity(4);
        assert_eq!(cache.capacity(), 4);
        for i in 0..20 {
            let pattern = format!("p{i}");
            let _ = cache.get_or_compile(&pattern, "", || ok(&pattern));
            assert!(cache.len() <= 4);
        }
        assert_eq!(cache.len(), 4);

        let mut compiles = 0;
        for _ in 0..3 {
            let err = cache
                .get_or_compile("[", "", || {
                    compiles += 1;
                    Err(XPathError::invalid_regex_pattern("["))
                })
                .unwrap_err();
            assert!(matches!(err, XPathError::FORX0002 { .. }));
        }
        assert_eq!(compiles, 1);
    }

    #[test]
    fn a_zero_capacity_is_taken_as_one() {
        let cache = RegexCache::with_capacity(0);
        assert_eq!(cache.capacity(), 1);
        for pattern in ["a", "b", "a"] {
            assert!(cache
                .get_or_compile(pattern, "", || ok(pattern))
                .unwrap()
                .is_match(pattern));
        }
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.compiles(), 3);
    }

    #[test]
    fn clearing_a_host_cache_keeps_programs_in_use_alive() {
        let cache = RegexCache::new();
        let held = cache.get_or_compile("x+", "", || ok("x+")).unwrap();
        cache.clear();
        assert!(cache.is_empty());
        assert!(held.is_match("xx"));
        let again = cache.get_or_compile("x+", "", || ok("x+")).unwrap();
        assert!(again.is_match("x"));
        assert_eq!(cache.compiles(), 2);
    }

    #[test]
    fn a_borrowed_host_cache_compiles_without_storing() {
        let cache = RegexCache::new();
        let _busy = cache.table.borrow_mut();
        let regex = cache.get_or_compile("y", "", || ok("y")).unwrap();
        assert!(regex.is_match("y"));
        drop(_busy);
        assert!(cache.is_empty());
    }

    #[test]
    fn a_host_cache_is_send_and_debug() {
        fn assert_send<T: Send>() {}
        assert_send::<RegexCache>();
        let cache = RegexCache::with_capacity(8);
        let _ = cache.get_or_compile("z", "", || ok("z"));
        assert_eq!(format!("{cache:?}"), "RegexCache { len: 1, capacity: 8 }");
    }
}
