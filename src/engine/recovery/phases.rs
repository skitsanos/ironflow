//! Topological phases of a validated execution graph.

use std::collections::{BTreeSet, HashMap};

/// Group the steps into phases: a step joins the phase after its last
/// dependency. Each step and each edge is visited once (Kahn's algorithm
/// with in-degree counts), so planning is linear in the size of the graph;
/// rescanning the waiting steps for every phase was quadratic on a chain
/// (IF-215). Members of a phase keep flow declaration order.
pub(super) fn topological_phases(
    graph: HashMap<String, BTreeSet<String>>,
    declaration_order: &HashMap<String, usize>,
) -> Result<Vec<Vec<String>>, Vec<String>> {
    let mut in_degree: HashMap<&str, usize> = graph.keys().map(|name| (name.as_str(), 0)).collect();
    for dependent in graph.values().flatten() {
        if let Some(degree) = in_degree.get_mut(dependent.as_str()) {
            *degree += 1;
        }
    }

    let in_declaration_order = |names: &mut Vec<&str>| {
        names.sort_by_key(|name| declaration_order[*name]);
    };
    let mut ready: Vec<&str> = in_degree
        .iter()
        .filter(|(_, degree)| **degree == 0)
        .map(|(name, _)| *name)
        .collect();
    in_declaration_order(&mut ready);

    let mut phases = Vec::new();
    while !ready.is_empty() {
        let mut released = Vec::new();
        for name in &ready {
            for dependent in &graph[*name] {
                if let Some(degree) = in_degree.get_mut(dependent.as_str()) {
                    *degree -= 1;
                    if *degree == 0 {
                        released.push(dependent.as_str());
                    }
                }
            }
        }
        in_declaration_order(&mut released);
        phases.push(ready.iter().map(|name| (*name).to_string()).collect());
        ready = released;
    }

    // Whatever was never released sits on a cycle or behind one.
    let mut cycle_steps: Vec<&str> = in_degree
        .iter()
        .filter(|(_, degree)| **degree > 0)
        .map(|(name, _)| *name)
        .collect();
    if cycle_steps.is_empty() {
        return Ok(phases);
    }
    in_declaration_order(&mut cycle_steps);
    Err(vec![format!(
        "Cycle detected in flow DAG involving steps: {}",
        cycle_steps.join(", ")
    )])
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    fn graph_of(edges: &[(usize, usize)], steps: usize) -> HashMap<String, BTreeSet<String>> {
        let mut graph: HashMap<String, BTreeSet<String>> = (0..steps)
            .map(|step| (format!("s{step}"), BTreeSet::new()))
            .collect();
        for (from, to) in edges {
            graph
                .get_mut(&format!("s{from}"))
                .unwrap()
                .insert(format!("s{to}"));
        }
        graph
    }

    fn phases_of(edges: &[(usize, usize)], steps: usize) -> Result<Vec<Vec<String>>, Vec<String>> {
        let order = (0..steps).map(|step| (format!("s{step}"), step)).collect();
        topological_phases(graph_of(edges, steps), &order)
    }

    fn names(steps: &[usize]) -> Vec<String> {
        steps.iter().map(|step| format!("s{step}")).collect()
    }

    #[test]
    fn a_long_dependency_chain_is_planned_in_linear_time() {
        // A 10,000-step chain took 10 s when every phase rescanned the
        // steps that were still waiting (IF-215). Ten times that is now
        // well inside the bound.
        const STEPS: usize = 100_000;
        let edges: Vec<_> = (1..STEPS).map(|step| (step - 1, step)).collect();

        let started = Instant::now();
        let phases = phases_of(&edges, STEPS).unwrap();

        assert!(
            started.elapsed() < Duration::from_secs(2),
            "took {:?}",
            started.elapsed()
        );
        assert_eq!(phases.len(), STEPS);
        assert_eq!(phases[0], names(&[0]));
        assert_eq!(phases[STEPS - 1], names(&[STEPS - 1]));
    }

    #[test]
    fn a_step_runs_in_the_phase_after_its_last_dependency() {
        // 0 -> 1 -> 3, 0 -> 2, 2 -> 3, 4 alone, 3 -> 5, 0 -> 5.
        let edges = [(0, 1), (1, 3), (0, 2), (2, 3), (3, 5), (0, 5)];

        assert_eq!(
            phases_of(&edges, 6).unwrap(),
            [names(&[0, 4]), names(&[1, 2]), names(&[3]), names(&[5])]
        );
    }

    #[test]
    fn phase_members_keep_declaration_order_whatever_released_them() {
        // Step 9 releases step 1 and step 0 releases step 8.
        let edges = [(9, 1), (0, 8)];

        let phases = phases_of(&edges, 10).unwrap();

        assert_eq!(phases[0], names(&[0, 2, 3, 4, 5, 6, 7, 9]));
        assert_eq!(phases[1], names(&[1, 8]));
    }

    #[test]
    fn a_cycle_names_its_steps_and_those_behind_it_in_declaration_order() {
        // 0 is free; 1 -> 2 -> 3 -> 1 is a cycle; 4 waits behind it.
        let edges = [(1, 2), (2, 3), (3, 1), (3, 4)];

        assert_eq!(
            phases_of(&edges, 5).unwrap_err(),
            ["Cycle detected in flow DAG involving steps: s1, s2, s3, s4"]
        );
    }
}
