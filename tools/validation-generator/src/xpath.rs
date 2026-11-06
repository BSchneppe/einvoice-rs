//! Owned bounded XPath 2 grammar. Numbers never enter a numeric machine representation.
use crate::{Error, Result};
use std::collections::{BTreeMap, BTreeSet};
use validation_ir::*;
pub const FN: &str = "http://www.w3.org/2005/xpath-functions";
pub const XS: &str = "http://www.w3.org/2001/XMLSchema";
#[derive(Debug, Clone, PartialEq)]
enum Token {
    Name(String),
    Number(String, NumericKind),
    String(String),
    Symbol(String),
    End,
}
#[derive(Debug, Clone)]
struct Lexeme {
    token: Token,
    offset: usize,
}
fn lex(text: &str) -> Result<Vec<Lexeme>> {
    let b = text.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < b.len() {
        if b[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        if text[i..].starts_with("(:") {
            i += 2;
            let mut depth = 1;
            while i < b.len() && depth > 0 {
                if text[i..].starts_with("(:") {
                    depth += 1;
                    i += 2;
                } else if text[i..].starts_with(":)") {
                    depth -= 1;
                    i += 2;
                } else {
                    i += text[i..].chars().next().unwrap().len_utf8();
                }
            }
            if depth != 0 {
                return Err(Error::xpath(start, "unterminated comment"));
            }
            continue;
        }
        let token = if b[i] == b'\'' || b[i] == b'"' {
            let quote = b[i];
            i += 1;
            let mut value = String::new();
            let mut closed = false;
            while i < b.len() {
                if b[i] == quote {
                    i += 1;
                    if i < b.len() && b[i] == quote {
                        value.push(quote as char);
                        i += 1;
                    } else {
                        closed = true;
                        break;
                    }
                } else {
                    let c = text[i..].chars().next().unwrap();
                    value.push(c);
                    i += c.len_utf8();
                }
            }
            if !closed {
                return Err(Error::xpath(start, "unterminated string"));
            }
            Token::String(value)
        } else if b[i].is_ascii_digit()
            || (b[i] == b'.' && b.get(i + 1).is_some_and(u8::is_ascii_digit))
        {
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            let mut kind = NumericKind::Integer;
            if b.get(i) == Some(&b'.') {
                kind = NumericKind::Decimal;
                i += 1;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
            }
            if matches!(b.get(i), Some(b'e' | b'E')) {
                kind = NumericKind::Double;
                i += 1;
                if matches!(b.get(i), Some(b'+' | b'-')) {
                    i += 1;
                }
                let digits = i;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
                if i == digits {
                    return Err(Error::xpath(start, "exponent requires digits"));
                }
            }
            if i < b.len() && name_start(text[i..].chars().next().unwrap()) {
                return Err(Error::xpath(
                    i,
                    "numeric literal must be separated from a name",
                ));
            }
            Token::Number(text[start..i].into(), kind)
        } else if name_start(text[i..].chars().next().unwrap()) {
            i += text[i..].chars().next().unwrap().len_utf8();
            while i < b.len() {
                let c = text[i..].chars().next().unwrap();
                if name_char(c) {
                    i += c.len_utf8();
                } else {
                    break;
                }
            }
            // Colon is deliberately separate: axes, QName and namespace wildcards differ.
            Token::Name(text[start..i].into())
        } else {
            let mut symbol = None;
            for s in ["//", "..", "::", "!=", "<=", ">=", "<<", ">>"] {
                if text[i..].starts_with(s) {
                    symbol = Some(s);
                    break;
                }
            }
            if let Some(s) = symbol {
                i += s.len();
                Token::Symbol(s.into())
            } else if b"/$@()[],|+-*=<>.?:".contains(&b[i]) {
                i += 1;
                Token::Symbol(text[start..i].into())
            } else {
                return Err(Error::xpath(start, "unknown token"));
            }
        };
        out.push(Lexeme {
            token,
            offset: start,
        });
        if out.len() > 4096 {
            return Err(Error::xpath(start, "token budget exceeded"));
        }
    }
    out.push(Lexeme {
        token: Token::End,
        offset: text.len(),
    });
    Ok(out)
}
fn name_start(c: char) -> bool {
    c == '_' || c.is_alphabetic()
}
fn name_char(c: char) -> bool {
    name_start(c) || c.is_ascii_digit() || c == '-' || c == '.'
}
#[derive(Clone, Default)]
pub struct Context {
    pub namespaces: BTreeMap<String, String>,
    pub helpers: BTreeSet<(String, usize)>,
}
impl Context {
    pub fn xslt2() -> Self {
        let mut c = Self::default();
        c.namespaces.insert("xs".into(), XS.into());
        c.namespaces.insert("fn".into(), FN.into());
        c.namespaces
            .insert("xml".into(), "http://www.w3.org/XML/1998/namespace".into());
        c
    }
    pub fn name(&self, raw: &str, function: bool) -> Result<Name> {
        let valid = raw.split(':').count() <= 2
            && raw.split(':').all(|part| {
                part.chars().next().is_some_and(name_start) && part.chars().all(name_char)
            });
        if !valid {
            return Err(Error::new(format!("invalid QName {raw:?}")));
        }
        if let Some((prefix, local)) = raw.split_once(':') {
            let namespace = self
                .namespaces
                .get(prefix)
                .ok_or_else(|| Error::new(format!("unknown namespace prefix {prefix}")))?;
            Ok(Name {
                qualified: raw.into(),
                namespace: namespace.clone(),
                local: local.into(),
            })
        } else {
            Ok(Name {
                qualified: raw.into(),
                namespace: if function { FN.into() } else { String::new() },
                local: raw.into(),
            })
        }
    }
}
struct Parser<'a> {
    tokens: Vec<Lexeme>,
    index: usize,
    depth: usize,
    context: &'a Context,
}
impl Parser<'_> {
    fn error(&self, s: &str) -> Error {
        Error::xpath(self.tokens[self.index].offset, s)
    }
    fn at(&self, s: &str) -> bool {
        matches!(&self.tokens[self.index].token,Token::Symbol(v)|Token::Name(v) if v == s)
    }
    fn eat(&mut self, s: &str) -> bool {
        if self.at(s) {
            self.index += 1;
            true
        } else {
            false
        }
    }
    fn need(&mut self, s: &str) -> Result<()> {
        if self.eat(s) {
            Ok(())
        } else {
            Err(self.error(&format!("expected {s}")))
        }
    }
    fn ident(&mut self) -> Result<String> {
        if let Token::Name(s) = self.tokens[self.index].token.clone() {
            self.index += 1;
            Ok(s)
        } else {
            Err(self.error("expected NCName"))
        }
    }
    fn qname(&mut self) -> Result<String> {
        let mut s = self.ident()?;
        if self.eat(":") {
            self.adjacent_colon()?;
            s.push(':');
            s.push_str(&self.ident()?);
        }
        Ok(s)
    }
    fn adjacent_colon(&self) -> Result<()> {
        let left = &self.tokens[self.index - 2];
        let colon = &self.tokens[self.index - 1];
        let right = &self.tokens[self.index];
        let width = match &left.token {
            Token::Name(s) => s.len(),
            Token::Symbol(s) if s == "*" => 1,
            _ => return Err(self.error("invalid QName prefix")),
        };
        if left.offset + width != colon.offset || colon.offset + 1 != right.offset {
            return Err(self.error("whitespace inside QName"));
        }
        Ok(())
    }
    fn expr(&mut self) -> Result<Expr> {
        let first = self.single()?;
        if !self.eat(",") {
            return Ok(first);
        }
        let mut items = vec![first];
        loop {
            items.push(self.single()?);
            if !self.eat(",") {
                break;
            }
        }
        Ok(Expr::Sequence { items })
    }
    fn single(&mut self) -> Result<Expr> {
        self.depth += 1;
        if self.depth > 64 {
            return Err(self.error("expression nesting budget exceeded"));
        }
        let result = self.single_inner();
        self.depth -= 1;
        result
    }
    fn single_inner(&mut self) -> Result<Expr> {
        if self.at("if") && matches!(&self.tokens[self.index+1].token,Token::Symbol(s) if s == "(")
        {
            self.index += 1;
            self.need("(")?;
            let condition = self.expr()?;
            self.need(")")?;
            self.need("then")?;
            let then = self.single()?;
            self.need("else")?;
            let otherwise = self.single()?;
            return Ok(Expr::If {
                condition: Box::new(condition),
                then: Box::new(then),
                otherwise: Box::new(otherwise),
            });
        }
        if ["for", "some", "every"].iter().any(|s| self.at(s))
            && matches!(&self.tokens[self.index+1].token,Token::Symbol(s) if s == "$")
        {
            let operator = self.ident()?;
            let mut bindings = Vec::new();
            loop {
                self.need("$")?;
                let offset = self.tokens[self.index].offset;
                let raw = self.qname()?;
                let name = self.context.name(&raw, false)?;
                self.need("in")?;
                let value = self.single()?;
                bindings.push(ExprBinding {
                    id: format!("expr:{offset}:{raw}"),
                    name,
                    value,
                });
                if !self.eat(",") {
                    break;
                }
            }
            self.need(if operator == "for" {
                "return"
            } else {
                "satisfies"
            })?;
            return Ok(Expr::Bind {
                operator,
                bindings,
                result: Box::new(self.single()?),
            });
        }
        self.binary(0)
    }
    fn operator(&self) -> Option<(&'static str, u8)> {
        [
            ("or", 1),
            ("and", 2),
            ("=", 3),
            ("!=", 3),
            ("<", 3),
            ("<=", 3),
            (">", 3),
            (">=", 3),
            ("eq", 3),
            ("ne", 3),
            ("lt", 3),
            ("le", 3),
            ("gt", 3),
            ("ge", 3),
            ("is", 3),
            ("<<", 3),
            (">>", 3),
            ("to", 4),
            ("+", 5),
            ("-", 5),
            ("*", 6),
            ("div", 6),
            ("idiv", 6),
            ("mod", 6),
            ("union", 7),
            ("|", 7),
            ("intersect", 8),
            ("except", 8),
            ("castable", 9),
            ("cast", 10),
            ("/", 12),
            ("//", 12),
        ]
        .into_iter()
        .find(|(s, _)| self.at(s))
    }
    fn binary(&mut self, min: u8) -> Result<Expr> {
        self.depth += 1;
        if self.depth > 64 {
            return Err(self.error("expression nesting budget exceeded"));
        }
        let result = self.binary_inner(min);
        self.depth -= 1;
        result
    }
    fn binary_inner(&mut self, min: u8) -> Result<Expr> {
        // The RHS of / and // is a StepExpr, not another absolute path or unary expression.
        // Parentheses enter expr() afresh and deliberately permit these forms inside a group.
        if min > 12 && ["+", "-", "/", "//"].iter().any(|s| self.at(s)) {
            return Err(
                self.error("path step requires parentheses around unary or absolute expression")
            );
        }
        let mut left = if self.at("+") || self.at("-") {
            let operator = if self.eat("+") {
                "+"
            } else {
                self.need("-")?;
                "-"
            };
            Expr::Unary {
                operator: operator.into(),
                operand: Box::new(self.binary(11)?),
            }
        } else if self.eat("/") {
            if self.step_starts() {
                Expr::Binary {
                    operator: "/".into(),
                    left: Box::new(Expr::Root),
                    right: Box::new(self.binary(13)?),
                }
            } else {
                Expr::Root
            }
        } else if self.eat("//") {
            Expr::Binary {
                operator: "//".into(),
                left: Box::new(Expr::Root),
                right: Box::new(self.binary(13)?),
            }
        } else {
            let mut primary = self.primary()?;
            while self.eat("[") {
                let predicate = self.expr()?;
                self.need("]")?;
                primary = Expr::Filter {
                    base: Box::new(primary),
                    predicate: Box::new(predicate),
                };
            }
            primary
        };
        let mut nonassoc = BTreeSet::new();
        let mut type_boundary = None;
        let mut chain = 0;
        loop {
            let Some((operator, prec)) = self.operator() else {
                break;
            };
            if prec < min {
                break;
            }
            if type_boundary.is_some_and(|boundary| prec > boundary) {
                return Err(
                    self.error("higher-precedence operand after type operator needs parentheses")
                );
            }
            if [3, 4, 9, 10].contains(&prec) && !nonassoc.insert(prec) {
                return Err(self.error("operator is not associative"));
            }
            chain += 1;
            if chain > 128 {
                return Err(self.error("operator chain budget exceeded"));
            }
            self.index += 1;
            if operator == "cast" || operator == "castable" {
                type_boundary = Some(prec);
                self.need("as")?;
                let sequence_type = self.sequence_type(true)?;
                left = Expr::TypeOp {
                    operator: operator.into(),
                    operand: Box::new(left),
                    sequence_type,
                };
            } else {
                let right = self.binary(prec + 1)?;
                left = Expr::Binary {
                    operator: operator.into(),
                    left: Box::new(left),
                    right: Box::new(right),
                };
            }
        }
        Ok(left)
    }
    fn step_starts(&self) -> bool {
        matches!(
            &self.tokens[self.index].token,
            Token::Name(_) | Token::Number(..) | Token::String(_)
        ) || [".", "..", "@", "*", "(", "$"].iter().any(|s| self.at(s))
    }
    fn primary(&mut self) -> Result<Expr> {
        match self.tokens[self.index].token.clone() {
            Token::Number(lexeme, lexical_kind) => {
                self.index += 1;
                return Ok(Expr::Number {
                    lexical_kind,
                    lexeme,
                });
            }
            Token::String(value) => {
                self.index += 1;
                return Ok(Expr::String { value });
            }
            _ => (),
        }
        if self.eat("(") {
            if self.eat(")") {
                return Ok(Expr::Sequence { items: Vec::new() });
            }
            let value = self.expr()?;
            self.need(")")?;
            return Ok(Expr::Group {
                operand: Box::new(value),
            });
        }
        if self.eat("$") {
            let raw = self.qname()?;
            return Ok(Expr::Variable {
                name: self.context.name(&raw, false)?,
                binding: String::new(),
            });
        }
        if self.eat(".") {
            return Ok(Expr::Context);
        }
        if self.eat("..") {
            return Ok(Expr::Step {
                axis: "parent".into(),
                test: NodeTest::Kind {
                    name: "node".into(),
                },
            });
        }
        let explicit_axis = self.at("@")
            || matches!(&self.tokens.get(self.index+1).map(|t| &t.token), Some(Token::Symbol(s)) if s == "::");
        let axis = if self.eat("@") {
            "attribute".to_owned()
        } else if matches!(&self.tokens.get(self.index+1).map(|t| &t.token), Some(Token::Symbol(s)) if s == "::")
        {
            let axis = self.ident()?;
            self.need("::")?;
            if ![
                "child",
                "attribute",
                "self",
                "parent",
                "descendant",
                "descendant-or-self",
                "ancestor",
                "ancestor-or-self",
                "following-sibling",
                "preceding-sibling",
                "following",
                "preceding",
            ]
            .contains(&axis.as_str())
            {
                return Err(self.error("unsupported axis"));
            }
            axis
        } else {
            "child".into()
        };
        if self.eat("*") {
            let test = if self.eat(":") {
                self.adjacent_colon()?;
                NodeTest::Local {
                    local: self.ident()?,
                }
            } else {
                NodeTest::Any
            };
            return Ok(Expr::Step { axis, test });
        }
        let mut raw = self.ident()?;
        if self.eat(":") {
            self.adjacent_colon()?;
            if self.eat("*") {
                let namespace = self
                    .context
                    .namespaces
                    .get(&raw)
                    .ok_or_else(|| self.error("unknown wildcard prefix"))?
                    .clone();
                return Ok(Expr::Step {
                    axis,
                    test: NodeTest::Namespace {
                        prefix: raw,
                        namespace,
                    },
                });
            }
            raw.push(':');
            raw.push_str(&self.ident()?);
        }
        if self.eat("(") {
            if ["text", "node", "comment", "processing-instruction"].contains(&raw.as_str()) {
                self.need(")")?;
                return Ok(Expr::Step {
                    axis,
                    test: NodeTest::Kind { name: raw },
                });
            }
            if explicit_axis {
                return Err(self.error("function call cannot be an axis node test"));
            }
            let name = self.context.name(&raw, true)?;
            let mut arguments = Vec::new();
            if !self.eat(")") {
                loop {
                    arguments.push(self.single()?);
                    if self.eat(")") {
                        break;
                    }
                    self.need(",")?;
                }
            }
            check_call(&name, arguments.len(), self.context)?;
            return Ok(Expr::Call { name, arguments });
        }
        Ok(Expr::Step {
            axis,
            test: NodeTest::Name {
                name: self.context.name(&raw, false)?,
            },
        })
    }
    fn sequence_type(&mut self, single: bool) -> Result<SequenceType> {
        let raw = self.qname()?;
        let name = self.context.name(&raw, false)?;
        if name.namespace != XS
            || ![
                "string",
                "boolean",
                "decimal",
                "integer",
                "double",
                "float",
                "date",
                "dateTime",
                "time",
                "anyURI",
                "untypedAtomic",
                "nonNegativeInteger",
                "positiveInteger",
                "int",
                "long",
                "token",
                "normalizedString",
            ]
            .contains(&name.local.as_str())
        {
            return Err(self.error("unsupported sequence type"));
        }
        let occurrence = if self.eat("?") {
            Some("?".into())
        } else if !single && self.eat("*") {
            Some("*".into())
        } else if !single && self.eat("+") {
            Some("+".into())
        } else {
            None
        };
        Ok(SequenceType { name, occurrence })
    }
}
fn check_call(name: &Name, arity: usize, context: &Context) -> Result<()> {
    let valid = if name.namespace == FN {
        let (min, max) = match name.local.as_str() {
            "true" | "false" | "position" | "last" => (0, 0),
            "name" | "local-name" | "namespace-uri" | "string" | "number" | "normalize-space"
            | "string-length" => (0, 1),
            "not"
            | "exists"
            | "empty"
            | "boolean"
            | "upper-case"
            | "lower-case"
            | "abs"
            | "floor"
            | "ceiling"
            | "reverse"
            | "string-to-codepoints"
            | "codepoints-to-string"
            | "distinct-values"
            | "count"
            | "data" => (1, 1),
            "round" => (1, 1),
            "sum" => (1, 2),
            "round-half-to-even" => (1, 2),
            "contains" | "starts-with" | "ends-with" | "substring-before" | "substring-after"
            | "matches" | "tokenize" => (2, 3),
            "substring" => (2, 3),
            "translate" => (3, 3),
            "replace" => (3, 4),
            "string-join" => (2, 2),
            "concat" => (2, usize::MAX),
            _ => {
                return Err(Error::new(format!(
                    "unknown standard function {}",
                    name.qualified
                )))
            }
        };
        (min..=max).contains(&arity)
    } else if name.namespace == XS {
        [
            "string", "decimal", "integer", "boolean", "double", "date", "dateTime",
        ]
        .contains(&name.local.as_str())
            && arity == 1
    } else {
        context.helpers.contains(&(name.expanded(), arity))
    };
    if valid {
        Ok(())
    } else {
        Err(Error::new(format!(
            "unknown function or arity {}/{}",
            name.qualified, arity
        )))
    }
}
fn bound_tree(tree: &Expr) -> Result<()> {
    let mut pending = vec![(tree, 1usize)];
    while let Some((e, depth)) = pending.pop() {
        if depth > 64 {
            return Err(Error::new("AST depth budget exceeded (64)"));
        }
        let next = depth + 1;
        match e {
            Expr::Call { arguments, .. } | Expr::Sequence { items: arguments } => {
                pending.extend(arguments.iter().map(|a| (a, next)))
            }
            Expr::Unary { operand, .. }
            | Expr::TypeOp { operand, .. }
            | Expr::Group { operand } => pending.push((operand, next)),
            Expr::Binary { left, right, .. } => {
                pending.push((left, next));
                pending.push((right, next));
            }
            Expr::Filter { base, predicate } => {
                pending.push((base, next));
                pending.push((predicate, next));
            }
            Expr::If {
                condition,
                then,
                otherwise,
            } => {
                pending.push((condition, next));
                pending.push((then, next));
                pending.push((otherwise, next));
            }
            Expr::Bind {
                bindings, result, ..
            } => {
                pending.extend(bindings.iter().map(|b| (&b.value, next)));
                pending.push((result, next));
            }
            _ => (),
        }
    }
    Ok(())
}
pub fn parse(text: &str, context: &Context) -> Result<Expr> {
    let mut p = Parser {
        tokens: lex(text)?,
        index: 0,
        depth: 0,
        context,
    };
    let tree = p.expr()?;
    if p.tokens[p.index].token != Token::End {
        return Err(p.error("unexpected trailing input"));
    }
    bound_tree(&tree)?;
    Ok(tree)
}
pub fn parse_type(text: &str, context: &Context) -> Result<SequenceType> {
    let mut p = Parser {
        tokens: lex(text)?,
        index: 0,
        depth: 0,
        context,
    };
    let t = p.sequence_type(false)?;
    if p.tokens[p.index].token != Token::End {
        return Err(p.error("unexpected type suffix"));
    }
    Ok(t)
}
/// Pattern paths admit only matchable child/attribute steps. Predicates are full expressions.
pub fn parse_pattern(text: &str, context: &Context) -> Result<Expr> {
    fn step(e: &Expr) -> bool {
        match e {
            Expr::Step { axis, .. } => axis == "child" || axis == "attribute",
            Expr::Filter { base, .. } => step(base),
            Expr::Group { operand } => step(operand),
            _ => false,
        }
    }
    fn branch(e: &Expr) -> bool {
        match e {
            Expr::Root => true,
            Expr::Binary {
                operator,
                left,
                right,
            } if ["/", "//"].contains(&operator.as_str()) => branch(left) && step(right),
            _ => step(e),
        }
    }
    fn validate(e: &Expr) -> bool {
        match e {
            Expr::Binary {
                operator,
                left,
                right,
            } if ["|", "union"].contains(&operator.as_str()) => validate(left) && validate(right),
            _ => branch(e),
        }
    }
    let tree = parse(text, context)?;
    if !validate(&tree) {
        return Err(Error::new(
            "unsupported match pattern (not an expression selection)",
        ));
    }
    Ok(tree)
}
/// Resolve every variable use against its actual lexical binding identity.
pub fn resolve(tree: &mut Expr, scope: &BTreeMap<String, String>, prefix: &str) -> Result<()> {
    match tree {
        Expr::Variable { name, binding } => {
            *binding = scope
                .get(&name.expanded())
                .ok_or_else(|| Error::new(format!("unresolved variable ${}", name.qualified)))?
                .clone();
        }
        Expr::Bind {
            bindings, result, ..
        } => {
            let mut inner = scope.clone();
            for b in bindings {
                resolve(&mut b.value, &inner, prefix)?;
                b.id = format!("{prefix}/{}", b.id);
                inner.insert(b.name.expanded(), b.id.clone());
            }
            resolve(result, &inner, prefix)?;
        }
        Expr::Call { arguments, .. } | Expr::Sequence { items: arguments } => {
            for a in arguments {
                resolve(a, scope, prefix)?;
            }
        }
        Expr::Unary { operand, .. } | Expr::TypeOp { operand, .. } | Expr::Group { operand } => {
            resolve(operand, scope, prefix)?
        }
        Expr::Binary { left, right, .. } => {
            resolve(left, scope, prefix)?;
            resolve(right, scope, prefix)?;
        }
        Expr::Filter { base, predicate } => {
            resolve(base, scope, prefix)?;
            resolve(predicate, scope, prefix)?;
        }
        Expr::If {
            condition,
            then,
            otherwise,
        } => {
            resolve(condition, scope, prefix)?;
            resolve(then, scope, prefix)?;
            resolve(otherwise, scope, prefix)?;
        }
        _ => (),
    }
    Ok(())
}
pub fn dependencies(tree: &Expr, variables: &mut BTreeSet<String>, calls: &mut BTreeSet<String>) {
    match tree {
        Expr::Variable { binding, .. } => {
            variables.insert(binding.clone());
        }
        Expr::Call { name, arguments } => {
            calls.insert(format!("{}/{}", name.expanded(), arguments.len()));
            for a in arguments {
                dependencies(a, variables, calls);
            }
        }
        Expr::Sequence { items } => {
            for e in items {
                dependencies(e, variables, calls);
            }
        }
        Expr::Bind {
            bindings, result, ..
        } => {
            for b in bindings {
                dependencies(&b.value, variables, calls);
            }
            dependencies(result, variables, calls);
        }
        Expr::Unary { operand, .. } | Expr::TypeOp { operand, .. } | Expr::Group { operand } => {
            dependencies(operand, variables, calls)
        }
        Expr::Binary { left, right, .. } => {
            dependencies(left, variables, calls);
            dependencies(right, variables, calls);
        }
        Expr::Filter { base, predicate } => {
            dependencies(base, variables, calls);
            dependencies(predicate, variables, calls);
        }
        Expr::If {
            condition,
            then,
            otherwise,
        } => {
            dependencies(condition, variables, calls);
            dependencies(then, variables, calls);
            dependencies(otherwise, variables, calls);
        }
        _ => (),
    }
}
