//! Backend-independent inspection contract. No parser, archive ingestion, or execution API.
pub mod resolved;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Changes to tree semantics require a new format, independently of generator/target versions.
pub const FORMAT: &str = "einvoice-validation-ir/1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Name {
    pub qualified: String,
    pub namespace: String,
    pub local: String,
}
impl Name {
    pub fn expanded(&self) -> String {
        format!("{{{}}}{}", self.namespace, self.local)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Location {
    pub resource: String,
    /// Raw UTF-8 XML element/attribute range, never a decoded XPath range.
    pub xml_byte_start: usize,
    pub xml_byte_end: usize,
    pub line: u32,
    pub column: u32,
    pub attribute: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Expression {
    pub text: String,
    pub source: Location,
    pub tree: Expr,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Expr {
    Number {
        lexical_kind: NumericKind,
        lexeme: String,
    },
    String {
        value: String,
    },
    Variable {
        name: Name,
        binding: String,
    },
    Context,
    Root,
    /// Explicit parentheses preserve the boundary between axis predicates and sequence filters.
    Group {
        operand: Box<Expr>,
    },
    Step {
        axis: String,
        test: NodeTest,
    },
    Call {
        name: Name,
        arguments: Vec<Expr>,
    },
    Unary {
        operator: String,
        operand: Box<Expr>,
    },
    Binary {
        operator: String,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Filter {
        /// A Step (or a chain of Filters rooted in a Step) uses axis proximity order.
        /// A Group terminates that chain: its result is filtered in sequence order.
        base: Box<Expr>,
        predicate: Box<Expr>,
    },
    Sequence {
        items: Vec<Expr>,
    },
    If {
        condition: Box<Expr>,
        then: Box<Expr>,
        otherwise: Box<Expr>,
    },
    Bind {
        operator: String,
        bindings: Vec<ExprBinding>,
        result: Box<Expr>,
    },
    TypeOp {
        operator: String,
        operand: Box<Expr>,
        sequence_type: SequenceType,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NumericKind {
    Integer,
    Decimal,
    Double,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NodeTest {
    Name { name: Name },
    Any,
    Namespace { prefix: String, namespace: String },
    Local { local: String },
    Kind { name: String },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExprBinding {
    pub id: String,
    pub name: Name,
    pub value: Expr,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SequenceType {
    pub name: Name,
    pub occurrence: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Xslt2SchemaGlobal,
    RuleLocal,
    HelperParameter,
    HelperLocal,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    pub id: String,
    pub name: Name,
    pub declared_owner: String,
    pub effective_scope: Scope,
    pub initialization_context: String,
    pub source: Location,
    pub sequence_type: Option<SequenceType>,
    pub value: BindingValue,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BindingValue {
    Select { expression: Expression },
    Content { body: Vec<Instruction> },
    Parameter,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Instruction {
    Text {
        text: String,
    },
    Variable {
        binding: Binding,
    },
    Sequence {
        expression: Expression,
    },
    Choose {
        source: Location,
        branches: Vec<Branch>,
        otherwise: Vec<Instruction>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Branch {
    pub test: Expression,
    pub body: Vec<Instruction>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Helper {
    pub name: Name,
    pub source: Location,
    pub result_type: SequenceType,
    pub parameters: Vec<Binding>,
    pub body: Vec<Instruction>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MessagePart {
    Text { text: String },
    Name { source: Location },
    ValueOf { expression: Expression },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Site {
    pub identity: String,
    pub source: Location,
    pub id: String,
    pub raw_flag: String,
    pub base_severity: Severity,
    pub effective_severity: Severity,
    pub test: Expression,
    pub message: Vec<MessagePart>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
    Information,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub identity: String,
    pub source: Location,
    pub flag: Option<String>,
    pub context: Expression,
    pub bindings: Vec<Binding>,
    pub sites: Vec<Site>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pattern {
    pub id: String,
    pub source: Location,
    pub bindings: Vec<Binding>,
    pub rules: Vec<Rule>,
    pub active: bool,
    pub activation_reason: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Phase {
    pub id: String,
    pub active_patterns: Vec<String>,
    pub source: Location,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Include {
    pub from: Location,
    pub to: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamespaceDeclaration {
    pub prefix: String,
    pub uri: String,
    pub source: Location,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Schema {
    pub source: Location,
    pub title: String,
    pub namespaces: BTreeMap<String, String>,
    pub namespace_declarations: Vec<NamespaceDeclaration>,
    pub xpath_element_default_namespace: String,
    pub xpath_function_default_namespace: String,
    pub declared_default_phase: Option<String>,
    pub selection: String,
    pub dispatch: Dispatch,
    pub phases: Vec<Phase>,
    pub includes: Vec<Include>,
    pub globals: Vec<Binding>,
    pub helpers: Vec<Helper>,
    pub patterns: Vec<Pattern>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Dispatch {
    DocumentTraversalFirstMatchingRulePerPatternInSourceOrder,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resource {
    pub archive: String,
    pub member: String,
    pub sha256: String,
    pub size: u64,
    pub role: ResourceRole,
    pub pin: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceRole {
    Schematron,
    CodelistTrace,
    CiiSchema,
    PolicyMappedSource,
    OracleExecutableEvidence,
    OpaqueReport,
    ReportSchema,
    EngineEvidence,
    Notice,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub relation: String,
    pub namespace: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    pub resource: String,
    pub sha256: String,
    pub locator: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PolicyClause {
    OrderedStages {
        stages: Vec<String>,
        schema_failure_skips_rules: bool,
        evidence: Vec<Evidence>,
    },
    FlagConversion {
        inputs: Vec<String>,
        precedence: Vec<Severity>,
        error: Vec<String>,
        warning: Vec<String>,
        information: Vec<String>,
        unknown: Severity,
        evidence: Vec<Evidence>,
    },
    Overrides {
        values: BTreeMap<String, Severity>,
        processing_error_immune: bool,
        evidence: Vec<Evidence>,
    },
    MissingSvrl {
        creates_processing_error_unless_already_present: bool,
        evidence: Vec<Evidence>,
    },
    Assessment {
        rejects_processing_errors: bool,
        rejects_unmatched: bool,
        rejects_normalized_fallback_name: bool,
        rejecting_severity: Severity,
        stage_invalid_severities: Vec<Severity>,
        stage_validity_uses: String,
        evidence: Vec<Evidence>,
    },
    Acceptance {
        absent_selector_fallback: String,
        selector_ebv: bool,
        requires_schema_scenario_report: bool,
        xpath_failure_is_processing_failure: bool,
        evidence: Vec<Evidence>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    pub scenario: String,
    pub namespaces: BTreeMap<String, String>,
    pub selection: Expression,
    pub acceptance: Expression,
    pub clauses: Vec<PolicyClause>,
    pub execution: BackendState,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackendState {
    Unavailable,
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Requirement {
    pub category: String,
    pub name: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Package {
    pub format: String,
    pub target: String,
    pub source_package: String,
    pub backend: BackendState,
    pub schemas: Vec<Schema>,
    pub resources: BTreeMap<String, Resource>,
    pub source_graph: Vec<Edge>,
    pub policy: Option<Policy>,
    pub requirements: BTreeSet<Requirement>,
}
/// Exact identity selection, without a "latest" or library-version fallback.
pub fn select<'a>(
    packages: &'a [Package],
    target: &str,
    source_package: &str,
) -> Result<&'a Package, &'static str> {
    let mut matches = packages
        .iter()
        .filter(|p| p.target == target && p.source_package == source_package);
    let selected = matches.next().ok_or("unknown exact package identity")?;
    if matches.next().is_some() {
        return Err("duplicate exact package identity");
    }
    Ok(selected)
}
