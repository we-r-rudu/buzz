/**
 * Pure capability-policy form logic for the definition/instance dialogs.
 * Extracted so mode transitions, dedupe, and the compatibility preview are
 * unit-testable without rendering (mirrors `harnessFormLogic.ts`).
 *
 * Runtime-change rule (HC-003): switching the harness NEVER mutates the
 * policy draft — incompatible selections are reported by
 * {@link capabilityCompatibility} and block save, but stay in form state so
 * switching back restores them. All functions here are pure; the draft is
 * never mutated.
 */

import type {
  AgentCapabilityPolicy,
  RuntimeCapabilitySupport,
  ToolCapabilityId,
} from "@/shared/api/capabilityPolicy";

export type ToolPolicyMode = "harness_default" | "none" | "selected";
export type SkillPolicyMode = "inherit" | "none" | "selected";

export type CapabilityPolicyDraft = {
  toolsMode: ToolPolicyMode;
  toolIds: ToolCapabilityId[];
  skillsMode: SkillPolicyMode;
  skillIds: string[];
};

export const DEFAULT_CAPABILITY_POLICY_DRAFT: CapabilityPolicyDraft = {
  toolsMode: "harness_default",
  toolIds: [],
  skillsMode: "inherit",
  skillIds: [],
};

/** Read a stored wire policy (or null = defaults) into draft form. */
export function draftFromCapabilityPolicy(
  policy: AgentCapabilityPolicy | null | undefined,
): CapabilityPolicyDraft {
  const tools = policy?.tools;
  const skills = policy?.skills;
  return {
    toolsMode: tools?.mode ?? "harness_default",
    toolIds: tools?.mode === "selected" ? [...tools.selected] : [],
    skillsMode: skills?.mode ?? "inherit",
    skillIds: skills?.mode === "selected" ? [...skills.selected] : [],
  };
}

/**
 * Build the wire policy from a draft. Returns `null` when every group is at
 * its default so serialization re-omits the field (absent-stable: hashes and
 * stored bytes return to the pre-feature baseline).
 */
export function buildCapabilityPolicy(
  draft: CapabilityPolicyDraft,
): AgentCapabilityPolicy | null {
  const policy: AgentCapabilityPolicy = {};
  if (draft.toolsMode === "none") {
    policy.tools = { mode: "none" };
  } else if (draft.toolsMode === "selected") {
    policy.tools = { mode: "selected", selected: dedupe(draft.toolIds) };
  }
  if (draft.skillsMode === "none") {
    policy.skills = { mode: "none" };
  } else if (draft.skillsMode === "selected") {
    policy.skills = { mode: "selected", selected: dedupe(draft.skillIds) };
  }
  return policy.tools === undefined && policy.skills === undefined
    ? null
    : policy;
}

/** Toggle one id in a selection list (append when absent, remove when present). */
export function toggleSelectionId<T>(ids: readonly T[], id: T): T[] {
  return ids.includes(id) ? ids.filter((value) => value !== id) : [...ids, id];
}

function dedupe<T>(ids: readonly T[]): T[] {
  return ids.filter((id, index) => ids.indexOf(id) === index);
}

/**
 * Save-button validity: a `selected` mode with an empty selection is
 * rejected server-side, so the client blocks it too (HC-003).
 */
export function capabilityDraftValid(draft: CapabilityPolicyDraft): boolean {
  return (
    (draft.toolsMode !== "selected" || draft.toolIds.length > 0) &&
    (draft.skillsMode !== "selected" || draft.skillIds.length > 0)
  );
}

/** Structural equality over built policies (seed-diff for hash-quiet submits). */
function policiesEqual(
  a: AgentCapabilityPolicy | null,
  b: AgentCapabilityPolicy | null,
): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

/**
 * Definition-dialog submit contract, mirroring `behaviorForSubmit`: an
 * untouched policy submits NOTHING (unrelated edits stay hash-quiet); a
 * changed policy submits the whole group; setting the policy back to
 * defaults in edit mode submits an explicit empty group (`{}`), which the
 * backend stores as the default and re-omits (absent-stable rollback).
 */
export function capabilityForSubmit(
  draft: CapabilityPolicyDraft,
  seed: CapabilityPolicyDraft,
  isEdit: boolean,
): AgentCapabilityPolicy | undefined {
  const next = buildCapabilityPolicy(draft);
  if (!isEdit) return next ?? undefined;
  if (policiesEqual(next, buildCapabilityPolicy(seed))) return undefined;
  return next ?? {};
}

