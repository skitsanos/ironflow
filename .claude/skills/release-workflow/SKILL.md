---
name: release-workflow
description: Prepare and cut an IronFlow release through develop and main, including PR resolution, integration validation, versioning, and tagging.
---

Read and follow the canonical skill in `.agents/skills/release-ironflow/SKILL.md`.
This entry only maps `/release-workflow` onto it. Every merge, version choice,
tag, push, and publication still needs the user's explicit approval, and a
release candidate never uses `IRONFLOW_PREPUSH_GATE=fast`.
