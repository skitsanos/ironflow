//! Child flows named by a flow's composition steps (IF-220).
//!
//! `ironflow validate` follows every child reference that is a literal path,
//! with the path rules the nodes apply at run time: an absolute path is used
//! as it is and a relative one is joined to the directory of the flow that
//! names it, then canonicalized. Each distinct child is loaded and checked
//! once, and the walk never goes below `IRONFLOW_MAX_SUBWORKFLOW_DEPTH`
//! levels, the depth at which a run refuses the child.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::engine::types::FlowDefinition;
use crate::lua::LuaRuntime;
use crate::nodes::NodeRegistry;

use super::references::{ChildRef, child_refs};

const DEPTH_LIMIT: &str = "IRONFLOW_MAX_SUBWORKFLOW_DEPTH";

/// What the walk below one flow found.
#[derive(Default)]
pub(super) struct ChildReport {
    /// Distinct child flows that were loaded.
    pub(super) flows: usize,
    pub(super) errors: Vec<String>,
    pub(super) warnings: Vec<String>,
}

/// One flow on the current path and how it was reached.
struct Frame {
    path: PathBuf,
    unconditional: bool,
}

/// Longest chain of children below a flow that was already walked.
#[derive(Clone, Copy, Default)]
struct Height {
    any: usize,
    unconditional: usize,
}

struct Walker<'a> {
    registry: &'a NodeRegistry,
    limit: usize,
    done: HashMap<PathBuf, Height>,
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
    let mut walker = Walker {
        registry,
        limit: crate::util::limits::max_subworkflow_depth(),
        done: HashMap::new(),
        stack: vec![Frame {
            path: root,
            unconditional: true,
        }],
        report: ChildReport::default(),
    };
    walker.visit(flow);
    walker.report
}

impl Walker<'_> {
    /// Walk the children of the flow on top of the stack.
    fn visit(&mut self, flow: &FlowDefinition) -> Height {
        let mut height = Height::default();
        for reference in child_refs(flow) {
            let Some(below) = self.follow(&reference) else {
                continue;
            };
            height.any = height.any.max(below.any + 1);
            if reference.unconditional {
                height.unconditional = height.unconditional.max(below.unconditional + 1);
            }
        }
        height
    }

    /// Resolve one reference and walk its flow. `None` means the reference
    /// adds nothing below its parent: it was reported or cannot be followed.
    fn follow(&mut self, reference: &ChildRef) -> Option<Height> {
        let parent = &self.stack[self.stack.len() - 1].path;
        let joined = parent.parent().map_or_else(
            || PathBuf::from(&reference.file),
            |dir| dir.join(&reference.file),
        );
        let Ok(child) = std::fs::canonicalize(&joined) else {
            self.report.warnings.push(format!(
                "{}: step '{}' names child flow '{}', which cannot be found",
                display(parent),
                reference.step,
                reference.file
            ));
            return None;
        };

        if let Some(start) = self.stack.iter().position(|frame| frame.path == child) {
            self.report_cycle(start, &child, reference.unconditional);
            return None;
        }
        let level = self.stack.len();
        let reached_unconditionally = self.path_is_unconditional(1) && reference.unconditional;
        if level > self.limit {
            self.report_depth(&child, level, reached_unconditionally);
            return None;
        }
        if let Some(height) = self.done.get(&child).copied() {
            if reached_unconditionally && level + height.unconditional > self.limit {
                self.report_depth(&child, level + height.unconditional, true);
            } else if level + height.any > self.limit {
                self.report_depth(&child, level + height.any, false);
            }
            return Some(height);
        }
        Some(self.load(child, reference.unconditional))
    }

    /// Load one child, record its own errors and walk what it names.
    fn load(&mut self, child: PathBuf, unconditional: bool) -> Height {
        let name = display(&child);
        let loaded = child
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("path is not valid UTF-8"))
            .and_then(|path| LuaRuntime::validate_flow(path, self.registry));
        let height = match loaded {
            Ok(validated) => {
                for error in super::definition_errors(&validated.flow, self.registry) {
                    self.report
                        .errors
                        .push(format!("child flow {name}: {error}"));
                }
                self.report.flows += 1;
                self.stack.push(Frame {
                    path: child.clone(),
                    unconditional,
                });
                let height = self.visit(&validated.flow);
                self.stack.pop();
                height
            }
            Err(error) => {
                self.report
                    .errors
                    .push(format!("child flow {name} failed to load: {error:#}"));
                Height::default()
            }
        };
        self.done.insert(child, height);
        height
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
                 ({DEPTH_LIMIT}); flows below the limit were not loaded"
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
