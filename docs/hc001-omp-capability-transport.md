# HC-001: omp capability-transport launch check (2026-07-28)

**Outcome: FAIL on capability granularity → omp ships `harness_managed`, not
`verified`. No `always_include` list is added — turns complete fine; the
plumbing-tool open risk (spec §3.1) did not materialize.**

## What was asked

Spec PLAN-HARNESS-CONFIG-001 §3.1/§11.12 conditions the omp `verified`
transport claim (and the no-`always_include` decision) on an executable
HC-001 smoke: launch the production compiler's args vector
(`["--tools=<csv>", "acp"]`) through buzz-acp's ACP client, complete ACP
`initialize` + one `session/prompt` turn, and exercise one allowed and one
excluded capability.

## Environment

- omp 17.1.6 (`/Users/pedromendes/.local/bin/omp`)
- Local provider credentials present (headless `omp -p "ok"` turns complete;
  no relay involved — buzz-acp's `AcpClient` speaks stdio JSON-RPC directly
  to the spawned `omp acp`).
- ACP client surface identical to production: buzz-acp's
  `build_client_capabilities()` (crates/buzz-acp/src/acp.rs) advertises only
  `auth.terminal` + `_meta` goose/terminal-auth keys — **no `fs`
  capability** — so every file operation observed below went through omp's
  NATIVE tool surface, exactly what production spawns.

## Method

For each args vector, spawned `omp <vector> acp` and drove ACP over stdio:
`initialize` (protocolVersion 2) → `session/new` → `session/prompt`, logging
`session/update` tool calls. Fresh fixture files per run; permission requests
answered `cancelled` (a denial must never auto-approve a write).

## Results (omp 17.1.6)

| # | Vector | Ask | Observed |
|---|--------|-----|----------|
| A | `--no-tools` | read a fixture file | Read **blocked**: "the filesystem-read tool isn't available in this session". Skill/lesson mutation tool calls (`manage_skill`/`learn` family, e.g. "Create no skill") still executed — `--no-tools` does NOT silence the skill/memory tools. |
| B | `--tools=read,grep,glob,ast_grep` | create a file | **Write SUCCEEDED**: `kind=edit` tool call "Create requested marker file", marker file existed after the turn. `files.write` was not in the vector. |
| C | `--tools=read` | create a file | **Write SUCCEEDED** (same as B). |
| D | `--tools=bash` | read a fixture file | Read **blocked** ("no direct `read` tool is exposed"); agent attempted `cat` via bash (permission-denied). |
| E | `--tools=read` | run a shell command | Shell **blocked** ("Shell execution is unavailable here"). |
| F | `--tools=grep` | create a file | Write **blocked** ("no filesystem write tool is available in this session"). |
| G | `--tools=write` | read a fixture file | Read **blocked** ("the advertised `read` tool is not exposed"). |
| — | `--tools=read` | "list your tools" | Model reports: `read`, **`write`**, `manage_skill`, `learn` directly callable, plus `xd://` devices (`ast_grep`, `ast_edit`, `lsp`, `browser`, `web_search`, `memory_edit`, `retain`, `recall`, `reflect`). |

`omp --tools=bogus_tool_name acp` fails fast ("Error: Unknown tool in
--tools"), so the flag IS parsed in `acp` mode; it is enforcement of the
selected set that is unreliable.

All turns completed `end_turn` — including `--no-tools` and minimal subsets —
so no plumbing tool (`yield`, `ask`, `goal`, …) is required for ACP
operation, and `--tools=<full-list> -p "ok"` (the previously recorded name
check) exits 0.

## Conclusion

The spec's mapping table promises `files.read → [read]` and
`files.write → [edit, write, ast_edit]` as DISTINCT capabilities — the point
of the feature is granting read WITHOUT write. omp 17.1.6 `acp` mode does
not honor that granularity:

- enabling `read` unlocks file **writes** (B, C — behavioral, not self-report);
- `--no-tools` still admits skill/lesson **mutation** tools (A);
- the model-visible surface under `--tools=read` includes unrequested
  browser/web_search/memory devices (—).

`ToolPolicy::Selected`/`None` on omp would therefore be false capability
parity — the plan's named primary risk. Decision (per the pre-authorized
remediation): the omp catalog entry downgrades to
`CapabilityTransport::HARNESS_MANAGED`; explicit tool policies on omp are
rejected at save and at the descriptor seam with the named-unsupported error,
identical to goose/claude/codex/buzz-agent. Skills policies still deliver
via composed prompt sections (portable), with the ambient-skill limitation
disclosed in the UI.

Re-verify and re-upgrade only with launch-test evidence against a fixed omp
release: rerun experiments A–G and require (1) writes blocked under a
read-only vector, (2) no unrequested tools model-visible, (3) turns still
completing. The probe script shape used above (stdio ACP:
initialize → session/new → session/prompt with cancelled permissions) is the
repeatable harness.

## Consequences applied in the same change

- `runtime_metadata.rs`: omp `capability_transport` → `HARNESS_MANAGED_TRANSPORT`;
  `OMP_TOOL_MAPPINGS`/`OMP_TRANSPORT` removed (no verified runtime ships in v1).
- `capability_compiler.rs`: compiler semantics now pinned against a fixture
  verified transport; omp assertions updated to the harness-managed behavior.
- UI: omp groups with the other built-in harness-managed runtimes (tools
  locked, Buzz prompt skills still selectable).
- `FORK.md` capability row: claim and pins updated; references this file.
