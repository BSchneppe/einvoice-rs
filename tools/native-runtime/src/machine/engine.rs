use super::atomic::{comparison, numeric};
use super::meter::{MachineLimits, MachineUsage, Meter};
use super::prepare::{capability, declaration_type, sequence_type, Prepared, FN, XS};
use super::source::{axis, SourceNode};
use crate::error::{resource, size_overflow};
use crate::{
    Arithmetic, Atomic, AtomicType, Comparison, Error, Failure, Fault, Limits, Resource, Result,
    Scope, Text, Usage,
};
use einvoice_source::{QueryBudget, SourceDocument};
use std::cell::Cell;
use std::cmp::Ordering;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use validation_ir::{Expr, Expression};

static RUN_ID: AtomicU64 = AtomicU64::new(1);
/// Entry focus can only reference the immutable source document, or be absent.
#[derive(Clone, Copy, Debug)]
pub enum EntryFocus<'d> {
    /// Document node, position/size 1/1.
    Document,
    /// A source occurrence, position/size 1/1; foreign documents are refused.
    Node(SourceNode<'d>),
    /// Undefined context item, position and size.
    Absent,
}
/// Independent replay position owned by its run. Fields cannot be forged.
#[derive(Clone, Copy, Debug)]
pub struct Cursor {
    run: u64,
    index: usize,
}
/// Read-only reference to an admitted item in this run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ItemHandle {
    run: u64,
    index: usize,
}
/// Borrowed output observation; there is no public item injection API.
#[derive(Debug)]
pub enum ItemRef<'r, 'd> {
    /// Scope-owned atom.
    Atomic(&'r Atomic),
    /// Original source occurrence.
    Node(SourceNode<'d>),
}
/// Original expression and exact AST construct for an evaluation failure.
#[derive(Clone, Copy, Debug)]
pub struct Origin<'a> {
    /// Retains the compiler's text and XML source location.
    pub expression: &'a Expression,
    /// Exact borrowed AST identity, without reconstruction.
    pub construct: &'a Expr,
}
#[derive(Clone, Copy)]
enum Size {
    Known(usize),
    Sequence(usize),
}
#[derive(Clone, Copy)]
struct Focus {
    item: Option<usize>,
    position: usize,
    size: Size,
}
#[derive(Clone, Copy)]
struct Capture {
    env: Option<usize>,
    focus: Focus,
    proximity: bool,
}
struct Env {
    slot: usize,
    seq: usize,
    parent: Option<usize>,
}
enum Item<'d> {
    Atomic(Atomic),
    Node(SourceNode<'d>),
}
struct Sequence {
    state: State,
    cache: Vec<usize>,
    terminal: Option<Result<()>>,
    active: bool,
    origin: Option<usize>,
    error_origin: Option<usize>,
    numeric_predicate: bool,
}
struct Replay {
    seq: usize,
    index: usize,
}
struct Frame {
    seq: usize,
    index: usize,
    reply: Option<Option<usize>>,
    owns_active: bool,
}
enum Action {
    Need(usize, usize),
    Emit(usize),
    NumericPredicate(usize),
    Done,
    Again,
}
#[derive(Clone, Copy)]
enum Scalar {
    Unary(bool),
    Arithmetic(Arithmetic),
    ValueComparison(Comparison),
    NodeComparison(u8),
    Cast(AtomicType, bool, bool),
    Constructor(AtomicType),
    String,
    Number,
    Abs,
    Round,
}
struct ScalarState {
    op: Scalar,
    args: [Option<usize>; 2],
    values: [Option<usize>; 2],
    arg: usize,
    second: bool,
    many: bool,
}
struct Collect {
    seq: usize,
    index: usize,
    values: Vec<usize>,
}
struct Path {
    left: usize,
    right: usize,
    capture: Capture,
    descendant: bool,
    phase: u8,
    kind: u8,
    input: Vec<usize>,
    output: Vec<usize>,
    index: usize,
    child: Option<usize>,
    cursor: usize,
}
struct Filter {
    base: usize,
    predicate: usize,
    capture: Capture,
    index: usize,
    current: Option<usize>,
    test: Option<usize>,
    stop: bool,
    done: bool,
}
struct TupleLevel {
    seq: usize,
    index: usize,
    env: Option<usize>,
}
struct Tuples {
    node: usize,
    capture: Capture,
    levels: Vec<TupleLevel>,
    body: Option<usize>,
    cursor: usize,
    kind: u8,
}
enum State {
    Start(usize, Capture),
    Busy,
    Ready(Vec<usize>, usize),
    Alias(usize, usize),
    Concat {
        node: usize,
        capture: Capture,
        next: usize,
        child: Option<usize>,
        index: usize,
    },
    Ebv {
        seq: usize,
        first: Option<usize>,
        second: bool,
    },
    Count {
        seq: usize,
        count: usize,
    },
    Predicate {
        seq: usize,
        position: usize,
        first: Option<usize>,
        second: bool,
    },
    Exists {
        seq: usize,
        empty: bool,
    },
    Boolean {
        seq: usize,
        invert: bool,
    },
    If {
        test: usize,
        yes: usize,
        no: usize,
        capture: Capture,
    },
    Logical {
        test: usize,
        right: usize,
        capture: Capture,
        or: bool,
        second: bool,
    },
    Scalar(ScalarState),
    Data {
        seq: usize,
        index: usize,
    },
    Convert {
        seq: usize,
        target: AtomicType,
        min: usize,
        max: Option<usize>,
        index: usize,
        first: Option<usize>,
        second: bool,
    },
    Sum {
        seq: usize,
        index: usize,
        sum: Option<usize>,
    },
    General {
        left: usize,
        right: usize,
        li: usize,
        ri: usize,
        current: Option<usize>,
        op: Comparison,
    },
    Range {
        start: ScalarState,
    },
    RangeItems {
        next: usize,
        end: usize,
        increment: bool,
    },
    Normalize(Collect),
    Union {
        left: Collect,
        right: usize,
        phase: bool,
    },
    Path(Path),
    Filter(Filter),
    Tuples(Tuples),
}

/// One non-resettable document run. Flat arenas own all retained recipes,
/// environments, cache cells and items. Scope/query fuel persists across pulls.
pub struct Run<'p, 'a, 'd> {
    program: &'p Prepared<'a>,
    doc: &'d SourceDocument,
    scope: Scope,
    query: QueryBudget,
    source_output: usize,
    meter: Meter,
    id: u64,
    sequences: Vec<Sequence>,
    items: Vec<Item<'d>>,
    environments: Vec<Env>,
    globals: Vec<usize>,
    cursors: Vec<Replay>,
    root: usize,
    poison: Cell<Option<Error>>,
    origin: Cell<Option<usize>>,
    source_work: usize,
}
impl<'p, 'a, 'd> Run<'p, 'a, 'd> {
    /// Create independent finite numeric, expression and source-query allowances.
    /// No source parsing, schema mutation, or executable-package promotion occurs.
    pub fn new(
        program: &'p Prepared<'a>,
        doc: &'d SourceDocument,
        entry: EntryFocus<'d>,
        atomic: Limits,
        machine: MachineLimits,
        source_work: usize,
        source_output: usize,
    ) -> Result<Self> {
        let id = RUN_ID
            .fetch_update(AtomicOrdering::Relaxed, AtomicOrdering::Relaxed, |n| {
                n.checked_add(1)
            })
            .map_err(|_| size_overflow())?;
        let mut run = Self {
            program,
            doc,
            scope: Scope::new(atomic),
            query: QueryBudget::new(source_work, source_output).map_err(source_error)?,
            source_output,
            meter: Meter::new(machine)?,
            id,
            sequences: Vec::new(),
            items: Vec::new(),
            environments: Vec::new(),
            globals: Vec::new(),
            cursors: Vec::new(),
            root: 0,
            poison: Cell::new(None),
            origin: Cell::new(None),
            source_work,
        };
        let document = run.item(Item::Node(SourceNode::Node(doc.document())))?;
        let focus = match entry {
            EntryFocus::Document => Focus {
                item: Some(document),
                position: 1,
                size: Size::Known(1),
            },
            EntryFocus::Absent => Focus {
                item: None,
                position: 0,
                size: Size::Known(0),
            },
            EntryFocus::Node(n) => {
                if !n.belongs(doc) {
                    return Err(Error::Fatal(Failure::ForeignScope));
                }
                let item = run.item(Item::Node(n))?;
                Focus {
                    item: Some(item),
                    position: 1,
                    size: Size::Known(1),
                }
            }
        };
        // All global slots exist before any initializer can be demanded.
        for _ in 0..program.slots {
            run.meter.push(&mut run.globals, usize::MAX)?;
        }
        for declaration in &program.declarations {
            let captured = if matches!(
                declaration.binding.effective_scope,
                validation_ir::Scope::Xslt2SchemaGlobal
            ) {
                Focus {
                    item: Some(document),
                    position: 1,
                    size: Size::Known(1),
                }
            } else {
                focus
            };
            let seq = run.expression(
                declaration.root,
                Capture {
                    env: None,
                    focus: captured,
                    proximity: false,
                },
            )?;
            let seq = if let Some(t) = &declaration.binding.sequence_type {
                let (target, min, max) = declaration_type(t)?;
                run.sequence(
                    State::Convert {
                        seq,
                        target,
                        min,
                        max,
                        index: 0,
                        first: None,
                        second: false,
                    },
                    Some(declaration.root),
                )?
            } else {
                seq
            };
            run.globals[declaration.slot] = seq;
        }
        run.root = run.expression(
            program.root,
            Capture {
                env: None,
                focus,
                proximity: false,
            },
        )?;
        Ok(run)
    }
    /// Create a fresh independent cursor over the entry sequence.
    pub fn cursor(&mut self) -> Result<Cursor> {
        self.healthy()?;
        let result = (|| {
            self.meter.object(size_of::<Replay>())?;
            self.meter.reserve(&mut self.cursors)?;
            let index = self.cursors.len();
            self.cursors.push(Replay {
                seq: self.root,
                index: 0,
            });
            Ok(Cursor {
                run: self.id,
                index,
            })
        })();
        self.record_observation(result)
    }
    /// Demand one item; prefix access does not force an undemanded tail.
    /// Fatal errors poison this run; XPath errors remain replayable terminals.
    pub fn next(&mut self, cursor: Cursor) -> Result<Option<ItemHandle>> {
        self.healthy()?;
        if cursor.run != self.id || cursor.index >= self.cursors.len() {
            return self.record_observation(Err(Error::Fatal(Failure::ForeignScope)));
        }
        let result = (|| {
            let c = self
                .cursors
                .get(cursor.index)
                .ok_or(Error::Fatal(Failure::ForeignScope))?;
            let (seq, index) = (c.seq, c.index);
            match self.pull(seq, index)? {
                Some(item) => {
                    self.cursors[cursor.index].index = inc(index)?;
                    Ok(Some(ItemHandle {
                        run: self.id,
                        index: item,
                    }))
                }
                None => Ok(None),
            }
        })();
        self.record(result)
    }
    /// Observe an item without copying or bypassing atomic metering. Valid
    /// retained items remain observable after failure; foreign handles latch a
    /// first fatal failure just like mutating operations.
    pub fn item_ref(&self, item: ItemHandle) -> Result<ItemRef<'_, 'd>> {
        if item.run != self.id {
            return self.record_observation(Err(Error::Fatal(Failure::ForeignScope)));
        }
        let item = self.record_observation(
            self.items
                .get(item.index)
                .ok_or(Error::Fatal(Failure::ForeignScope)),
        )?;
        match item {
            Item::Atomic(a) => Ok(ItemRef::Atomic(a)),
            Item::Node(n) => Ok(ItemRef::Node(*n)),
        }
    }
    /// Metered XPath string-value observation (original source atomization for nodes).
    pub fn render(&mut self, item: ItemHandle) -> Result<Text> {
        // Replaying a latched evaluation failure preserves its original evidence.
        self.healthy()?;
        let result = (|| {
            if item.run != self.id {
                return Err(Error::Fatal(Failure::ForeignScope));
            }
            let index = self.atomize(item.index)?;
            self.scope.render(atom(&self.items, index)?)
        })();
        self.record_observation(result)
    }
    /// Machine logical storage/work usage.
    pub fn usage(&self) -> MachineUsage {
        self.meter.usage
    }
    /// Persistent atomic/numeric accounting.
    pub fn atomic_usage(&self) -> Usage {
        self.scope.usage()
    }
    /// Original failing evaluation construct, if any. A newly failed external
    /// observation/admission has no AST origin. Successful observations preserve
    /// earlier evaluation evidence; fatal replay preserves the first failure.
    pub fn error_origin(&self) -> Option<Origin<'a>> {
        self.origin.get().map(|i| Origin {
            expression: self.program.nodes[i].owner,
            construct: self.program.nodes[i].expr,
        })
    }
    fn record<T>(&self, result: Result<T>) -> Result<T> {
        if let Err(e @ Error::Fatal(_)) = result {
            if let Some(first) = self.poison.get() {
                return Err(first);
            }
            self.poison.set(Some(e));
        }
        result
    }
    fn record_observation<T>(&self, result: Result<T>) -> Result<T> {
        if result.is_err() && self.poison.get().is_none() {
            self.origin.set(None);
        }
        self.record(result)
    }
    fn healthy(&self) -> Result<()> {
        self.poison.get().map_or(Ok(()), Err)
    }
    fn item(&mut self, item: Item<'d>) -> Result<usize> {
        self.meter.object(size_of::<Item<'_>>())?;
        self.meter.reserve(&mut self.items)?;
        let i = self.items.len();
        self.items.push(item);
        Ok(i)
    }
    fn boolean(&mut self, b: bool) -> Result<usize> {
        let a = self
            .scope
            .lexical(AtomicType::Boolean, if b { "true" } else { "false" })?;
        self.item(Item::Atomic(a))
    }
    fn integer(&mut self, n: usize) -> Result<usize> {
        // usize decimal spelling is bounded by 3*sizeof(usize), precharged.
        self.meter.work(size_of::<usize>() * 3)?;
        let bytes = size_of::<usize>() * 3;
        self.meter.bytes(bytes)?;
        let a = self.scope.lexical(AtomicType::Integer, &n.to_string())?;
        self.meter.release(bytes);
        self.item(Item::Atomic(a))
    }
    fn sequence(&mut self, state: State, origin: Option<usize>) -> Result<usize> {
        self.meter.object(size_of::<Sequence>())?;
        self.meter.reserve(&mut self.sequences)?;
        let index = self.sequences.len();
        self.sequences.push(Sequence {
            state,
            cache: Vec::new(),
            terminal: None,
            active: false,
            origin,
            error_origin: None,
            numeric_predicate: false,
        });
        Ok(index)
    }
    fn expression(&mut self, node: usize, capture: Capture) -> Result<usize> {
        if !capture.proximity && self.program.nodes[node].reverse_chain {
            let seq = self.sequence(
                State::Start(
                    node,
                    Capture {
                        proximity: true,
                        ..capture
                    },
                ),
                Some(node),
            )?;
            self.sequence(
                State::Normalize(Collect {
                    seq,
                    index: 0,
                    values: Vec::new(),
                }),
                Some(node),
            )
        } else {
            self.sequence(State::Start(node, capture), Some(node))
        }
    }
    fn singleton(&mut self, item: usize) -> Result<usize> {
        let mut items = Vec::new();
        self.meter.push(&mut items, item)?;
        self.sequence(State::Ready(items, 0), None)
    }
    fn ebv(&mut self, seq: usize) -> Result<usize> {
        self.sequence(
            State::Ebv {
                seq,
                first: None,
                second: false,
            },
            self.sequences[seq].origin,
        )
    }
    fn binding(&mut self, slot: usize, mut env: Option<usize>) -> Result<usize> {
        while let Some(e) = env {
            self.meter.work(1)?;
            let frame = &self.environments[e];
            if frame.slot == slot {
                return Ok(frame.seq);
            }
            env = frame.parent;
        }
        self.globals
            .get(slot)
            .copied()
            .filter(|n| *n != usize::MAX)
            .ok_or(Error::Fatal(Failure::Internal("binding scope")))
    }
    fn bind(&mut self, slot: usize, seq: usize, parent: Option<usize>) -> Result<Option<usize>> {
        self.meter.object(size_of::<Env>())?;
        self.meter.reserve(&mut self.environments)?;
        let index = self.environments.len();
        self.environments.push(Env { slot, seq, parent });
        Ok(Some(index))
    }
    fn pull(&mut self, seq: usize, index: usize) -> Result<Option<usize>> {
        let mut stack = Vec::new();
        self.origin.set(None);
        self.push_frame(&mut stack, seq, index)?;
        loop {
            let top = stack.len() - 1;
            let (s, i) = (stack[top].seq, stack[top].index);
            if let Err(e) = self.meter.work(1) {
                self.fail_stack(&stack, s, e);
                return Err(e);
            }
            let cached = self.sequences[s].cache.get(i).copied();
            let terminal = self.sequences[s].terminal;
            let answer = if let Some(item) = cached {
                Some(Ok(Some(item)))
            } else {
                terminal.map(|t| t.map(|()| None))
            };
            if let Some(answer) = answer {
                if stack[top].owns_active {
                    self.sequences[s].active = false;
                }
                stack.pop();
                self.meter.release(size_of::<Frame>());
                match answer {
                    Ok(reply) => {
                        if let Some(parent) = stack.last_mut() {
                            parent.reply = Some(reply);
                        } else {
                            return Ok(reply);
                        }
                    }
                    Err(e) => {
                        self.fail_stack(&stack, s, e);
                        return Err(e);
                    }
                }
                continue;
            }
            let state = std::mem::replace(&mut self.sequences[s].state, State::Busy);
            let origin = self.sequences[s].origin;
            match self.poll(state, stack[top].reply.take(), origin) {
                Ok((state, action)) => {
                    self.sequences[s].state = state;
                    match action {
                        Action::Need(child, index) => {
                            if let Err(e) = self.push_frame(&mut stack, child, index) {
                                self.fail_stack(&stack, s, e);
                                return Err(e);
                            }
                        }
                        Action::Emit(item) => {
                            if let Err(e) = self.meter.push(&mut self.sequences[s].cache, item) {
                                self.fail_stack(&stack, s, e);
                                return Err(e);
                            }
                        }
                        Action::NumericPredicate(item) => {
                            self.sequences[s].numeric_predicate = true;
                            if let Err(e) = self.meter.push(&mut self.sequences[s].cache, item) {
                                self.fail_stack(&stack, s, e);
                                return Err(e);
                            }
                        }
                        Action::Done => self.sequences[s].terminal = Some(Ok(())),
                        Action::Again => {}
                    }
                }
                Err(e) => {
                    self.fail_stack(&stack, s, e);
                    return Err(e);
                }
            }
        }
    }
    fn push_frame(&mut self, stack: &mut Vec<Frame>, seq: usize, index: usize) -> Result<()> {
        self.meter
            .depth(stack.len().checked_add(1).ok_or_else(size_overflow)?)?;
        if self.sequences[seq].active && self.sequences[seq].cache.len() <= index {
            return Err(Error::XPath(Fault::Xtde0640));
        }
        let owns_active =
            self.sequences[seq].cache.len() <= index && self.sequences[seq].terminal.is_none();
        self.meter.push(
            stack,
            Frame {
                seq,
                index,
                reply: None,
                owns_active,
            },
        )?;
        if owns_active {
            self.sequences[seq].active = true;
        }
        Ok(())
    }
    fn fail_stack(&mut self, stack: &[Frame], failing: usize, error: Error) {
        let origin = self.sequences[failing]
            .error_origin
            .or(self.sequences[failing].origin);
        self.origin.set(origin);
        self.sequences[failing].error_origin = origin;
        self.sequences[failing].terminal = Some(Err(error));
        self.sequences[failing].active = false;
        for f in stack {
            self.sequences[f.seq].error_origin = origin;
            self.sequences[f.seq].terminal = Some(Err(error));
            self.sequences[f.seq].active = false;
        }
        self.meter.release(std::mem::size_of_val(stack));
    }
    fn start(&mut self, node: usize, cap: Capture) -> Result<State> {
        let plan = &self.program.nodes[node];
        let child = |i: usize| plan.children[i];
        Ok(match plan.expr {
            Expr::Number {
                lexical_kind,
                lexeme,
            } => {
                let a = self.scope.literal(lexical_kind, lexeme)?;
                let i = self.item(Item::Atomic(a))?;
                return self.ready_one(i);
            }
            Expr::String { value } => {
                let a = self.scope.lexical(AtomicType::String, value)?;
                let i = self.item(Item::Atomic(a))?;
                return self.ready_one(i);
            }
            Expr::Context => {
                let i = cap.focus.item.ok_or(Error::XPath(Fault::Xpdy0002))?;
                return self.ready_one(i);
            }
            Expr::Root => {
                let i = cap.focus.item.ok_or(Error::XPath(Fault::Xpdy0002))?;
                if !matches!(self.items[i], Item::Node(_)) {
                    return Err(Error::XPath(Fault::Xpty0020));
                }
                let i = self.item(Item::Node(SourceNode::Node(self.doc.document())))?;
                return self.ready_one(i);
            }
            Expr::Variable { .. } => State::Alias(
                self.binding(
                    plan.variable.ok_or(capability("unresolved binding"))?,
                    cap.env,
                )?,
                0,
            ),
            Expr::Group { .. } => State::Alias(
                self.expression(
                    child(0),
                    Capture {
                        proximity: false,
                        ..cap
                    },
                )?,
                0,
            ),
            Expr::Step {
                axis: axis_name,
                test,
            } => {
                let i = cap.focus.item.ok_or(Error::XPath(Fault::Xpdy0002))?;
                let Item::Node(n) = self.items[i] else {
                    return Err(Error::XPath(Fault::Xpty0020));
                };
                let nodes = axis(self.doc, n, axis_name, test, cap.proximity, &mut self.meter)?;
                let bytes = nodes
                    .len()
                    .checked_mul(size_of::<SourceNode<'_>>())
                    .ok_or_else(size_overflow)?;
                let mut out = Vec::new();
                for n in nodes {
                    let item = self.item(Item::Node(n))?;
                    self.meter.push(&mut out, item)?;
                }
                self.meter.release(bytes);
                State::Ready(out, 0)
            }
            Expr::Sequence { .. } => State::Concat {
                node,
                capture: cap,
                next: 0,
                child: None,
                index: 0,
            },
            Expr::If { .. } => {
                let seq = self.expression(child(0), cap)?;
                let test = self.ebv(seq)?;
                State::If {
                    test,
                    yes: child(1),
                    no: child(2),
                    capture: cap,
                }
            }
            Expr::Unary { operator, .. } => {
                let a = self.expression(child(0), cap)?;
                State::Scalar(scalar(Scalar::Unary(operator == "-"), a, None))
            }
            Expr::TypeOp {
                operator,
                sequence_type: t,
                ..
            } => {
                let (target, optional) = sequence_type(t)?;
                let a = self.expression(child(0), cap)?;
                State::Scalar(scalar(
                    Scalar::Cast(target, optional, operator == "castable"),
                    a,
                    None,
                ))
            }
            Expr::Filter { .. } => {
                if let Expr::Number {
                    lexical_kind,
                    lexeme,
                } = self.program.nodes[child(1)].expr
                {
                    let a = self.scope.literal(lexical_kind, lexeme)?;
                    let zero = self.scope.literal(&crate::NumericKind::Integer, "0")?;
                    if self
                        .scope
                        .numeric_compare(Comparison::LessEqual, &a, &zero)?
                    {
                        return Ok(self.done());
                    }
                    let integral = match self.scope.cast(&a, AtomicType::Integer) {
                        Ok(integer) => {
                            self.scope
                                .numeric_compare(Comparison::Equal, &a, &integer)?
                        }
                        Err(Error::XPath(Fault::Foca0002)) => false,
                        Err(e) => return Err(e),
                    };
                    if !integral {
                        return Ok(self.done());
                    }
                }
                let base = self.expression(
                    child(0),
                    Capture {
                        proximity: cap.proximity,
                        ..cap
                    },
                )?;
                State::Filter(Filter {
                    base,
                    predicate: child(1),
                    capture: cap,
                    index: 0,
                    current: None,
                    test: None,
                    stop: !self.program.nodes[child(1)].focus_dependent,
                    done: false,
                })
            }
            Expr::Bind { operator, .. } => State::Tuples(Tuples {
                node,
                capture: cap,
                levels: Vec::new(),
                body: None,
                cursor: 0,
                kind: match operator.as_str() {
                    "some" => 1,
                    "every" => 2,
                    _ => 0,
                },
            }),
            Expr::Binary { operator, .. } => {
                let left = self.expression(
                    child(0),
                    Capture {
                        proximity: false,
                        ..cap
                    },
                )?;
                match operator.as_str() {
                    "and" | "or" => {
                        let test = self.ebv(left)?;
                        State::Logical {
                            test,
                            right: child(1),
                            capture: cap,
                            or: operator == "or",
                            second: false,
                        }
                    }
                    "/" | "//" => State::Path(Path {
                        left,
                        right: child(1),
                        capture: cap,
                        descendant: operator == "//",
                        phase: 0,
                        kind: 0,
                        input: Vec::new(),
                        output: Vec::new(),
                        index: 0,
                        child: None,
                        cursor: 0,
                    }),
                    _ => {
                        let right = self.expression(
                            child(1),
                            Capture {
                                proximity: false,
                                ..cap
                            },
                        )?;
                        match operator.as_str() {
                            "|" => State::Union {
                                left: Collect {
                                    seq: left,
                                    index: 0,
                                    values: Vec::new(),
                                },
                                right,
                                phase: false,
                            },
                            "=" | "!=" | "<" | "<=" | ">" | ">=" => State::General {
                                left,
                                right,
                                li: 0,
                                ri: 0,
                                current: None,
                                op: comparison(operator),
                            },
                            "eq" | "ne" | "lt" | "le" | "gt" | "ge" => State::Scalar(scalar(
                                Scalar::ValueComparison(comparison(operator)),
                                left,
                                Some(right),
                            )),
                            "is" | "<<" | ">>" => State::Scalar(scalar(
                                Scalar::NodeComparison(match operator.as_str() {
                                    "is" => 0,
                                    "<<" => 1,
                                    _ => 2,
                                }),
                                left,
                                Some(right),
                            )),
                            "to" => State::Range {
                                start: scalar(
                                    Scalar::Constructor(AtomicType::Integer),
                                    left,
                                    Some(right),
                                ),
                            },
                            op => State::Scalar(scalar(
                                Scalar::Arithmetic(match op {
                                    "+" => Arithmetic::Add,
                                    "-" => Arithmetic::Subtract,
                                    "*" => Arithmetic::Multiply,
                                    "div" => Arithmetic::Divide,
                                    "idiv" => Arithmetic::IntegerDivide,
                                    "mod" => Arithmetic::Modulo,
                                    _ => return Err(capability("binary dispatch")),
                                }),
                                left,
                                Some(right),
                            )),
                        }
                    }
                }
            }
            Expr::Call { name, .. } => {
                if name.namespace == XS {
                    let a = self.expression(child(0), cap)?;
                    State::Scalar(scalar(
                        Scalar::Constructor(Scope::resolve_type(&name.namespace, &name.local)?),
                        a,
                        None,
                    ))
                } else if name.namespace == FN {
                    match name.local.as_str() {
                        "true" | "false" => {
                            let i = self.boolean(name.local == "true")?;
                            return self.ready_one(i);
                        }
                        "position" => {
                            cap.focus.item.ok_or(Error::XPath(Fault::Xpdy0002))?;
                            let i = self.integer(cap.focus.position)?;
                            return self.ready_one(i);
                        }
                        "last" => {
                            cap.focus.item.ok_or(Error::XPath(Fault::Xpdy0002))?;
                            match cap.focus.size {
                                Size::Known(size) => {
                                    let i = self.integer(size)?;
                                    return self.ready_one(i);
                                }
                                Size::Sequence(seq) => State::Count { seq, count: 0 },
                            }
                        }
                        _ => {
                            let a = if plan.children.is_empty() {
                                self.singleton(
                                    cap.focus.item.ok_or(Error::XPath(Fault::Xpdy0002))?,
                                )?
                            } else {
                                self.expression(child(0), cap)?
                            };
                            match name.local.as_str() {
                                "exists" | "empty" => State::Exists {
                                    seq: a,
                                    empty: name.local == "empty",
                                },
                                "count" => State::Count { seq: a, count: 0 },
                                "boolean" | "not" => {
                                    let seq = self.ebv(a)?;
                                    State::Boolean {
                                        seq,
                                        invert: name.local == "not",
                                    }
                                }
                                "data" => State::Data { seq: a, index: 0 },
                                "sum" => State::Sum {
                                    seq: a,
                                    index: 0,
                                    sum: None,
                                },
                                local => State::Scalar(scalar(
                                    match local {
                                        "string" => Scalar::String,
                                        "number" => Scalar::Number,
                                        "abs" => Scalar::Abs,
                                        "round" => Scalar::Round,
                                        _ => return Err(capability("builtin dispatch")),
                                    },
                                    a,
                                    None,
                                )),
                            }
                        }
                    }
                } else {
                    return Err(capability("function namespace"));
                }
            }
        })
    }
    fn ready_one(&mut self, i: usize) -> Result<State> {
        let mut v = Vec::new();
        self.meter.push(&mut v, i)?;
        Ok(State::Ready(v, 0))
    }
    fn poll(
        &mut self,
        state: State,
        reply: Option<Option<usize>>,
        origin: Option<usize>,
    ) -> Result<(State, Action)> {
        match state {
            State::Start(node, cap) => Ok((self.start(node, cap)?, Action::Again)),
            State::Busy => Err(Error::Fatal(Failure::Internal("recursive recipe poll"))),
            State::Ready(values, index) => {
                let action = values
                    .get(index)
                    .copied()
                    .map_or(Action::Done, Action::Emit);
                Ok((
                    State::Ready(values, index.checked_add(1).ok_or_else(size_overflow)?),
                    action,
                ))
            }
            State::Alias(seq, index) => match reply {
                None => Ok((State::Alias(seq, index), Action::Need(seq, index))),
                Some(Some(item)) => Ok((State::Alias(seq, inc(index)?), Action::Emit(item))),
                Some(None) => Ok((State::Alias(seq, index), Action::Done)),
            },
            State::Concat {
                node,
                capture,
                mut next,
                mut child,
                mut index,
            } => {
                if let Some(value) = reply {
                    if let Some(item) = value {
                        index = inc(index)?;
                        return Ok((
                            State::Concat {
                                node,
                                capture,
                                next,
                                child,
                                index,
                            },
                            Action::Emit(item),
                        ));
                    }
                    child = None;
                    index = 0;
                }
                if child.is_none() {
                    if let Some(n) = self.program.nodes[node].children.get(next) {
                        child = Some(self.expression(*n, capture)?);
                        next = inc(next)?;
                    } else {
                        return Ok((
                            State::Concat {
                                node,
                                capture,
                                next,
                                child,
                                index,
                            },
                            Action::Done,
                        ));
                    }
                }
                let seq = child.ok_or(capability("concat invariant"))?;
                Ok((
                    State::Concat {
                        node,
                        capture,
                        next,
                        child,
                        index,
                    },
                    Action::Need(seq, index),
                ))
            }
            State::Ebv { seq, first, second } => {
                if let Some(value) = reply {
                    if second {
                        if value.is_some() {
                            return Err(Error::XPath(Fault::Forg0006));
                        }
                        let i = first.ok_or(Error::Fatal(Failure::Internal("EBV first")))?;
                        let b = self.scope.singleton_ebv(atom(&self.items, i)?)?;
                        let i = self.boolean(b)?;
                        return Ok((self.done(), Action::Emit(i)));
                    }
                    match value {
                        None => {
                            let i = self.boolean(false)?;
                            return Ok((self.done(), Action::Emit(i)));
                        }
                        Some(i) if matches!(self.items[i], Item::Node(_)) => {
                            let i = self.boolean(true)?;
                            return Ok((self.done(), Action::Emit(i)));
                        }
                        Some(i) => {
                            return Ok((
                                State::Ebv {
                                    seq,
                                    first: Some(i),
                                    second: true,
                                },
                                Action::Need(seq, 1),
                            ))
                        }
                    }
                }
                Ok((State::Ebv { seq, first, second }, Action::Need(seq, 0)))
            }
            State::Predicate {
                seq,
                position,
                first,
                second,
            } => {
                if let Some(value) = reply {
                    if second {
                        let first = required(first)?;
                        if let Item::Atomic(a) = &self.items[first] {
                            if numeric(a.atomic_type()) {
                                if value.is_some() {
                                    return Err(Error::XPath(Fault::Forg0006));
                                }
                                let pos = self.integer(position)?;
                                let b = self.scope.numeric_compare(
                                    Comparison::Equal,
                                    atom(&self.items, first)?,
                                    atom(&self.items, pos)?,
                                )?;
                                let i = self.boolean(b)?;
                                return Ok((self.done(), Action::NumericPredicate(i)));
                            }
                        }
                        if value.is_some() {
                            return Err(Error::XPath(Fault::Forg0006));
                        }
                        let b = self.scope.singleton_ebv(atom(&self.items, first)?)?;
                        let i = self.boolean(b)?;
                        return Ok((self.done(), Action::Emit(i)));
                    }
                    match value {
                        None => {
                            let i = self.boolean(false)?;
                            return Ok((self.done(), Action::Emit(i)));
                        }
                        Some(i) if matches!(self.items[i], Item::Node(_)) => {
                            let i = self.boolean(true)?;
                            return Ok((self.done(), Action::Emit(i)));
                        }
                        Some(i) => {
                            return Ok((
                                State::Predicate {
                                    seq,
                                    position,
                                    first: Some(i),
                                    second: true,
                                },
                                Action::Need(seq, 1),
                            ))
                        }
                    }
                }
                Ok((
                    State::Predicate {
                        seq,
                        position,
                        first,
                        second,
                    },
                    Action::Need(seq, 0),
                ))
            }
            State::Count { seq, mut count } => {
                if let Some(value) = reply {
                    if value.is_some() {
                        count = inc(count)?;
                    } else {
                        let i = self.integer(count)?;
                        return Ok((self.done(), Action::Emit(i)));
                    }
                }
                Ok((State::Count { seq, count }, Action::Need(seq, count)))
            }
            State::Exists { seq, empty } => match reply {
                None => Ok((State::Exists { seq, empty }, Action::Need(seq, 0))),
                Some(value) => {
                    let i = self.boolean(value.is_none() == empty)?;
                    Ok((self.done(), Action::Emit(i)))
                }
            },
            State::Boolean { seq, invert } => match reply {
                None => Ok((State::Boolean { seq, invert }, Action::Need(seq, 0))),
                Some(value) => {
                    let b = atom(&self.items, required(value)?)?
                        .boolean()
                        .ok_or(capability("EBV boolean invariant"))?;
                    let i = self.boolean(b != invert)?;
                    Ok((self.done(), Action::Emit(i)))
                }
            },
            State::If {
                test,
                yes,
                no,
                capture,
            } => match reply {
                None => Ok((
                    State::If {
                        test,
                        yes,
                        no,
                        capture,
                    },
                    Action::Need(test, 0),
                )),
                Some(value) => {
                    let b = atom(&self.items, required(value)?)?
                        .boolean()
                        .ok_or(capability("if EBV invariant"))?;
                    let seq = self.expression(if b { yes } else { no }, capture)?;
                    Ok((State::Alias(seq, 0), Action::Again))
                }
            },
            State::Logical {
                test,
                right,
                capture,
                or,
                second,
            } => match reply {
                None => Ok((
                    State::Logical {
                        test,
                        right,
                        capture,
                        or,
                        second,
                    },
                    Action::Need(test, 0),
                )),
                Some(value) => {
                    let i = required(value)?;
                    let b = atom(&self.items, i)?
                        .boolean()
                        .ok_or(capability("logical EBV invariant"))?;
                    if second || b == or {
                        Ok((self.done(), Action::Emit(i)))
                    } else {
                        let seq = self.expression(right, capture)?;
                        let test = self.ebv(seq)?;
                        Ok((
                            State::Logical {
                                test,
                                right,
                                capture,
                                or,
                                second: true,
                            },
                            Action::Again,
                        ))
                    }
                }
            },
            State::Scalar(mut scalar) => {
                if let Some(action) = self.gather(&mut scalar, reply)? {
                    return Ok((State::Scalar(scalar), action));
                }
                let result = self.scalar_result(&scalar)?;
                Ok((self.done(), result.map_or(Action::Done, Action::Emit)))
            }
            State::Convert {
                seq,
                target,
                min,
                max,
                mut index,
                first,
                second,
            } => {
                if let Some(value) = reply {
                    if second {
                        if value.is_some() {
                            return Err(Error::XPath(Fault::Xtte0570));
                        }
                        let i = self.function_convert(required(first)?, target)?;
                        return Ok((self.done(), Action::Emit(i)));
                    }
                    if let Some(i) = value {
                        if max == Some(1) {
                            return Ok((
                                State::Convert {
                                    seq,
                                    target,
                                    min,
                                    max,
                                    index,
                                    first: Some(i),
                                    second: true,
                                },
                                Action::Need(seq, 1),
                            ));
                        }
                        let i = self.function_convert(i, target)?;
                        index = inc(index)?;
                        return Ok((
                            State::Convert {
                                seq,
                                target,
                                min,
                                max,
                                index,
                                first: None,
                                second: false,
                            },
                            Action::Emit(i),
                        ));
                    }
                    if index < min {
                        return Err(Error::XPath(Fault::Xtte0570));
                    }
                    return Ok((self.done(), Action::Done));
                }
                Ok((
                    State::Convert {
                        seq,
                        target,
                        min,
                        max,
                        index,
                        first,
                        second,
                    },
                    Action::Need(seq, index),
                ))
            }
            State::Data { seq, mut index } => {
                if let Some(value) = reply {
                    if let Some(i) = value {
                        index = inc(index)?;
                        let i = self.atomize(i)?;
                        return Ok((State::Data { seq, index }, Action::Emit(i)));
                    }
                    return Ok((self.done(), Action::Done));
                }
                Ok((State::Data { seq, index }, Action::Need(seq, index)))
            }
            State::Sum {
                seq,
                mut index,
                mut sum,
            } => {
                if let Some(value) = reply {
                    if let Some(i) = value {
                        let i = self.numeric_argument(i, Fault::Forg0006)?;
                        if let Some(s) = sum {
                            let a = self.scope.arithmetic(
                                Arithmetic::Add,
                                atom(&self.items, s)?,
                                atom(&self.items, i)?,
                            )?;
                            sum = Some(self.item(Item::Atomic(a))?);
                        } else {
                            sum = Some(i);
                        }
                        index = inc(index)?;
                    } else {
                        let i = match sum {
                            Some(i) => i,
                            None => self.integer(0)?,
                        };
                        return Ok((self.done(), Action::Emit(i)));
                    }
                }
                Ok((State::Sum { seq, index, sum }, Action::Need(seq, index)))
            }
            State::General {
                left,
                right,
                mut li,
                mut ri,
                mut current,
                op,
            } => {
                if let Some(value) = reply {
                    if let Some(a) = current {
                        if let Some(b) = value {
                            let b = self.atomize(b)?;
                            if self.scope.machine_compare(
                                op,
                                atom(&self.items, a)?,
                                atom(&self.items, b)?,
                                true,
                            )? {
                                let i = self.boolean(true)?;
                                return Ok((self.done(), Action::Emit(i)));
                            }
                            ri = inc(ri)?;
                        } else {
                            if ri == 0 {
                                let i = self.boolean(false)?;
                                return Ok((self.done(), Action::Emit(i)));
                            }
                            current = None;
                            li = inc(li)?;
                            ri = 0;
                        }
                    } else if let Some(i) = value {
                        current = Some(self.atomize(i)?);
                    } else {
                        let i = self.boolean(false)?;
                        return Ok((self.done(), Action::Emit(i)));
                    }
                }
                let need = if current.is_some() {
                    Action::Need(right, ri)
                } else {
                    Action::Need(left, li)
                };
                Ok((
                    State::General {
                        left,
                        right,
                        li,
                        ri,
                        current,
                        op,
                    },
                    need,
                ))
            }
            State::Range { mut start } => {
                if let Some(action) = self.gather(&mut start, reply)? {
                    return Ok((State::Range { start }, action));
                }
                if let (Some(a), Some(b)) = (start.values[0], start.values[1]) {
                    let a = self.range_endpoint(a)?;
                    let b = self.range_endpoint(b)?;
                    Ok((
                        State::RangeItems {
                            next: a,
                            end: b,
                            increment: false,
                        },
                        Action::Again,
                    ))
                } else {
                    Ok((self.done(), Action::Done))
                }
            }
            State::RangeItems {
                mut next,
                end,
                increment,
            } => {
                if increment
                    && self.scope.numeric_compare(
                        Comparison::Equal,
                        atom(&self.items, next)?,
                        atom(&self.items, end)?,
                    )?
                {
                    return Ok((self.done(), Action::Done));
                }
                if increment {
                    let one = self.integer(1)?;
                    let a = self.scope.arithmetic(
                        Arithmetic::Add,
                        atom(&self.items, next)?,
                        atom(&self.items, one)?,
                    )?;
                    next = self.item(Item::Atomic(a))?;
                }
                if self.scope.numeric_compare(
                    Comparison::Greater,
                    atom(&self.items, next)?,
                    atom(&self.items, end)?,
                )? {
                    return Ok((self.done(), Action::Done));
                }
                Ok((
                    State::RangeItems {
                        next,
                        end,
                        increment: true,
                    },
                    Action::Emit(next),
                ))
            }
            State::Normalize(mut collect) => {
                if let Some(value) = reply {
                    if let Some(i) = value {
                        self.meter.push(&mut collect.values, i)?;
                        collect.index = inc(collect.index)?;
                    } else {
                        self.sort_nodes(&mut collect.values, Fault::Xpty0004)?;
                        return Ok((State::Ready(collect.values, 0), Action::Again));
                    }
                }
                let need = Action::Need(collect.seq, collect.index);
                Ok((State::Normalize(collect), need))
            }
            State::Union {
                mut left,
                right,
                mut phase,
            } => {
                if let Some(value) = reply {
                    if let Some(i) = value {
                        if !matches!(self.items[i], Item::Node(_)) {
                            return Err(Error::XPath(Fault::Xpty0004));
                        }
                        self.meter.push(&mut left.values, i)?;
                        left.index = inc(left.index)?;
                    } else if phase {
                        self.sort_nodes(&mut left.values, Fault::Xpty0004)?;
                        return Ok((State::Ready(left.values, 0), Action::Again));
                    } else {
                        phase = true;
                        left.seq = right;
                        left.index = 0;
                    }
                }
                let need = Action::Need(left.seq, left.index);
                Ok((State::Union { left, right, phase }, need))
            }
            State::Path(path) => self.poll_path(path, reply),
            State::Filter(filter) => self.poll_filter(filter, reply, origin),
            State::Tuples(tuples) => self.poll_tuples(tuples, reply),
        }
    }
    fn done(&self) -> State {
        State::Ready(Vec::new(), 0)
    }
    fn gather(
        &mut self,
        s: &mut ScalarState,
        reply: Option<Option<usize>>,
    ) -> Result<Option<Action>> {
        if let Some(value) = reply {
            if s.second {
                if value.is_some() {
                    if matches!(s.op, Scalar::Cast(_, _, true)) {
                        s.many = true;
                        s.arg = 2;
                        return Ok(None);
                    }
                    return Err(Error::XPath(Fault::Xpty0004));
                }
                s.arg = inc(s.arg)?;
                s.second = false;
            } else {
                s.values[s.arg] = value;
                if value.is_some() {
                    s.second = true;
                } else {
                    s.arg = inc(s.arg)?;
                }
            }
        }
        while s.arg < s.args.len() && s.args[s.arg].is_none() {
            s.arg = inc(s.arg)?;
        }
        if s.arg == s.args.len() {
            return Ok(None);
        }
        Ok(Some(Action::Need(
            s.args[s.arg].ok_or(capability("scalar argument"))?,
            usize::from(s.second),
        )))
    }
    fn scalar_result(&mut self, s: &ScalarState) -> Result<Option<usize>> {
        let [a, b] = s.values;
        if let Scalar::Cast(target, optional, castable) = s.op {
            if s.many {
                return Ok(Some(self.boolean(false)?));
            }
            let Some(a) = a else {
                return if castable {
                    Ok(Some(self.boolean(optional)?))
                } else if optional {
                    Ok(None)
                } else {
                    Err(Error::XPath(Fault::Xpty0004))
                };
            };
            let a = self.atomize(a)?; // operand/atomization errors are outside castability recovery
            if castable {
                let b = self.scope.castable(atom(&self.items, a)?, target)?;
                return Ok(Some(self.boolean(b)?));
            }
            let a = self.scope.cast(atom(&self.items, a)?, target)?;
            return Ok(Some(self.item(Item::Atomic(a))?));
        }
        if matches!(s.op, Scalar::String) {
            let value = if let Some(a) = a {
                let a = self.atomize(a)?;
                self.scope.cast(atom(&self.items, a)?, AtomicType::String)?
            } else {
                self.scope.lexical(AtomicType::String, "")?
            };
            return Ok(Some(self.item(Item::Atomic(value))?));
        }
        if matches!(s.op, Scalar::Number) {
            let a = a.map(|i| self.atomize(i)).transpose()?;
            let value = self
                .scope
                .number(a.map(|i| atom(&self.items, i)).transpose()?)?;
            return Ok(Some(self.item(Item::Atomic(value))?));
        }
        let Some(a) = a else {
            return Ok(None);
        };
        if let Scalar::NodeComparison(op) = s.op {
            let Some(b) = b else {
                return Ok(None);
            };
            let (Item::Node(a), Item::Node(b)) = (&self.items[a], &self.items[b]) else {
                return Err(Error::XPath(Fault::Xpty0004));
            };
            self.meter.work(1)?;
            let order = a
                .order()
                .partial_cmp(&b.order())
                .ok_or(Error::Fatal(Failure::ForeignScope))?;
            return Ok(Some(self.boolean(match op {
                0 => a.identity() == b.identity(),
                1 => order == Ordering::Less,
                _ => order == Ordering::Greater,
            })?));
        }
        let a = self.atomize(a)?;
        let value = match s.op {
            Scalar::Constructor(target) => self.scope.cast(atom(&self.items, a)?, target)?,
            Scalar::Unary(negative) => {
                let a = self.numeric_argument(a, Fault::Xpty0004)?;
                if negative {
                    self.scope.negate(atom(&self.items, a)?)?
                } else {
                    return Ok(Some(a));
                }
            }
            Scalar::Abs | Scalar::Round => {
                let a = self.numeric_argument(a, Fault::Xpty0004)?;
                if matches!(s.op, Scalar::Abs) {
                    self.scope.abs(atom(&self.items, a)?)?
                } else {
                    self.scope.round(atom(&self.items, a)?, 0)?
                }
            }
            Scalar::Arithmetic(op) => {
                let Some(b) = b else {
                    return Ok(None);
                };
                let b = self.atomize(b)?;
                self.scope
                    .arithmetic(op, atom(&self.items, a)?, atom(&self.items, b)?)?
            }
            Scalar::ValueComparison(op) => {
                let Some(b) = b else {
                    return Ok(None);
                };
                let b = self.atomize(b)?;
                let result = self.scope.machine_compare(
                    op,
                    atom(&self.items, a)?,
                    atom(&self.items, b)?,
                    false,
                )?;
                return Ok(Some(self.boolean(result)?));
            }
            _ => return Err(Error::Fatal(Failure::Internal("scalar dispatch"))),
        };
        Ok(Some(self.item(Item::Atomic(value))?))
    }
    fn function_convert(&mut self, item: usize, target: AtomicType) -> Result<usize> {
        let i = self.atomize(item)?;
        let a = atom(&self.items, i)?;
        let kind = a.atomic_type();
        if kind == target || (target == AtomicType::Decimal && kind == AtomicType::Integer) {
            return Ok(i);
        }
        if kind == AtomicType::UntypedAtomic
            || (target == AtomicType::Double && numeric(kind))
            || (target == AtomicType::Float
                && matches!(kind, AtomicType::Integer | AtomicType::Decimal))
        {
            let a = self.scope.cast(a, target).map_err(|error| match error {
                Error::XPath(Fault::Forg0001) => Error::XPath(Fault::Xtte0570),
                other => other,
            })?;
            return self.item(Item::Atomic(a));
        }
        Err(Error::XPath(Fault::Xtte0570))
    }
    fn numeric_argument(&mut self, item: usize, fault: Fault) -> Result<usize> {
        let i = self.atomize(item)?;
        let a = atom(&self.items, i)?;
        if a.atomic_type() == AtomicType::UntypedAtomic {
            let a = self.scope.cast(a, AtomicType::Double)?;
            self.item(Item::Atomic(a))
        } else if numeric(a.atomic_type()) {
            Ok(i)
        } else {
            Err(Error::XPath(fault))
        }
    }
    fn range_endpoint(&mut self, i: usize) -> Result<usize> {
        let i = self.atomize(i)?;
        let a = atom(&self.items, i)?;
        if a.atomic_type() == AtomicType::UntypedAtomic {
            let a = self.scope.cast(a, AtomicType::Integer)?;
            self.item(Item::Atomic(a))
        } else if a.atomic_type() == AtomicType::Integer {
            Ok(i)
        } else {
            Err(Error::XPath(Fault::Xpty0004))
        }
    }
    fn atomize(&mut self, index: usize) -> Result<usize> {
        let Item::Node(n) = self.items[index] else {
            return Ok(index);
        };
        if !n.belongs(self.doc) {
            return Err(Error::Fatal(Failure::ForeignScope));
        }
        let source_work = match n {
            SourceNode::Attribute(a) => a.value().len().checked_add(1),
            SourceNode::Namespace(n) => n.uri().len().checked_add(1),
            SourceNode::Node(_) => self
                .doc
                .nodes()
                .len()
                .checked_add(self.doc.original_bytes().len()),
        }
        .ok_or_else(size_overflow)?;
        self.source_work = self
            .source_work
            .checked_sub(source_work)
            .ok_or_else(|| resource(Resource::SourceQuery))?;
        let (a, bytes) = match n {
            SourceNode::Attribute(a) => {
                self.source_output = self
                    .source_output
                    .checked_sub(a.value().len())
                    .ok_or_else(|| resource(Resource::SourceQuery))?;
                (self.scope.lexical(AtomicType::UntypedAtomic, a.value())?, 0)
            }
            SourceNode::Namespace(n) => {
                self.source_output = self
                    .source_output
                    .checked_sub(n.uri().len())
                    .ok_or_else(|| resource(Resource::SourceQuery))?;
                (self.scope.lexical(AtomicType::String, n.uri())?, 0)
            }
            SourceNode::Node(n) => {
                let mut sink = SourceSink {
                    text: String::new(),
                    meter: &mut self.meter,
                    remaining: &mut self.source_output,
                    error: None,
                };
                let result = n.write_string_value(&mut sink, &mut self.query);
                let text = sink.text;
                let own_error = sink.error;
                if let Some(error) = own_error {
                    return Err(error);
                }
                result.map_err(source_error)?;
                let target = if matches!(
                    n.kind(),
                    einvoice_source::NodeKind::Comment
                        | einvoice_source::NodeKind::ProcessingInstruction
                ) {
                    AtomicType::String
                } else {
                    AtomicType::UntypedAtomic
                };
                let bytes = text.len();
                let a = self.scope.lexical(target, &text)?;
                (a, bytes)
            }
        };
        self.meter.release(bytes);
        self.item(Item::Atomic(a))
    }
    fn sort_nodes(&mut self, values: &mut Vec<usize>, fault: Fault) -> Result<()> {
        // Fallible insertion order: each comparison is charged, no unmetered sort callback.
        for i in 0..values.len() {
            if !matches!(self.items[values[i]], Item::Node(_)) {
                return Err(Error::XPath(fault));
            }
            let mut j = i;
            while j > 0 {
                self.meter.work(1)?;
                let (Item::Node(a), Item::Node(b)) =
                    (&self.items[values[j - 1]], &self.items[values[j]])
                else {
                    return Err(Error::XPath(fault));
                };
                let order = a
                    .order()
                    .partial_cmp(&b.order())
                    .ok_or(Error::Fatal(Failure::ForeignScope))?;
                if order != Ordering::Greater {
                    break;
                }
                values.swap(j - 1, j);
                j -= 1;
            }
        }
        let mut write = 0usize;
        for read in 0..values.len() {
            self.meter.work(1)?;
            let duplicate = if write == 0 {
                false
            } else {
                let (Item::Node(a), Item::Node(b)) =
                    (&self.items[values[write - 1]], &self.items[values[read]])
                else {
                    return Err(Error::XPath(fault));
                };
                a.identity() == b.identity()
            };
            if !duplicate {
                values[write] = values[read];
                write = inc(write)?;
            }
        }
        // Retain the admitted allocation; truncate only logical contents.
        values.truncate(write);
        Ok(())
    }
    fn poll_path(&mut self, mut p: Path, reply: Option<Option<usize>>) -> Result<(State, Action)> {
        if p.phase == 0 {
            if let Some(value) = reply {
                if let Some(i) = value {
                    let Item::Node(n) = self.items[i] else {
                        return Err(Error::XPath(Fault::Xpty0019));
                    };
                    if p.descendant {
                        let nodes = axis(
                            self.doc,
                            n,
                            "descendant-or-self",
                            &validation_ir::NodeTest::Kind {
                                name: "node".into(),
                            },
                            false,
                            &mut self.meter,
                        )?;
                        let bytes = nodes
                            .len()
                            .checked_mul(size_of::<SourceNode<'_>>())
                            .ok_or_else(size_overflow)?;
                        for n in nodes {
                            let i = self.item(Item::Node(n))?;
                            self.meter.push(&mut p.input, i)?;
                        }
                        self.meter.release(bytes);
                    } else {
                        self.meter.push(&mut p.input, i)?;
                    }
                    p.index = inc(p.index)?;
                } else {
                    self.sort_nodes(&mut p.input, Fault::Xpty0019)?;
                    p.phase = 1;
                    p.index = 0;
                }
            }
            if p.phase == 0 {
                let need = Action::Need(p.left, p.index);
                return Ok((State::Path(p), need));
            }
        } else if let Some(value) = reply {
            if let Some(i) = value {
                let kind = if matches!(self.items[i], Item::Node(_)) {
                    1
                } else {
                    2
                };
                if p.kind != 0 && p.kind != kind {
                    return Err(Error::XPath(Fault::Xpty0018));
                }
                p.kind = kind;
                p.cursor = inc(p.cursor)?;
                if kind == 2 {
                    return Ok((State::Path(p), Action::Emit(i)));
                }
                self.meter.push(&mut p.output, i)?;
            } else {
                p.child = None;
                p.cursor = 0;
                p.index = inc(p.index)?;
            }
        }
        if p.child.is_none() {
            if let Some(i) = p.input.get(p.index) {
                let focus = Focus {
                    item: Some(*i),
                    position: inc(p.index)?,
                    size: Size::Known(p.input.len()),
                };
                p.child = Some(self.expression(
                    p.right,
                    Capture {
                        focus,
                        proximity: false,
                        ..p.capture
                    },
                )?);
            } else {
                if p.kind == 1 {
                    self.sort_nodes(&mut p.output, Fault::Xpty0018)?;
                }
                return Ok((State::Ready(p.output, 0), Action::Again));
            }
        }
        let need = Action::Need(p.child.ok_or(capability("path cursor"))?, p.cursor);
        Ok((State::Path(p), need))
    }
    fn poll_filter(
        &mut self,
        mut f: Filter,
        reply: Option<Option<usize>>,
        origin: Option<usize>,
    ) -> Result<(State, Action)> {
        if f.done {
            return Ok((State::Filter(f), Action::Done));
        }
        if let Some(value) = reply {
            if f.test.is_some() {
                let i = required(value)?;
                let yes = atom(&self.items, i)?
                    .boolean()
                    .ok_or(capability("predicate EBV"))?;
                let numeric =
                    self.sequences[f.test.ok_or(capability("predicate recipe"))?].numeric_predicate;
                f.test = None;
                let current = f.current.take().ok_or(capability("predicate current"))?;
                if yes {
                    f.done = f.stop && numeric;
                    return Ok((State::Filter(f), Action::Emit(current)));
                }
            } else {
                let Some(i) = value else {
                    return Ok((State::Filter(f), Action::Done));
                };
                f.current = Some(i);
                f.index = inc(f.index)?;
                let focus = Focus {
                    item: Some(i),
                    position: f.index,
                    size: Size::Sequence(f.base),
                };
                let seq = self.expression(
                    f.predicate,
                    Capture {
                        focus,
                        proximity: false,
                        ..f.capture
                    },
                )?;
                // A predicate has special numeric-singleton semantics; use its own recipe.
                let test = self.sequence(
                    State::Predicate {
                        seq,
                        position: f.index,
                        first: None,
                        second: false,
                    },
                    origin,
                )?;
                f.test = Some(test);
                return Ok((State::Filter(f), Action::Need(test, 0)));
            }
        }
        let need = Action::Need(f.base, f.index);
        Ok((State::Filter(f), need))
    }
    fn poll_tuples(
        &mut self,
        mut t: Tuples,
        reply: Option<Option<usize>>,
    ) -> Result<(State, Action)> {
        if let Some(value) = reply {
            if t.body.is_some() {
                if let Some(i) = value {
                    if t.kind != 0 {
                        let b = atom(&self.items, i)?
                            .boolean()
                            .ok_or(capability("quantifier EBV"))?;
                        t.body = None;
                        t.cursor = 0;
                        if b == (t.kind == 1) {
                            return Ok((self.done(), Action::Emit(i)));
                        }
                    } else {
                        t.cursor = inc(t.cursor)?;
                        return Ok((State::Tuples(t), Action::Emit(i)));
                    }
                } else {
                    t.body = None;
                    t.cursor = 0;
                }
            } else {
                let level = t
                    .levels
                    .len()
                    .checked_sub(1)
                    .ok_or(capability("tuple level"))?;
                if let Some(i) = value {
                    t.levels[level].index = inc(t.levels[level].index)?;
                    let seq = self.singleton(i)?;
                    let env = self.bind(
                        self.program.nodes[t.node].slots[level],
                        seq,
                        t.levels[level].env,
                    )?;
                    if inc(level)? == self.program.nodes[t.node].slots.len() {
                        let result = *self.program.nodes[t.node]
                            .children
                            .last()
                            .ok_or(capability("tuple result"))?;
                        let seq = self.expression(result, Capture { env, ..t.capture })?;
                        t.body = Some(if t.kind == 0 { seq } else { self.ebv(seq)? });
                    } else {
                        self.meter
                            .depth(t.levels.len().checked_add(1).ok_or_else(size_overflow)?)?;
                        let seq = self.expression(
                            self.program.nodes[t.node].children[inc(level)?],
                            Capture { env, ..t.capture },
                        )?;
                        self.meter
                            .push(&mut t.levels, TupleLevel { seq, index: 0, env })?;
                    }
                } else {
                    t.levels.pop();
                    self.meter.release(size_of::<TupleLevel>());
                    if t.levels.is_empty() {
                        let action = if t.kind == 0 {
                            Action::Done
                        } else {
                            Action::Emit(self.boolean(t.kind == 2)?)
                        };
                        return Ok((self.done(), action));
                    }
                }
            }
        }
        if t.levels.is_empty() {
            let seq = self.expression(self.program.nodes[t.node].children[0], t.capture)?;
            self.meter.push(
                &mut t.levels,
                TupleLevel {
                    seq,
                    index: 0,
                    env: t.capture.env,
                },
            )?;
        }
        let need = if let Some(seq) = t.body {
            Action::Need(seq, t.cursor)
        } else {
            let l = t.levels.last().ok_or(capability("tuple continuation"))?;
            Action::Need(l.seq, l.index)
        };
        Ok((State::Tuples(t), need))
    }
}
fn scalar(op: Scalar, left: usize, right: Option<usize>) -> ScalarState {
    ScalarState {
        op,
        args: [Some(left), right],
        values: [None, None],
        arg: 0,
        second: false,
        many: false,
    }
}
fn atom<'r>(items: &'r [Item<'_>], i: usize) -> Result<&'r Atomic> {
    match &items[i] {
        Item::Atomic(a) => Ok(a),
        _ => Err(Error::Fatal(Failure::Internal("expected atomic item"))),
    }
}
fn required(item: Option<usize>) -> Result<usize> {
    item.ok_or(Error::Fatal(Failure::Internal("required machine item")))
}
fn inc(n: usize) -> Result<usize> {
    n.checked_add(1).ok_or_else(size_overflow)
}
fn source_error(error: einvoice_source::SourceError) -> Error {
    match error {
        einvoice_source::SourceError::Resource(_) => resource(Resource::SourceQuery),
        einvoice_source::SourceError::InvalidLimits(_) => resource(Resource::InvalidLimits),
        _ => Error::Fatal(Failure::Internal("immutable source query failed")),
    }
}
struct SourceSink<'m> {
    text: String,
    meter: &'m mut Meter,
    remaining: &'m mut usize,
    error: Option<Error>,
}
impl std::fmt::Write for SourceSink<'_> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        let result = (|| {
            *self.remaining = self
                .remaining
                .checked_sub(s.len())
                .ok_or_else(|| resource(Resource::SourceQuery))?;
            self.meter.work(s.len())?;
            self.meter.bytes(s.len())?;
            if self.text.capacity().saturating_sub(self.text.len()) < s.len() {
                self.meter.bytes(self.text.len())?;
                self.meter.work(self.text.len())?;
            }
            self.text
                .try_reserve_exact(s.len())
                .map_err(|_| resource(Resource::Machine("allocation")))?;
            self.text.push_str(s);
            Ok(())
        })();
        if let Err(error) = result {
            self.error = Some(error);
            return Err(std::fmt::Error);
        }
        Ok(())
    }
}
