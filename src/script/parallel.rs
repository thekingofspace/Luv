use std::collections::HashSet;
use std::fmt;
use std::thread;

use full_moon::LuaVersion;
use full_moon::ast::luau::{ConstAssignment, ConstFunction, TypeFunction, TypeInfo};
use full_moon::ast::{
    AnonymousFunction, Block, Call, Expression, FunctionArgs, FunctionBody, FunctionCall, FunctionDeclaration,
    GenericFor, Index, LocalAssignment, LocalFunction, NumericFor, Parameter, Prefix, Repeat, Stmt, Suffix, Var,
};
use full_moon::node::Node;
use full_moon::tokenizer::TokenReference;
use full_moon::visitors::Visitor;

pub const ENTER: &str = "EnterParallel";
pub const EXIT: &str = "ExitParallel";
pub const HOOK: &str = "__luv_parallel";
pub const FUNCTION_HOOK: &str = "__luv_parallel_function";
pub const DESYNCHRONIZE: &str = "desynchronize";
pub const SYNCHRONIZE: &str = "synchronize";
pub const BIND: &str = "BindParallel";
pub const SPAWN: &str = "parallel";
const TASK: &str = "task";

const PARSER_STACK_SIZE: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParallelError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ParallelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParallelError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClusterKind {
    Block,
    Function,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cluster {
    pub kind: ClusterKind,
    pub line: usize,
    pub captures: Vec<String>,
    pub source: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Units {
    pub main: String,
    pub clusters: Vec<Cluster>,
}

pub fn has_markers(source: &str) -> bool {
    [ENTER, EXIT, SYNCHRONIZE, BIND, SPAWN]
        .iter()
        .any(|marker| source.contains(marker))
}

pub fn split(source: &str) -> Result<Units, ParallelError> {
    if !has_markers(source) {
        return Ok(Units {
            main: source.to_owned(),
            clusters: Vec::new(),
        });
    }

    let failure = |message: String| ParallelError { line: 1, message };
    thread::scope(|scope| {
        thread::Builder::new()
            .name("luv parser".to_owned())
            .stack_size(PARSER_STACK_SIZE)
            .spawn_scoped(scope, || split_on_this_thread(source))
            .map_err(|error| failure(format!("could not start the parser: {error}")))?
            .join()
            .map_err(|_| failure("the parser crashed".to_owned()))?
    })
}

fn split_on_this_thread(source: &str) -> Result<Units, ParallelError> {
    let ast = full_moon::parse_fallible(source, LuaVersion::luau())
        .into_result()
        .map_err(|errors| {
            let error = &errors[0];
            ParallelError {
                line: error.range().0.line(),
                message: error.error_message().into_owned(),
            }
        })?;

    let mut analyzer = Analyzer::default();
    analyzer.visit_ast(&ast);
    if let Some(error) = analyzer.error {
        return Err(error);
    }

    let mut regions = analyzer.regions;
    regions.sort_by_key(|region| region.start);

    let clusters = regions
        .iter()
        .enumerate()
        .map(|(index, region)| {
            let mut unit = String::new();
            if !region.captures.is_empty() {
                unit.push_str(&format!("local {} = ...;", region.captures.join(", ")));
            }
            unit.push_str(&"\n".repeat(region.line - 1));
            if region.kind == ClusterKind::Function {
                unit.push_str("return ");
            }
            unit.push_str(&render(source, &regions, region.body_start, region.body_end, Some(index)));
            Cluster {
                kind: region.kind,
                line: region.line,
                captures: region.captures.clone(),
                source: unit,
            }
        })
        .collect();

    Ok(Units {
        main: render(source, &regions, 0, source.len(), None),
        clusters,
    })
}

fn replacement(source: &str, regions: &[Region], index: usize) -> String {
    let region = &regions[index];
    let hook = match region.kind {
        ClusterKind::Block => HOOK,
        ClusterKind::Function => FUNCTION_HOOK,
    };
    let mut text = format!("{hook}({}, \"{}\"", index + 1, region.captures.join(","));
    for capture in &region.captures {
        text.push_str(", ");
        text.push_str(capture);
    }
    text.push(')');
    text.push_str(&"\n".repeat(newlines(&source[region.start..region.end])));
    text
}

fn render(source: &str, regions: &[Region], from: usize, to: usize, skip: Option<usize>) -> String {
    let mut text = String::with_capacity(to - from);
    let mut cursor = from;
    for (index, region) in regions.iter().enumerate() {
        if Some(index) == skip || region.start < cursor || region.end > to {
            continue;
        }
        text.push_str(&source[cursor..region.start]);
        text.push_str(&replacement(source, regions, index));
        cursor = region.end;
    }
    text.push_str(&source[cursor..to]);
    text
}

fn newlines(text: &str) -> usize {
    text.bytes().filter(|byte| *byte == b'\n').count()
}

fn name(token: &TokenReference) -> String {
    token.token().to_string()
}

fn start(node: &impl Node) -> usize {
    node.start_position().map_or(0, |position| position.bytes())
}

fn end(node: &impl Node) -> usize {
    node.end_position().map_or(0, |position| position.bytes())
}

fn line(node: &impl Node) -> usize {
    node.start_position().map_or(1, |position| position.line())
}

struct Local {
    name: String,
    position: usize,
}

struct Region {
    kind: ClusterKind,
    start: usize,
    end: usize,
    body_start: usize,
    body_end: usize,
    line: usize,
    depth: usize,
    opener: &'static str,
    captures: Vec<String>,
}

impl Region {
    fn contains(&self, position: usize) -> bool {
        (self.start..self.end).contains(&position)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Open,
    Close,
}

#[derive(Clone, Copy)]
struct Marker {
    side: Side,
    text: &'static str,
    open: &'static str,
    close: &'static str,
}

const LEGACY_OPEN: Marker = Marker {
    side: Side::Open,
    text: "EnterParallel()",
    open: "EnterParallel()",
    close: "ExitParallel()",
};

const LEGACY_CLOSE: Marker = Marker {
    side: Side::Close,
    text: "ExitParallel()",
    ..LEGACY_OPEN
};

const TASK_OPEN: Marker = Marker {
    side: Side::Open,
    text: "task.desynchronize()",
    open: "task.desynchronize()",
    close: "task.synchronize()",
};

const TASK_CLOSE: Marker = Marker {
    side: Side::Close,
    text: "task.synchronize()",
    ..TASK_OPEN
};

#[derive(Default)]
struct Analyzer {
    scopes: Vec<Vec<Local>>,
    pending: Vec<(*const Block, Vec<Local>)>,
    deferred: Vec<*const Block>,
    methods: HashSet<*const FunctionBody>,
    regions: Vec<Region>,
    markers: HashSet<usize>,
    function_depth: usize,
    type_depth: usize,
    error: Option<ParallelError>,
}

impl Analyzer {
    fn fail(&mut self, line: usize, message: impl Into<String>) {
        if self.error.as_ref().is_none_or(|error| line < error.line) {
            self.error = Some(ParallelError {
                line,
                message: message.into(),
            });
        }
    }

    fn declare(&mut self, token: &TokenReference) {
        let local = Local {
            name: name(token),
            position: start(token),
        };
        if let Some(scope) = self.scopes.last_mut() {
            scope.push(local);
        }
    }

    fn locals<'a>(tokens: impl IntoIterator<Item = &'a TokenReference>) -> Vec<Local> {
        tokens
            .into_iter()
            .map(|token| Local {
                name: name(token),
                position: start(token),
            })
            .collect()
    }

    fn resolve(&self, name: &str) -> Option<usize> {
        self.scopes
            .iter()
            .rev()
            .flat_map(|scope| scope.iter().rev())
            .find(|local| local.name == name)
            .map(|local| local.position)
    }

    fn reference(&mut self, token: &TokenReference) {
        if self.type_depth > 0 {
            return;
        }
        let name = name(token);
        let position = start(token);
        let declared = self.resolve(&name);

        if declared.is_none() && (name == ENTER || name == EXIT) && !self.markers.contains(&position) {
            self.fail(line(token), format!("{name}() must be called on its own as a statement"));
            return;
        }

        let Some(declared) = declared else { return };
        for region in self.regions.iter_mut() {
            if region.contains(position) && declared < region.start && !region.captures.contains(&name) {
                region.captures.push(name.clone());
            }
        }
    }

    fn task_member<'a>(
        &self,
        call: &'a FunctionCall,
        shadowed: bool,
    ) -> Option<(&'a TokenReference, String, &'a FunctionArgs)> {
        let Prefix::Name(token) = call.prefix() else { return None };
        if name(token) != TASK || shadowed {
            return None;
        }
        let suffixes: Vec<&Suffix> = call.suffixes().collect();
        let [Suffix::Index(Index::Dot { name: member, .. }), Suffix::Call(Call::AnonymousCall(args))] = suffixes.as_slice()
        else {
            return None;
        };
        Some((token, name(member), args))
    }

    fn marker<'a>(&mut self, stmt: &'a Stmt, shadowed: bool) -> Option<(Marker, &'a TokenReference)> {
        let Stmt::FunctionCall(call) = stmt else { return None };
        let (marker, token, args) = match self.task_member(call, shadowed) {
            Some((token, member, args)) if member == DESYNCHRONIZE => (TASK_OPEN, token, args),
            Some((token, member, args)) if member == SYNCHRONIZE => (TASK_CLOSE, token, args),
            Some(_) => return None,
            None => {
                let Prefix::Name(token) = call.prefix() else { return None };
                let marker = match name(token).as_str() {
                    ENTER => LEGACY_OPEN,
                    EXIT => LEGACY_CLOSE,
                    _ => return None,
                };
                let suffixes: Vec<&Suffix> = call.suffixes().collect();
                let [Suffix::Call(Call::AnonymousCall(args))] = suffixes[..] else {
                    return None;
                };
                (marker, token, args)
            }
        };
        if !matches!(args, FunctionArgs::Parentheses { arguments, .. } if arguments.is_empty()) {
            self.fail(line(token), format!("{} does not take any arguments", marker.text));
            return None;
        }
        Some((marker, token))
    }

    fn find_regions(&mut self, block: &Block) {
        let mut open: Option<(usize, usize, usize, Marker)> = None;
        let mut shadowed = self.resolve(TASK).is_some();
        for (stmt, semicolon) in block.stmts_with_semicolon() {
            let declares = match stmt {
                Stmt::LocalAssignment(local) => local.names().iter().any(|token| name(token) == TASK),
                Stmt::LocalFunction(local) => name(local.name()) == TASK,
                _ => false,
            };
            let found = self.marker(stmt, shadowed);
            shadowed |= declares;
            let Some((marker, token)) = found else { continue };
            let stmt_start = start(stmt);
            let stmt_end = semicolon.as_ref().map_or_else(|| end(stmt), end);
            let stmt_line = line(stmt);

            match (marker.side, open) {
                (Side::Open, None) => {
                    let inside = self
                        .regions
                        .iter()
                        .any(|region| region.kind == ClusterKind::Block && region.contains(stmt_start));
                    if inside {
                        self.fail(stmt_line, format!("{} cannot be used inside another parallel block", marker.text));
                        return;
                    }
                    self.markers.insert(start(token));
                    open = Some((stmt_start, stmt_end, stmt_line, marker));
                }
                (Side::Open, Some((_, _, _, opener))) => {
                    self.fail(
                        stmt_line,
                        format!("{} cannot be nested, call {} first", marker.text, opener.close),
                    );
                    return;
                }
                (Side::Close, None) => {
                    self.fail(
                        stmt_line,
                        format!("{} has no matching {} in the same block", marker.text, marker.open),
                    );
                    return;
                }
                (Side::Close, Some((region_start, body_start, region_line, opener))) => {
                    self.markers.insert(start(token));
                    self.regions.push(Region {
                        kind: ClusterKind::Block,
                        start: region_start,
                        end: stmt_end,
                        body_start,
                        body_end: stmt_start,
                        line: region_line,
                        depth: self.function_depth,
                        opener: opener.open,
                        captures: Vec::new(),
                    });
                    open = None;
                }
            }
        }
        if let Some((_, _, region_line, opener)) = open {
            self.fail(
                region_line,
                format!("{} has no matching {} in the same block", opener.open, opener.close),
            );
        }
    }

    fn function_region(&mut self, function: &AnonymousFunction) {
        let (from, to) = (start(function), end(function));
        self.regions.push(Region {
            kind: ClusterKind::Function,
            start: from,
            end: to,
            body_start: from,
            body_end: to,
            line: line(function),
            depth: self.function_depth,
            opener: "",
            captures: Vec::new(),
        });
    }
}

