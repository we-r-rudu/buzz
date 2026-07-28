/**
 * Harness-neutral capability policy types, split from `types.ts` (file-size
 * guard). Mirrors `managed_agents/types/capability_policy.rs` — the wire keys
 * (`tools` / `skills` / `mode` / `selected`) are single lowercase words, so
 * the JSON shape is identical on both sides and needs no case conversion.
 *
 * The portable policy group travels as ONE unit at every save boundary:
 * absent = don't touch, present = validate server-side and replace as a
 * group. `null` on the frontend models "harness defaults" (the Rust default).
 */

/** Semantic, harness-neutral tool capability. Wire names are stable dotted strings. */
export type ToolCapabilityId =
  | "files.read"
  | "files.write"
  | "code.search"
  | "code.intelligence"
  | "shell.execute"
  | "browser"
  | "web.search"
  | "subagents"
  | "task.tracking"
  | "image.inspect";

/** Every tool capability id, in the Rust declaration order (canonical listing order). */
export const TOOL_CAPABILITY_IDS: readonly ToolCapabilityId[] = [
  "files.read",
  "files.write",
  "code.search",
  "code.intelligence",
  "shell.execute",
  "browser",
  "web.search",
  "subagents",
  "task.tracking",
  "image.inspect",
];

/**
 * Tool capability policy. `harness_default` (absent) keeps the harness's
 * ambient tool set — byte-identical behavior to pre-feature agents.
 */
export type ToolPolicy =
  | { mode: "harness_default" }
  | { mode: "none" }
  | { mode: "selected"; selected: ToolCapabilityId[] };

/**
 * Skill policy. `inherit` (absent) keeps harness-default behavior: ambient
 * native skills untouched, no Buzz prompt-skill sections.
 */
export type SkillPolicy =
  | { mode: "inherit" }
  | { mode: "none" }
  | { mode: "selected"; selected: string[] };

/** The portable capability policy group (definition policy or instance override). */
export type AgentCapabilityPolicy = {
  tools?: ToolPolicy;
  skills?: SkillPolicy;
};

/** Whether a runtime's capability transport is verified for explicit policy. */
export type CapabilitySupportLevel = "verified" | "harness_managed";

/**
 * The capability-policy facts the UI needs, projected from the Rust runtime
 * catalog (`KnownAcpRuntime::capability_support`). The frontend never
 * maintains a rival copy of this table (features/agents/AGENTS.md one rule).
 */
export type RuntimeCapabilitySupport = {
  toolPolicy: CapabilitySupportLevel;
  supportedToolIds: ToolCapabilityId[];
  unsupportedToolIds: ToolCapabilityId[];
  skillsDisable: boolean;
  /** e.g. omp: "Disabling ambient skills also disables the bundled buzz-cli skill." */
  ambientSkillNote: string | null;
};

/** Fallback for pre-feature mock fixtures: harness-managed, nothing supported. */
export const HARNESS_MANAGED_CAPABILITY_SUPPORT: RuntimeCapabilitySupport = {
  toolPolicy: "harness_managed",
  supportedToolIds: [],
  unsupportedToolIds: [...TOOL_CAPABILITY_IDS],
  skillsDisable: false,
  ambientSkillNote: null,
};

/** Static Buzz prompt-skill catalog entry (prompt text never leaves Rust). */
export type BuzzPromptSkillInfo = {
  id: string;
  label: string;
  description: string;
};
