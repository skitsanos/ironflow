//! Child flows named by a flow's composition steps (IF-220).
//!
//! `ironflow validate` follows every child reference that is a literal path,
//! with the path rules the nodes apply at run time: an absolute path is used
//! as it is and a relative one is joined to the directory of the flow that
//! names it, then canonicalized. Each distinct child is loaded and checked
//! once, and the walk never goes below `IRONFLOW_MAX_SUBWORKFLOW_DEPTH`
//! levels, the depth at which a run refuses the child.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::engine::types::FlowDefinition;
use crate::lua::LuaRuntime;
use crate::nodes::NodeRegistry;

use super::references::child_refs;

const DEPTH_LIMIT: &str = "IRONFLOW_MAX_SUBWORKFLOW_DEPTH";

/// What the walk below one flow found.
#[derive(Default)]
pub(super) struct ChildReport {
    /// Distinct child flows that were loaded.
    pub(super) flows: usize,
    pub(super) errors: Vec<String>,
    pub(super) warnings: Vec<String>,
}

/// A canonical flow path and the edge by which it is reached.
#[derive(Clone)]
struct Frame {
    path: PathBuf,
    unconditional: bool,
}

struct CachedFlow {
    children: Rc<[Frame]>,
    // A shallower route can reach new files; an unconditional route can
    // turn a depth warning into an error. Equal visits need no further walk.
    visits: HashSet<(usize, bool)>,
}

struct Walker<'a> {
    registry: &'a NodeRegistry,
    limit: usize,
    loaded: HashMap<PathBuf, CachedFlow>,
    stack: Vec<Frame>,
    report: ChildReport,
}

/// Load and check every child flow reachable from `flow`.
pub(super) fn check(
    flow_path: &Path,
    flow: &FlowDefinition,
    registry: &NodeRegistry,
) -> ChildReport {
    let root = std::fs::canonicalize(flow_path).unwrap_or_else(|_| flow_path.to_path_buf());
    let mut report = ChildReport::default();
    let children = resolve_children(&root, flow, &mut report);
    let mut walker = Walker {
        registry,
        limit: crate::util::limits::max_subworkflow_depth(),
        loaded: HashMap::new(),
        stack: vec![Frame {
            path: root,
            unconditional: true,
        }],
        report,
    };
    walker.visit(&children);
    walker.report
}

fn resolve_children(parent: &Path, flow: &FlowDefinition, report: &mut ChildReport) -> Vec<Frame> {
    let mut children = Vec::new();
    for reference in child_refs(flow) {
        let joined = parent.parent().map_or_else(
            || PathBuf::from(&reference.file),
            |dir| dir.join(&reference.file),
        );
        let Ok(child) = std::fs::canonicalize(&joined) else {
            report.warnings.push(format!(
                "{}: step '{}' names child flow '{}', which cannot be found",
                display(parent),
                reference.step,
                reference.file
            ));
            continue;
        };
        children.push(Frame {
            path: child,
            unconditional: reference.unconditional,
        });
    }
    children
}

impl CachedFlow {
    fn load(child: &Path, registry: &NodeRegistry, report: &mut ChildReport) -> Self {
        let name = display(child);
        let loaded = child
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("path is not valid UTF-8"))
            .and_then(|path| LuaRuntime::validate_flow(path, registry));
        let children = match loaded {
            Ok(validated) => {
                for error in super::definition_errors(&validated.flow, registry) {
                    report.errors.push(format!("child flow {name}: {error}"));
                }
                report.flows += 1;
                resolve_children(child, &validated.flow, report)
            }
            Err(error) => {
                report
                    .errors
                    .push(format!("child flow {name} failed to load: {error:#}"));
                Vec::new()
            }
        };
        Self {
            children: children.into(),
            visits: HashSet::new(),
        }
    }
}

impl Walker<'_> {
    fn visit(&mut self, children: &[Frame]) {
        for child in children {
            if let Some(start) = self.stack.iter().position(|frame| frame.path == child.path) {
                self.report_cycle(start, &child.path, child.unconditional);
                continue;
            }
            let level = self.stack.len();
            let unconditional = self.path_is_unconditional(1) && child.unconditional;
            if level > self.limit {
                self.report_depth(&child.path, level, unconditional);
                continue;
            }
            let cached = self
                .loaded
                .entry(child.path.clone())
                .or_insert_with(|| CachedFlow::load(&child.path, self.registry, &mut self.report));
            if !cached.visits.insert((level, unconditional)) {
                continue;
            }
            let descendants = Rc::clone(&cached.children);
            self.stack.push(child.clone());
            self.visit(&descendants);
            self.stack.pop();
        }
    }

    /// Whether every flow from `from` to the top was reached unconditionally.
    fn path_is_unconditional(&self, from: usize) -> bool {
        self.stack[from..].iter().all(|frame| frame.unconditional)
    }

    fn chain(&self, from: usize, last: &Path) -> String {
        self.stack[from..]
            .iter()
            .map(|frame| display(&frame.path))
            .chain(std::iter::once(display(last)))
            .collect::<Vec<_>>()
            .join(" -> ")
    }

    fn report_cycle(&mut self, start: usize, child: &Path, unconditional: bool) {
        let chain = self.chain(start, child);
        let refusal = self.limit.saturating_add(1);
        if unconditional && self.path_is_unconditional(start + 1) {
            self.report.errors.push(format!(
                "child flow recursion: {chain} always starts itself again; a run fails \
                 at nesting level {refusal} ({DEPTH_LIMIT})"
            ));
        } else {
            self.report.warnings.push(format!(
                "child flow recursion: {chain} can start itself again; the child at \
                 nesting level {refusal} is refused ({DEPTH_LIMIT})"
            ));
        }
    }

    fn report_depth(&mut self, child: &Path, levels: usize, unconditional: bool) {
        let chain = self.chain(0, child);
        let limit = self.limit;
        if unconditional {
            self.report.errors.push(format!(
                "child flow chain {chain} nests {levels} levels, above the limit of {limit} \
                 ({DEPTH_LIMIT}); a run refuses the child below that level"
            ));
        } else {
            self.report.warnings.push(format!(
                "child flow chain {chain} can nest {levels} levels, above the limit of {limit} \
                 ({DEPTH_LIMIT}); this path was not followed past the limit"
            ));
        }
    }
}

fn display(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}
