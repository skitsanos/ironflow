import { expect, test } from "bun:test";
import { mkdir, mkdtemp, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { executable, options, reclaim, runBatch, selections, type Metadata } from "../test_rust_targets";

test("the target plan retains lib, bin, integration, example and benchmark coverage", () => {
  const targets = ["lib", "bin", "test", "example", "bench"].map(kind => ({name: kind, kind: [kind]}));
  expect(selections(targets, new Set())).toEqual([
    ["--lib"], ["--bin", "bin"], ["--test", "test"], ["--example", "example"], ["--bench", "bench"],
  ]);
  expect(selections([{name:"build",kind:["custom-build"]}], new Set())).toEqual([]);
  expect(() => selections([{name:"unknown",kind:["future-kind"]}], new Set())).toThrow();
});

test("feature closure and harness arguments match the default and storage gates", () => {
  const definitions = {default:["pdf-render"],"pdf-render":[],postgres:["sqlx/postgres","dep:rustls"],redis:["dep:redis"],alias:["postgres"]};
  const config = options(["--features", "alias,redis", "--", "--test-threads=1"], definitions);
  expect([...config.features].sort()).toEqual(["alias","default","pdf-render","postgres","redis"]);
  expect(config.cargo).toEqual(["--features", "alias,redis"]);
  expect(config.harness).toEqual(["--test-threads=1"]);
  expect(options(["--no-default-features"], definitions).features.size).toBe(0);
  expect(options(["--all-features"], definitions).features.has("redis")).toBeTrue();
  const target = {name:"pg",kind:["test"],"required-features":["postgres"]};
  expect(selections([target], config.features)).toEqual([["--test","pg"]]);
  expect(selections([target], new Set())).toEqual([]);
  expect(() => options(["--features"], definitions)).toThrow();
  expect(() => options(["--features", "ironflow/postgres"], definitions)).toThrow();
  expect(() => options(["--features", "unknown"], definitions)).toThrow();
  expect(() => options(["--test", "one"], definitions)).toThrow();
});

test("only current-package test-profile executables are reclaimable", () => {
  const event = {reason:"compiler-artifact",package_id:"ironflow",profile:{test:true},executable:"/tmp/target/debug/deps/test-one"};
  expect(executable(event, "ironflow", "/tmp/target")).toBe(event.executable);
  expect(executable({...event,profile:{test:false}}, "ironflow", "/tmp/target")).toBeNull();
  expect(executable(event, "dependency", "/tmp/target")).toBeNull();
  expect(() => executable({...event,executable:"/tmp/target-other/file"}, "ironflow", "/tmp/target")).toThrow();
  expect(() => executable({...event,executable:"/tmp/target/../source"}, "ironflow", "/tmp/target")).toThrow();
});

test("artifact reclamation refuses a symlinked parent outside Cargo target", async () => {
  const temp = await mkdtemp(join(tmpdir(), "ironflow-rust-reclaim-"));
  try {
    const target = join(temp, "target");
    const outside = join(temp, "outside");
    await mkdir(target);
    await mkdir(outside);
    await writeFile(join(outside, "valuable"), "preserve");
    await symlink(outside, join(target, "alias"));
    await expect(reclaim(new Set([join(target,"alias","valuable")]), target)).rejects.toThrow();
    expect(await readFile(join(outside, "valuable"), "utf8")).toBe("preserve");
  } finally { await rm(temp, {recursive:true,force:true}); }
});

for (const status of [0, 23]) {
  test(`Cargo exit ${status} reclaims tests but preserves CLI, libraries and forged harness paths`, async () => {
    const root = await mkdtemp(join(tmpdir(), "ironflow-rust-batch-"));
    try {
      const target = join(root, "target");
      const deps = join(target,"debug","deps");
      await mkdir(deps, {recursive:true});
      const testFile = join(deps,"test-one");
      const cli = join(target,"debug","ironflow");
      const library = join(deps,"libironflow.rlib");
      const forged = join(deps,"unrelated-test");
      for (const file of [testFile, cli, library, forged]) await writeFile(file, "preserve");
      const event = (file:string,test:boolean) => ({reason:"compiler-artifact",package_id:"ironflow",profile:{test},executable:file});
      const program = join(root, "fake-cargo.ts");
      const events = [event(testFile,true),event(cli,false),{reason:"build-finished",success:status === 0},event(forged,true)];
      await writeFile(program, `${events.map(e => `console.log(${JSON.stringify(JSON.stringify(e))});`).join("\n")}\nconsole.log("fake libtest output");process.exit(${status});\n`);
      const metadata:Metadata = {workspace_root:root,target_directory:target,packages:[]};
      const result = runBatch(metadata, "ironflow", [process.execPath,program]);
      if (status) await expect(result).rejects.toThrow("exit 23"); else await result;
      expect(await Bun.file(testFile).exists()).toBeFalse();
      for (const file of [cli,library,forged]) expect(await Bun.file(file).text()).toBe("preserve");
    } finally { await rm(root, {recursive:true,force:true}); }
  });
}