export type CapabilityCompatibility = {
  /**
   * Selected tool ids the runtime has no verified mapping for, in selection
   * order. Empty on harness-managed runtimes — there the WHOLE explicit
   * policy is unsupported (`toolPolicyUnsupported`) instead.
   */
  unsupportedToolIds: ToolCapabilityId[];
  /** Explicit tool policy (none/selected) the runtime cannot honor. */
  toolPolicyUnsupported: boolean;
  /**
   * Non-default skills policy (none/selected) the runtime cannot honor.
   * Only preset/custom harnesses set this: the backend rejects EVERY explicit
   * policy there (§11.3 — their raw args are the only capability mechanism),
   * while built-in harness-managed runtimes still deliver Buzz skills via
   * composed prompt sections.
   */
  skillsPolicyUnsupported: boolean;
  /** Save is blocked while true (the backend re-validates as the backstop). */
  blocked: boolean;
};

/** The catalog entry's provenance — projected by the Rust catalog (AGENTS.md one rule). */
export type HarnessSource = "builtin" | "preset" | "custom";

/**
 * Compute the client-side compatibility preview for a draft against the
 * runtime's catalog-projected support facts. Pure — the draft is preserved
 * untouched regardless of the result.
 *
 * `source` is the catalog entry's provenance (SPEC-006): preset/custom
 * harnesses reject EVERY explicit policy at the save boundary, so any
 * non-default draft blocks; built-in harness-managed runtimes lock tools
 * but still take Buzz prompt skills. `undefined` fails closed (treated as
 * non-builtin) — it is only reachable while the catalog entry itself is
 * unresolved, in which case the component never calls this.
 */
export function capabilityCompatibility(
  draft: CapabilityPolicyDraft,
  support: RuntimeCapabilitySupport,
  source?: HarnessSource,
): CapabilityCompatibility {
  if (source !== "builtin") {
    // Preset/custom (or fail-closed unknown): the backend rejects any
    // explicit policy — tools AND skills (SPEC-006, §11.3).
    const toolPolicyUnsupported = draft.toolsMode !== "harness_default";
    const skillsPolicyUnsupported = draft.skillsMode !== "inherit";
    return {
      unsupportedToolIds: [],
      toolPolicyUnsupported,
      skillsPolicyUnsupported,
      blocked: toolPolicyUnsupported || skillsPolicyUnsupported,
    };
  }
  if (support.toolPolicy !== "verified") {
    const toolPolicyUnsupported = draft.toolsMode !== "harness_default";
    return {
      unsupportedToolIds: [],
      toolPolicyUnsupported,
      skillsPolicyUnsupported: false,
      blocked: toolPolicyUnsupported,
    };
  }
  const unsupportedToolIds =
    draft.toolsMode === "selected"
      ? draft.toolIds.filter((id) => !support.supportedToolIds.includes(id))
      : [];
  return {
    unsupportedToolIds,
    toolPolicyUnsupported: false,
    skillsPolicyUnsupported: false,
    blocked: unsupportedToolIds.length > 0,
  };
}

/**
 * The instance-level override to send on save: `null` clears to inherit the
 * definition (or harness defaults), a policy replaces the group as a unit.
 * `undefined` is never produced by the dialogs — the tri-state is always
 * explicit so an "Inherit from definition" choice actually clears a stored
 * override.
 *
 * SPEC-002: in override mode an all-default draft returns `{}` — the
 * EXPLICIT default override (`Some(AgentCapabilityPolicy::default())`) that
 * masks a non-default definition policy — never `null`, which the backend
 * reads as "clear to inherit" and would keep the definition's policy
 * running while the UI reports Harness defaults. `null` is reserved for the
 * "Inherit from definition" radio alone.
 */
export function overrideForSave(
  inheritFromDefinition: boolean,
  draft: CapabilityPolicyDraft,
): AgentCapabilityPolicy | null {
  if (inheritFromDefinition) return null;
  return buildCapabilityPolicy(draft) ?? {};
}

/**
 * Instance-dialog submit contract, mirroring `capabilityForSubmit`: an
 * unchanged resolved override submits NOTHING (unrelated edits stay
 * hash-quiet); a change submits the tri-state explicitly — `null` clears a
 * stored override back to inherit, a policy replaces the group as a unit.
 */
export function capabilityOverrideForSubmit(
  inheritFromDefinition: boolean,
  draft: CapabilityPolicyDraft,
  stored: AgentCapabilityPolicy | null | undefined,
): AgentCapabilityPolicy | null | undefined {
  const next = overrideForSave(inheritFromDefinition, draft);
  return policiesEqual(next, stored ?? null) ? undefined : next;
}
