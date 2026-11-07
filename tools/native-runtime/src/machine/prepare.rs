use super::meter::{MachineLimits, MachineUsage, Meter};
use crate::{AtomicType, Error, Failure, Result, Scope};
use validation_ir::{Binding, BindingValue, Expr, Expression, Name, NodeTest, SequenceType};

pub(super) const FN: &str = "http://www.w3.org/2005/xpath-functions";
pub(super) const XS: &str = "http://www.w3.org/2001/XMLSchema";
pub(super) fn capability(s: &'static str) -> Error {
    Error::Fatal(Failure::Capability(s))
}
pub(super) struct PlanNode<'a> {
    pub expr: &'a Expr,
    pub owner: &'a Expression,
    pub children: Vec<usize>,
    pub slots: Vec<usize>,
    pub variable: Option<usize>,
    pub reverse_chain: bool,
    pub focus_dependent: bool,
}
pub(super) struct Declaration<'a> {
    pub binding: &'a Binding,
    pub root: usize,
    pub slot: usize,
}
/// A checked, borrowed view of original AST/source/binding identities. This is
/// neither a serialized IR nor a package admission or source-resolution API.
pub struct Prepared<'a> {
    pub(super) nodes: Vec<PlanNode<'a>>,
    pub(super) declarations: Vec<Declaration<'a>>,
    pub(super) root: usize,
    pub(super) slots: usize,
    usage: MachineUsage,
}
struct Lexical<'a> {
    id: &'a str,
    name: &'a Name,
    slot: usize,
    parent: Option<usize>,
}
struct Visit {
    node: usize,
    env: Option<usize>,
    depth: usize,
}
fn string(meter: &mut Meter, s: &str) -> Result<()> {
    meter.work(s.len())
}
fn name(meter: &mut Meter, n: &Name) -> Result<()> {
    string(meter, &n.namespace)?;
    string(meter, &n.local)?;
    string(meter, &n.qualified)
}
pub(super) fn declaration_type(t: &SequenceType) -> Result<(AtomicType, usize, Option<usize>)> {
    let target = Scope::resolve_type(&t.name.namespace, &t.name.local)?;
    let (min, max) = match t.occurrence.as_deref() {
        None => (1, Some(1)),
        Some("?") => (0, Some(1)),
        Some("*") => (0, None),
        Some("+") => (1, None),
        _ => return Err(capability("declared occurrence")),
    };
    Ok((target, min, max))
}
pub(super) fn sequence_type(t: &SequenceType) -> Result<(AtomicType, bool)> {
    let target = Scope::resolve_type(&t.name.namespace, &t.name.local)?;
    match t.occurrence.as_deref() {
        None => Ok((target, false)),
        Some("?") => Ok((target, true)),
        _ => Err(capability("cast occurrence other than singleton/optional")),
    }
}
fn supported(expr: &Expr, meter: &mut Meter) -> Result<()> {
    match expr {
        Expr::Number { lexeme, .. } => string(meter, lexeme)?,
        Expr::String { value } => string(meter, value)?,
        Expr::Variable { name: n, binding } => {
            name(meter, n)?;
            string(meter, binding)?;
        }
        Expr::Step { axis, test } => {
            string(meter, axis)?;
            if ![
                "self",
                "parent",
                "child",
                "attribute",
                "namespace",
                "ancestor",
                "ancestor-or-self",
                "descendant",
                "descendant-or-self",
                "preceding-sibling",
                "following-sibling",
            ]
            .contains(&axis.as_str())
            {
                return Err(capability("axis"));
            }
            match test {
                NodeTest::Name { name: n } => name(meter, n)?,
                NodeTest::Namespace { namespace, prefix } => {
                    string(meter, namespace)?;
                    string(meter, prefix)?;
                }
                NodeTest::Local { local } => string(meter, local)?,
                NodeTest::Kind { name: n } => {
                    string(meter, n)?;
                    if !["node", "text", "comment", "processing-instruction"].contains(&n.as_str())
                    {
                        return Err(capability("node kind test"));
                    }
                }
                NodeTest::Any => {}
            }
        }
        Expr::Call { name: n, arguments } => {
            name(meter, n)?;
            let arity = arguments.len();
            let allowed = if n.namespace == XS {
                arity == 1 && Scope::resolve_type(&n.namespace, &n.local).is_ok()
            } else if n.namespace == FN {
                match n.local.as_str() {
                    "true" | "false" | "position" | "last" => arity == 0,
                    "string" | "number" => arity <= 1,
                    "exists" | "empty" | "count" | "boolean" | "not" | "data" | "abs" | "round"
                    | "sum" => arity == 1,
                    _ => false,
                }
            } else {
                false
            };
            if !allowed {
                return Err(capability("function expanded name/arity"));
            }
        }
        Expr::Unary { operator, .. } => {
            string(meter, operator)?;
            if !["+", "-"].contains(&operator.as_str()) {
                return Err(capability("unary operator"));
            }
        }
        Expr::Binary { operator, .. } => {
            string(meter, operator)?;
            if ![
                "+", "-", "*", "div", "idiv", "mod", "to", "=", "!=", "<", "<=", ">", ">=", "eq",
                "ne", "lt", "le", "gt", "ge", "is", "<<", ">>", "and", "or", "/", "//", "|",
            ]
            .contains(&operator.as_str())
            {
                return Err(capability("binary operator"));
            }
        }
        Expr::Bind {
            operator, bindings, ..
        } => {
            string(meter, operator)?;
            if !["for", "some", "every"].contains(&operator.as_str()) || bindings.is_empty() {
                return Err(capability("binding operator/empty tuple"));
            }
            for b in bindings {
                string(meter, &b.id)?;
                name(meter, &b.name)?;
            }
        }
        Expr::TypeOp {
            operator,
            sequence_type: t,
            ..
        } => {
            string(meter, operator)?;
            name(meter, &t.name)?;
            if let Some(o) = &t.occurrence {
                string(meter, o)?;
            }
            if !["cast", "castable"].contains(&operator.as_str()) {
                return Err(capability("type operator"));
            }
            sequence_type(t)?;
        }
        _ => {}
    }
    Ok(())
}
fn schedule(meter: &mut Meter, todo: &mut Vec<Visit>, value: Visit) -> Result<()> {
    meter.depth(value.depth)?;
    meter.depth(
        todo.len()
            .checked_add(1)
            .ok_or_else(crate::error::size_overflow)?,
    )?;
    meter.push(todo, value)
}
fn add<'a>(
    nodes: &mut Vec<PlanNode<'a>>,
    expr: &'a Expr,
    owner: &'a Expression,
    meter: &mut Meter,
) -> Result<usize> {
    meter.object(size_of::<PlanNode<'_>>())?;
    meter.reserve(nodes)?;
    let id = nodes.len();
    nodes.push(PlanNode {
        expr,
        owner,
        children: Vec::new(),
        slots: Vec::new(),
        variable: None,
        reverse_chain: false,
        focus_dependent: false,
    });
    Ok(id)
}
fn extend<'a>(
    envs: &mut Vec<Lexical<'a>>,
    id: &'a str,
    n: &'a Name,
    slot: usize,
    parent: Option<usize>,
    meter: &mut Meter,
) -> Result<Option<usize>> {
    meter.object(size_of::<Lexical<'_>>())?;
    meter.reserve(envs)?;
    let index = envs.len();
    envs.push(Lexical {
        id,
        name: n,
        slot,
        parent,
    });
    Ok(Some(index))
}
impl<'a> Prepared<'a> {
    /// Admit the complete entry expression and supplied select declarations,
    /// including unused initializers and dead branches. Global initializers get
    /// document-root focus; rule-local selects capture the run's entry focus.
    /// Helper/content declarations remain explicit later-layer gaps.
    pub fn new(
        entry: &'a Expression,
        bindings: &'a [Binding],
        limits: MachineLimits,
    ) -> Result<Self> {
        let mut meter = Meter::new(limits)?;
        let mut nodes = Vec::new();
        let mut declarations = Vec::new();
        let mut envs = Vec::new();
        let mut env = None;
        let mut slots = bindings.len();
        // Declaration identity is unique even when lexical names shadow.
        for (i, b) in bindings.iter().enumerate() {
            string(&mut meter, &b.id)?;
            name(&mut meter, &b.name)?;
            if let Some(t) = &b.sequence_type {
                name(&mut meter, &t.name)?;
                if let Some(o) = &t.occurrence {
                    string(&mut meter, o)?;
                }
                declaration_type(t)?;
            }
            if !matches!(
                b.effective_scope,
                validation_ir::Scope::Xslt2SchemaGlobal | validation_ir::Scope::RuleLocal
            ) {
                return Err(capability("helper declaration"));
            }
            if !matches!(b.value, BindingValue::Select { .. }) {
                return Err(capability("content/parameter declaration"));
            }
            for previous in &bindings[..i] {
                meter.work(b.id.len().max(previous.id.len()))?;
                if previous.id == b.id {
                    return Err(capability("duplicate binding identity"));
                }
            }
        }
        for (slot, b) in bindings
            .iter()
            .enumerate()
            .filter(|(_, b)| matches!(b.effective_scope, validation_ir::Scope::Xslt2SchemaGlobal))
        {
            env = extend(&mut envs, &b.id, &b.name, slot, env, &mut meter)?;
        }
        let globals_env = env;
        let mut initializers = Vec::new();
        for (slot, b) in bindings.iter().enumerate() {
            if matches!(b.effective_scope, validation_ir::Scope::RuleLocal) {
                meter.push(&mut initializers, env)?;
                env = extend(&mut envs, &b.id, &b.name, slot, env, &mut meter)?;
            } else {
                meter.push(&mut initializers, globals_env)?;
            }
        }
        let root = add(&mut nodes, &entry.tree, entry, &mut meter)?;
        let mut todo = Vec::new();
        schedule(
            &mut meter,
            &mut todo,
            Visit {
                node: root,
                env,
                depth: 1,
            },
        )?;
        for (slot, b) in bindings.iter().enumerate() {
            let BindingValue::Select { expression } = &b.value else {
                return Err(capability("select declaration"));
            };
            let id = add(&mut nodes, &expression.tree, expression, &mut meter)?;
            meter.push(
                &mut declarations,
                Declaration {
                    binding: b,
                    root: id,
                    slot,
                },
            )?;
            schedule(
                &mut meter,
                &mut todo,
                Visit {
                    node: id,
                    env: initializers[slot],
                    depth: 1,
                },
            )?;
        }
        meter.release(
            initializers
                .len()
                .checked_mul(size_of::<Option<usize>>())
                .ok_or_else(crate::error::size_overflow)?,
        );
        while let Some(visit) = todo.pop() {
            meter.release(size_of::<Visit>());
            meter.depth(visit.depth)?;
            meter.work(1)?;
            let expr = nodes[visit.node].expr;
            let owner = nodes[visit.node].owner;
            supported(expr, &mut meter)?;
            if let Expr::Variable { binding, name: n } = expr {
                let mut e = visit.env;
                while let Some(i) = e {
                    let lexical = &envs[i];
                    meter.work(binding.len().max(lexical.id.len()))?;
                    if lexical.id == binding {
                        meter.work(
                            n.namespace
                                .len()
                                .max(lexical.name.namespace.len())
                                .checked_add(n.local.len().max(lexical.name.local.len()))
                                .ok_or_else(crate::error::size_overflow)?,
                        )?;
                        if n.namespace != lexical.name.namespace || n.local != lexical.name.local {
                            return Err(capability("binding expanded-name mismatch"));
                        }
                        nodes[visit.node].variable = Some(lexical.slot);
                        break;
                    }
                    e = lexical.parent;
                }
                if nodes[visit.node].variable.is_none() {
                    return Err(capability("unresolved/out-of-scope binding identity"));
                }
            }
            let depth = visit
                .depth
                .checked_add(1)
                .ok_or_else(crate::error::size_overflow)?;
            let mut child = |expr, env| -> Result<()> {
                meter.depth(depth)?;
                let id = add(&mut nodes, expr, owner, &mut meter)?;
                meter.push(&mut nodes[visit.node].children, id)?;
                schedule(
                    &mut meter,
                    &mut todo,
                    Visit {
                        node: id,
                        env,
                        depth,
                    },
                )?;
                Ok(())
            };
            match expr {
                Expr::Group { operand }
                | Expr::Unary { operand, .. }
                | Expr::TypeOp { operand, .. } => child(operand, visit.env)?,
                Expr::Binary { left, right, .. } => {
                    child(left, visit.env)?;
                    child(right, visit.env)?;
                }
                Expr::Filter { base, predicate } => {
                    child(base, visit.env)?;
                    child(predicate, visit.env)?;
                }
                Expr::If {
                    condition,
                    then,
                    otherwise,
                } => {
                    child(condition, visit.env)?;
                    child(then, visit.env)?;
                    child(otherwise, visit.env)?;
                }
                Expr::Call { arguments, .. } | Expr::Sequence { items: arguments } => {
                    for a in arguments {
                        child(a, visit.env)?;
                    }
                }
                Expr::Bind {
                    bindings, result, ..
                } => {
                    let mut bound = visit.env;
                    for b in bindings {
                        meter.depth(depth)?;
                        for previous in &envs {
                            meter.work(previous.id.len().max(b.id.len()))?;
                            if previous.id == b.id {
                                return Err(capability(
                                    "duplicate resolved tuple binding identity",
                                ));
                            }
                        }
                        let id = add(&mut nodes, &b.value, owner, &mut meter)?;
                        meter.push(&mut nodes[visit.node].children, id)?;
                        schedule(
                            &mut meter,
                            &mut todo,
                            Visit {
                                node: id,
                                env: bound,
                                depth,
                            },
                        )?;
                        let slot = slots;
                        slots = slots
                            .checked_add(1)
                            .ok_or_else(crate::error::size_overflow)?;
                        meter.push(&mut nodes[visit.node].slots, slot)?;
                        bound = extend(&mut envs, &b.id, &b.name, slot, bound, &mut meter)?;
                    }
                    meter.depth(depth)?;
                    let id = add(&mut nodes, result, owner, &mut meter)?;
                    meter.push(&mut nodes[visit.node].children, id)?;
                    schedule(
                        &mut meter,
                        &mut todo,
                        Visit {
                            node: id,
                            env: bound,
                            depth,
                        },
                    )?;
                }
                _ => {}
            }
        }
        // Descendants have higher indices, so this structural annotation is iterative.
        for i in (0..nodes.len()).rev() {
            meter.work(1)?;
            meter.work(nodes[i].children.len())?;
            nodes[i].focus_dependent = matches!(
                nodes[i].expr,
                Expr::Context | Expr::Root | Expr::Step { .. }
            ) || matches!(nodes[i].expr, Expr::Call { name, arguments } if name.namespace == FN && (["position", "last"].contains(&name.local.as_str()) || (arguments.is_empty() && ["string", "number"].contains(&name.local.as_str()))))
                || nodes[i]
                    .children
                    .iter()
                    .any(|child| nodes[*child].focus_dependent);
            nodes[i].reverse_chain = match nodes[i].expr {
                Expr::Step { axis, .. } => {
                    ["ancestor", "ancestor-or-self", "preceding-sibling"].contains(&axis.as_str())
                }
                Expr::Filter { .. } => nodes[nodes[i].children[0]].reverse_chain,
                _ => false,
            };
        }
        // Temporary lexical/visit storage was charged cumulatively; release live scratch.
        meter.release(
            envs.len()
                .checked_mul(size_of::<Lexical<'_>>())
                .ok_or_else(crate::error::size_overflow)?,
        );
        Ok(Self {
            nodes,
            declarations,
            root,
            slots,
            usage: meter.usage,
        })
    }
    /// Original source expression, without AST clone or reparsing.
    pub fn expression(&self) -> &'a Expression {
        self.nodes[self.root].owner
    }
    /// Preparation accounting, including transient admission.
    pub fn usage(&self) -> MachineUsage {
        self.usage
    }
}
