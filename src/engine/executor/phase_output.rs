use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use anyhow::{Result, bail};
use serde_json::Value;
use tokio::sync::RwLock;
use tracing::warn;

use crate::engine::types::Context;

const MAX_REPORTED_COLLISIONS: usize = 16;

/// The context output that one completed task may publish at a phase barrier.
#[derive(Debug)]
pub(super) struct StepCompletion {
    step_name: String,
    output: Option<Arc<Context>>,
}

impl StepCompletion {
    pub(super) fn new(step_name: String, output: Option<Arc<Context>>) -> Self {
        Self { step_name, output }
    }

    pub(super) fn published(step_name: String, output: Arc<Context>) -> Self {
        Self::new(step_name, Some(output))
    }
}

#[derive(Debug)]
struct BufferedValue {
    value: Value,
    winning_rank: usize,
    writer_ranks: BTreeSet<usize>,
}

/// Phase-local output reduced to the deterministic winner for every key.
///
/// This accumulator remains private until `commit`, which makes publication
/// phase-atomic. Reducing each completion as it arrives also avoids retaining
/// full outputs that have already lost a same-key collision.
#[derive(Debug)]
pub(super) struct PhaseOutputAccumulator {
    phase: Vec<String>,
    ranks: HashMap<String, usize>,
    completed_steps: HashSet<String>,
    values: BTreeMap<String, BufferedValue>,
}

impl PhaseOutputAccumulator {
    pub(super) fn new(phase: &[String]) -> Self {
        Self {
            phase: phase.to_vec(),
            ranks: phase
                .iter()
                .enumerate()
                .map(|(rank, step_name)| (step_name.clone(), rank))
                .collect(),
            completed_steps: HashSet::new(),
            values: BTreeMap::new(),
        }
    }

    /// Reduce one arbitrarily timed completion into declaration-order winners.
    pub(super) fn record(&mut self, completion: StepCompletion) -> Result<()> {
        let Some(&rank) = self.ranks.get(&completion.step_name) else {
            bail!(
                "Executor received output for step '{}' outside its planned phase",
                completion.step_name
            );
        };
        if !self.completed_steps.insert(completion.step_name.clone()) {
            bail!(
                "Executor received duplicate output for step '{}'",
                completion.step_name
            );
        }
        let Some(output) = completion.output else {
            return Ok(());
        };

        for (key, value) in output.iter() {
            match self.values.get_mut(key) {
                Some(buffered) => {
                    buffered.writer_ranks.insert(rank);
                    if rank > buffered.winning_rank {
                        buffered.value = value.clone();
                        buffered.winning_rank = rank;
                    }
                }
                None => {
                    self.values.insert(
                        key.clone(),
                        BufferedValue {
                            value: value.clone(),
                            winning_rank: rank,
                            writer_ranks: BTreeSet::from([rank]),
                        },
                    );
                }
            }
        }
        Ok(())
    }

    /// Publish a settled phase without exposing a scheduler-dependent subset.
    pub(super) async fn commit(self, ctx: &Arc<RwLock<Arc<Context>>>) {
        if self.values.is_empty() {
            return;
        }

        let collision_count = self
            .values
            .values()
            .filter(|buffered| buffered.writer_ranks.len() > 1)
            .count();
        if collision_count > 0 {
            let reported: Vec<String> = self
                .values
                .iter()
                .filter(|(_, buffered)| buffered.writer_ranks.len() > 1)
                .take(MAX_REPORTED_COLLISIONS)
                .map(|(key, buffered)| format!("{key}->{}", self.phase[buffered.winning_rank]))
                .collect();
            warn!(
                collision_count,
                reported = %reported.join(", "),
                omitted = collision_count.saturating_sub(reported.len()),
                "Parallel context output collisions; later flow declarations win"
            );
        }

        let mut ctx_write = ctx.write().await;
        let shared = Arc::make_mut(&mut *ctx_write);
        shared.extend(
            self.values
                .into_iter()
                .map(|(key, buffered)| (key, buffered.value)),
        );
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn completion_timing_does_not_control_collision_precedence() {
        let ctx = Arc::new(RwLock::new(Arc::new(Context::new())));
        let phase = vec!["z_first".to_string(), "a_second".to_string()];
        let mut accumulator = PhaseOutputAccumulator::new(&phase);
        accumulator
            .record(StepCompletion::published(
                "a_second".to_string(),
                Arc::new(Context::from([
                    ("collision".to_string(), json!("second")),
                    ("second_only".to_string(), json!(true)),
                ])),
            ))
            .unwrap();
        accumulator
            .record(StepCompletion::published(
                "z_first".to_string(),
                Arc::new(Context::from([
                    ("collision".to_string(), json!("first")),
                    ("first_only".to_string(), json!(true)),
                ])),
            ))
            .unwrap();

        accumulator.commit(&ctx).await;

        let ctx = ctx.read().await;
        assert_eq!(ctx["collision"], "second");
        assert_eq!(ctx["first_only"], true);
        assert_eq!(ctx["second_only"], true);
    }

    #[test]
    fn rejects_duplicate_and_out_of_phase_completions() {
        let phase = vec!["only".to_string()];
        let mut accumulator = PhaseOutputAccumulator::new(&phase);
        accumulator
            .record(StepCompletion::new("only".to_string(), None))
            .unwrap();

        assert!(
            accumulator
                .record(StepCompletion::new("only".to_string(), None))
                .unwrap_err()
                .to_string()
                .contains("duplicate")
        );
        assert!(
            accumulator
                .record(StepCompletion::new("unknown".to_string(), None))
                .unwrap_err()
                .to_string()
                .contains("outside its planned phase")
        );
    }
}
