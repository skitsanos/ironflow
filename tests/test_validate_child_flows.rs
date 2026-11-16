//! IF-220 (e): `ironflow validate` loads the child flows a flow names and
//! reports recursion, instead of leaving both to the first run.

use std::path::Path;
use std::process::{Command, Output};

fn write(directory: &Path, name: &str, body: &str) {
    std::fs::write(directory.join(name), body).unwrap();
}

/// A flow whose single step starts `child` through `node`.
fn calling(name: &str, child: &str) -> String {
    format!(
        "local flow = Flow.new('{name}')\n\
         flow:step('call', nodes.subworkflow({{ flow = '{child}' }}))\n\
         return flow\n"
    )
}

const LEAF: &str = "local flow = Flow.new('leaf')\n\
    flow:step('say', nodes.log({ message = 'leaf' }))\n\
    return flow\n";

fn validate(directory: &Path, flow: &str, extra: &[&str], depth: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ironflow"));
    command
        .current_dir(directory)
        .env_remove("IRONFLOW_MAX_SUBWORKFLOW_DEPTH")
        .arg("validate")
        .arg(directory.join(flow))
        .args(extra);
    if let Some(depth) = depth {
        command.env("IRONFLOW_MAX_SUBWORKFLOW_DEPTH", depth);
    }
    command.output().expect("run ironflow CLI")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn a_valid_child_chain_passes() {
    let directory = tempfile::tempdir().unwrap();
    write(
        directory.path(),
        "parent.lua",
        &calling("parent", "leaf.lua"),
    );
    write(directory.path(), "leaf.lua", LEAF);

    let output = validate(directory.path(), "parent.lua", &["--strict"], None);
    let text = stdout(&output);
    assert!(output.status.success(), "{text}");
    assert!(text.contains("Validation: OK"), "{text}");
    assert!(text.contains("Child flows: 1"), "{text}");
}

#[test]
fn a_flow_that_always_calls_itself_fails_validation() {
    let directory = tempfile::tempdir().unwrap();
    write(directory.path(), "self.lua", &calling("self", "self.lua"));

    let output = validate(directory.path(), "self.lua", &[], None);
    let text = stdout(&output);
    assert_eq!(output.status.code(), Some(1), "{text}");
    assert!(
        text.contains("always starts itself again") && text.contains("self.lua -> self.lua"),
        "{text}"
    );
}

#[test]
fn mutual_recursion_names_the_whole_cycle() {
    let directory = tempfile::tempdir().unwrap();
    write(directory.path(), "a.lua", &calling("a", "b.lua"));
    // The same rules as at run time: relative to the calling flow's directory.
    std::fs::create_dir(directory.path().join("nested")).unwrap();
    write(directory.path(), "b.lua", &calling("b", "nested/c.lua"));
    write(
        &directory.path().join("nested"),
        "c.lua",
        &calling("c", "../a.lua"),
    );

    let output = validate(directory.path(), "a.lua", &[], None);
    let text = stdout(&output);
    assert_eq!(output.status.code(), Some(1), "{text}");
    assert!(text.contains("a.lua -> b.lua -> c.lua -> a.lua"), "{text}");
}

#[test]
fn an_invalid_child_fails_the_parent_and_is_named() {
    let directory = tempfile::tempdir().unwrap();
    write(
        directory.path(),
        "parent.lua",
        &calling("parent", "broken.lua"),
    );
    write(
        directory.path(),
        "broken.lua",
        "local flow = Flow.new('broken')\n\
         flow:step('a', nodes.log({ message = 'x' })):depends_on('missing')\n\
         return flow\n",
    );

    let output = validate(directory.path(), "parent.lua", &[], None);
    let text = stdout(&output);
    assert_eq!(output.status.code(), Some(1), "{text}");
    assert!(
        text.contains("broken.lua") && text.contains("missing"),
        "{text}"
    );
}

#[test]
fn guarded_recursion_and_a_missing_child_are_warnings_that_strict_rejects() {
    let directory = tempfile::tempdir().unwrap();
    write(
        directory.path(),
        "guarded.lua",
        "local flow = Flow.new('guarded')\n\
         flow:step('check', nodes.if_node({ condition = 'ctx.depth < 3' }))\n\
         flow:step('again', nodes.subworkflow({ flow = 'guarded.lua' }))\n\
             :depends_on('check'):route('true')\n\
         return flow\n",
    );
    write(
        directory.path(),
        "absent.lua",
        &calling("absent", "nowhere.lua"),
    );

    // The refusal at the limit is tolerated one level up, so the run succeeds.
    write(
        directory.path(),
        "tolerant.lua",
        "local flow = Flow.new('tolerant')\n\
         flow:step('again', nodes.subworkflow({ flow = 'tolerant.lua', on_error = 'ignore' }))\n\
         return flow\n",
    );

    for (flow, needle) in [
        ("guarded.lua", "can start itself again"),
        ("tolerant.lua", "can start itself again"),
        ("absent.lua", "nowhere.lua"),
    ] {
        let output = validate(directory.path(), flow, &[], None);
        let text = stdout(&output);
        assert!(output.status.success(), "{flow}: {text}");
        assert!(
            text.contains("Warnings:") && text.contains(needle),
            "{flow}: {text}"
        );

        let strict = validate(directory.path(), flow, &["--strict"], None);
        assert_eq!(strict.status.code(), Some(1), "{flow}: {}", stdout(&strict));
    }
}

#[test]
fn a_chain_deeper_than_the_nesting_limit_fails_and_the_walk_stops_there() {
    let directory = tempfile::tempdir().unwrap();
    for (name, child) in [("l0", "l1"), ("l1", "l2"), ("l2", "l3")] {
        write(
            directory.path(),
            &format!("{name}.lua"),
            &calling(name, &format!("{child}.lua")),
        );
    }
    // Never loaded: it sits below the limit, so its syntax error is not seen.
    write(directory.path(), "l3.lua", "this is not lua");

    let output = validate(directory.path(), "l0.lua", &[], Some("2"));
    let text = stdout(&output);
    assert_eq!(output.status.code(), Some(1), "{text}");
    assert!(
        text.contains("IRONFLOW_MAX_SUBWORKFLOW_DEPTH") && text.contains("l2.lua -> l3.lua"),
        "{text}"
    );
    assert!(
        !text.contains("not lua") && !text.contains("syntax"),
        "{text}"
    );

    let within = validate(directory.path(), "l0.lua", &[], Some("3"));
    assert_eq!(
        within.status.code(),
        Some(1),
        "l3.lua must be loaded and refused"
    );
}