fn literal(arguments: &FunctionArgs, last: bool) -> Option<&AnonymousFunction> {
    let FunctionArgs::Parentheses { arguments, .. } = arguments else {
        return None;
    };
    let chosen = if last { arguments.iter().last() } else { arguments.iter().next() };
    match chosen {
        Some(Expression::Function(function)) => Some(function),
        _ => None,
    }
}

impl Visitor for Analyzer {
    fn visit_block(&mut self, block: &Block) {
        let scope = match self.pending.iter().rposition(|(pending, _)| std::ptr::eq(*pending, block)) {
            Some(index) => self.pending.remove(index).1,
            None => Vec::new(),
        };
        self.scopes.push(scope);
        if self.type_depth == 0 {
            self.find_regions(block);
        }
    }

    fn visit_block_end(&mut self, block: &Block) {
        if !self.deferred.last().is_some_and(|deferred| std::ptr::eq(*deferred, block)) {
            self.scopes.pop();
        }
    }

    fn visit_repeat(&mut self, node: &Repeat) {
        self.deferred.push(node.block());
    }

    fn visit_repeat_end(&mut self, _node: &Repeat) {
        self.deferred.pop();
        self.scopes.pop();
    }

    fn visit_numeric_for(&mut self, node: &NumericFor) {
        self.pending.push((node.block(), Self::locals([node.index_variable()])));
    }

