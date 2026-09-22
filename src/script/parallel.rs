use std::collections::HashSet;
use std::fmt;
use std::thread;

use full_moon::LuaVersion;
use full_moon::ast::luau::{ConstAssignment, ConstFunction, TypeFunction, TypeInfo};
use full_moon::ast::{
    Block, Call, Expression, FunctionArgs, FunctionBody, GenericFor, LocalAssignment, LocalFunction, NumericFor,
    Parameter, Prefix, Repeat, Stmt, Suffix, Var,
};
use full_moon::node::Node;
use full_moon::tokenizer::TokenReference;
use full_moon::visitors::Visitor;

pub const ENTER: &str = "EnterParallel";
pub const EXIT: &str = "ExitParallel";
pub const HOOK: &str = "__luv_parallel";

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cluster {
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
    source.contains(ENTER) || source.contains(EXIT)
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

    let mut main = String::with_capacity(source.len());
    let mut clusters = Vec::with_capacity(regions.len());
    let mut cursor = 0;
    for (index, region) in regions.into_iter().enumerate() {
        main.push_str(&source[cursor..region.start]);
        main.push_str(&format!("{HOOK}({}, \"{}\"", index + 1, region.captures.join(",")));
        for capture in &region.captures {
            main.push_str(", ");
            main.push_str(capture);
        }
        main.push(')');
        main.push_str(&"\n".repeat(newlines(&source[region.start..region.end])));
        cursor = region.end;

        let mut cluster = String::new();
        if !region.captures.is_empty() {
            cluster.push_str(&format!("local {} = ...;", region.captures.join(", ")));
        }
        cluster.push_str(&"\n".repeat(region.line - 1));
        cluster.push_str(&source[region.body_start..region.body_end]);
        clusters.push(Cluster {
            line: region.line,
            captures: region.captures,
            source: cluster,
        });
    }
    main.push_str(&source[cursor..]);

    Ok(Units { main, clusters })
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
    start: usize,
    end: usize,
    body_start: usize,
    body_end: usize,
    line: usize,
    depth: usize,
    captures: Vec<String>,
}

impl Region {
    fn contains(&self, position: usize) -> bool {
        (self.start..self.end).contains(&position)
    }
}

#[derive(Default)]
struct Analyzer {
    scopes: Vec<Vec<Local>>,
    pending: Vec<(*const Block, Vec<Local>)>,
    deferred: Vec<*const Block>,
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
        if let Some(region) = self.regions.iter_mut().find(|region| region.contains(position))
            && declared < region.start
            && !region.captures.contains(&name)
        {
            region.captures.push(name);
        }
    }

    fn marker<'a>(&mut self, stmt: &'a Stmt) -> Option<(&'static str, &'a TokenReference)> {
        let Stmt::FunctionCall(call) = stmt else { return None };
        let Prefix::Name(token) = call.prefix() else { return None };
        let kind = match name(token).as_str() {
            ENTER => ENTER,
            EXIT => EXIT,
            _ => return None,
        };
        let suffixes: Vec<&Suffix> = call.suffixes().collect();
        let [Suffix::Call(Call::AnonymousCall(args))] = suffixes.as_slice() else {
            return None;
        };
        if !matches!(args, FunctionArgs::Parentheses { arguments, .. } if arguments.is_empty()) {
            self.fail(line(token), format!("{kind}() does not take any arguments"));
            return None;
        }
        Some((kind, token))
    }

    fn find_regions(&mut self, block: &Block) {
        let mut open: Option<(usize, usize, usize)> = None;
        for (stmt, semicolon) in block.stmts_with_semicolon() {
            let Some((kind, token)) = self.marker(stmt) else { continue };
            let stmt_start = start(stmt);
            let stmt_end = semicolon.as_ref().map_or_else(|| end(stmt), end);
            let stmt_line = line(stmt);

            match (kind, open) {
                (ENTER, None) => {
                    if self.regions.iter().any(|region| region.contains(stmt_start)) {
                        self.fail(stmt_line, "EnterParallel() cannot be used inside another parallel block");
                        return;
                    }
                    self.markers.insert(start(token));
                    open = Some((stmt_start, stmt_end, stmt_line));
                }
                (ENTER, Some(_)) => {
                    self.fail(stmt_line, "EnterParallel() cannot be nested, call ExitParallel() first");
                    return;
                }
                (_, None) => {
                    self.fail(stmt_line, "ExitParallel() has no matching EnterParallel() in the same block");
                    return;
                }
                (_, Some((region_start, body_start, region_line))) => {
                    self.markers.insert(start(token));
                    self.regions.push(Region {
                        start: region_start,
                        end: stmt_end,
                        body_start,
                        body_end: stmt_start,
                        line: region_line,
                        depth: self.function_depth,
                        captures: Vec::new(),
                    });
                    open = None;
                }
            }
        }
        if let Some((_, _, region_line)) = open {
            self.fail(region_line, "EnterParallel() has no matching ExitParallel() in the same block");
        }
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

    fn visit_function_body(&mut self, node: &FunctionBody) {
        self.function_depth += 1;
        let parameters = node.parameters().iter().filter_map(|parameter| match parameter {
            Parameter::Name(token) => Some(token),
            _ => None,
        });
        self.pending.push((node.block(), Self::locals(parameters)));
    }

    fn visit_function_body_end(&mut self, _node: &FunctionBody) {
        self.function_depth -= 1;
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
        if self
            .regions
            .iter()
            .any(|region| region.contains(position) && region.depth == depth)
        {
            self.fail(
                line(token),
                "`...` cannot be used directly inside a parallel block, store it in a local before EnterParallel()",
            );
        }
    }
}