    fn visit_generic_for(&mut self, node: &GenericFor) {
        self.pending.push((node.block(), Self::locals(node.names())));
    }

    fn visit_function_declaration(&mut self, node: &FunctionDeclaration) {
        if node.name().method_colon().is_some() {
            self.methods.insert(node.body());
        }
    }

    fn visit_function_body(&mut self, node: &FunctionBody) {
        self.function_depth += 1;
        let parameters = node.parameters().iter().filter_map(|parameter| match parameter {
            Parameter::Name(token) => Some(token),
            _ => None,
        });
        let mut locals = Self::locals(parameters);
        if self.methods.contains(&(node as *const FunctionBody)) {
            locals.insert(
                0,
                Local {
                    name: "self".to_owned(),
                    position: start(node),
                },
            );
        }
        self.pending.push((node.block(), locals));
    }

    fn visit_function_body_end(&mut self, _node: &FunctionBody) {
        self.function_depth -= 1;
    }

    fn visit_function_call(&mut self, call: &FunctionCall) {
        if self.type_depth > 0 {
            return;
        }
        if let Some((token, member, args)) = self.task_member(call, self.resolve(TASK).is_some()) {
            if (member == DESYNCHRONIZE || member == SYNCHRONIZE) && !self.markers.contains(&start(token)) {
                self.fail(line(token), format!("task.{member}() must be called on its own as a statement"));
                return;
            }
            if member == SPAWN
                && let Some(function) = literal(args, false)
            {
                self.function_region(function);
            }
            return;
        }
        for suffix in call.suffixes() {
            if let Suffix::Call(Call::MethodCall(method)) = suffix
                && name(method.name()) == BIND
                && let Some(function) = literal(method.args(), true)
            {
                self.function_region(function);
            }
        }
    }

    fn visit_local_function(&mut self, node: &LocalFunction) {
        self.declare(node.name());
    }

    fn visit_local_assignment_end(&mut self, node: &LocalAssignment) {
        for token in node.names() {
            self.declare(token);
        }
    }

    fn visit_const_function(&mut self, node: &ConstFunction) {
        self.declare(node.name());
    }

    fn visit_const_assignment_end(&mut self, node: &ConstAssignment) {
        for token in node.names() {
            self.declare(token);
        }
    }

    fn visit_type_info(&mut self, _node: &TypeInfo) {
        self.type_depth += 1;
    }

    fn visit_type_info_end(&mut self, _node: &TypeInfo) {
        self.type_depth -= 1;
    }

    fn visit_type_function(&mut self, _node: &TypeFunction) {
        self.type_depth += 1;
    }

    fn visit_type_function_end(&mut self, _node: &TypeFunction) {
        self.type_depth -= 1;
    }

    fn visit_var(&mut self, node: &Var) {
        if let Var::Name(token) = node {
            self.reference(token);
        }
    }

    fn visit_prefix(&mut self, node: &Prefix) {
        if let Prefix::Name(token) = node {
            self.reference(token);
        }
    }

    fn visit_expression(&mut self, node: &Expression) {
        let Expression::Symbol(token) = node else { return };
        if self.type_depth > 0 || name(token) != "..." {
            return;
        }
        let position = start(token);
        let depth = self.function_depth;
        let opener = self
            .regions
            .iter()
            .find(|region| region.kind == ClusterKind::Block && region.contains(position) && region.depth == depth)
            .map(|region| region.opener);
        if let Some(opener) = opener {
            self.fail(
                line(token),
                format!("`...` cannot be used directly inside a parallel block, store it in a local before {opener}"),
            );
        }
    }
}
