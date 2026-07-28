import * as React from "react";

import type {
  AgentCapabilityPolicy,
  BuzzPromptSkillInfo,
  RuntimeCapabilitySupport,
  ToolCapabilityId,
} from "@/shared/api/types";
import { TOOL_CAPABILITY_IDS } from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
import {
  buildCapabilityPolicy,
  capabilityCompatibility,
  type CapabilityPolicyDraft,
  draftFromCapabilityPolicy,
  type HarnessSource,
  toggleSelectionId,
} from "./capabilityPolicyLogic";

/**
 * Harness-neutral capability-policy editor, shared by the definition dialog
 * (policy on the definition itself) and the instance edit dialog (override or
 * "Inherit from definition"). Keyboard-accessible by construction: native
 * radio groups for modes, native checkboxes for selections, fieldset+legend
 * per section, and an aria-live region for the compatibility preview.
 *
 * The policy draft is never mutated on a runtime switch — incompatible
 * selections are reported and block save, but stay in form state so switching
 * back restores them (HC-003).
 */

/** Plain-English labels for the semantic tool capabilities (no runtime jargon). */
const TOOL_CAPABILITY_LABELS: Record<ToolCapabilityId, string> = {
  "files.read": "Read files",
  "files.write": "Write and edit files",
  "code.search": "Search code",
  "code.intelligence": "Code intelligence (go to definition, find references)",
  "shell.execute": "Run shell commands",
  browser: "Use the browser",
  "web.search": "Search the web",
  subagents: "Delegate to subagents",
  "task.tracking": "Track tasks",
  "image.inspect": "Inspect images",
};

const CONTROL_CLASS =
  "size-4 accent-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50";

function summarizePolicy(policy: AgentCapabilityPolicy | null): string {
  if (!policy) return "Harness defaults";
  const parts: string[] = [];
  if (policy.tools?.mode === "none") parts.push("no tools");
  if (policy.tools?.mode === "selected")
    parts.push(`${policy.tools.selected.length} tool(s)`);
  if (policy.skills?.mode === "none") parts.push("no skills");
  if (policy.skills?.mode === "selected")
    parts.push(`${policy.skills.selected.length} skill(s)`);
  return parts.length > 0 ? parts.join(", ") : "Harness defaults";
}

export function CapabilityPolicyFields({
  variant,
  draft,
  onDraftChange,
  support,
  skills,
  disabled = false,
  harnessSource,
  providerBacked = false,
  inheritFromDefinition,
  onInheritFromDefinitionChange,
  definitionPolicy,
  idPrefix,
}: {
  variant: "definition" | "instance";
  draft: CapabilityPolicyDraft;
  onDraftChange: (next: CapabilityPolicyDraft) => void;
  /** Catalog-projected support for the prospective runtime (undefined while loading). */
  support: RuntimeCapabilitySupport | undefined;
  /** Static Buzz prompt-skill catalog (listBuzzPromptSkills). */
  skills: BuzzPromptSkillInfo[];
  /** Locked entirely (team personas are non-editable). */
  disabled?: boolean;
  /**
   * The catalog entry's provenance (SPEC-006). Preset/custom harnesses reject
   * EVERY explicit policy at the save boundary (§11.3), so both fieldsets
   * lock and any non-default draft blocks with an announcement; built-in
   * harness-managed runtimes lock tools but still take Buzz prompt skills.
   * Undefined while the entry is unresolved — treated as builtin (the blank
   * "No preference" runtime resolves to the built-in app default).
   */
  harnessSource?: HarnessSource;
  /**
   * Provider-backed instance: the release gate replaces the controls with a
   * note until the provider script exports the lossless args transport.
   */
  providerBacked?: boolean;
  /** Instance variant only: override vs inherit tri-state. */
  inheritFromDefinition?: boolean;
  onInheritFromDefinitionChange?: (inherit: boolean) => void;
  /** Instance variant only: the linked definition's policy, for the inherit summary. */
  definitionPolicy?: AgentCapabilityPolicy | null;
  idPrefix: string;
}) {
  const noteId = `${idPrefix}-capability-note`;
  const toolsLocked = support?.toolPolicy !== "verified";
  // SPEC-006: preset/custom harnesses reject every explicit policy at the
  // save boundary — both fieldsets lock. Undefined source is the built-in
  // app default (blank runtime), so it does NOT lock skills.
  const skillsLocked = harnessSource === "preset" || harnessSource === "custom";
  // The compatibility preview reads the EFFECTIVE policy: the draft when
  // overriding, the definition's policy when inheriting (a harness switch
  // incompatible with the inherited policy blocks save too — HC-003).
  const effectivePolicy: AgentCapabilityPolicy | null =
    variant === "instance" && inheritFromDefinition
      ? (definitionPolicy ?? null)
      : buildCapabilityPolicy(draft);
  const compatibility = React.useMemo(() => {
    if (!support) return null;
    const effectiveDraft: CapabilityPolicyDraft =
      variant === "instance" && inheritFromDefinition
        ? draftFromCapabilityPolicy(effectivePolicy)
        : draft;
    return capabilityCompatibility(effectiveDraft, support, harnessSource);
  }, [
    support,
    draft,
    variant,
    inheritFromDefinition,
    effectivePolicy,
    harnessSource,
  ]);

  const blocked = compatibility?.blocked ?? false;

  if (providerBacked) {
    // SPEC-007: the create flow can arrive here with a non-default draft
    // (set before the provider destination was picked). Submit is blocked
    // with an explicit reason — the draft is never silently dropped, and
    // switching the destination back to local restores the controls with
    // the draft intact.
    //
    // round2-general-003 (instance variant): the gate reads the EFFECTIVE
    // policy — the override draft when overriding, the LINKED DEFINITION's
    // policy when inheriting — because the backend resolves the same way at
    // deploy time. The inherit/override radios stay rendered so the safe
    // escapes remain reachable: "Inherit from definition" clears a stored
    // override, and an all-default override masks the definition's policy.
    const policySet = buildCapabilityPolicy(draft) != null;
    const definitionPolicySet =
      buildCapabilityPolicy(
        draftFromCapabilityPolicy(definitionPolicy ?? null),
      ) != null;
    const effectiveBlocked =
      variant === "instance" &&
      (inheritFromDefinition === false ? policySet : definitionPolicySet);
    return (
      <section aria-label="Tools and skills" className="space-y-1.5">
        {variant === "instance" ? (
          <fieldset className="space-y-1.5" disabled={disabled}>
            <legend className="text-sm font-medium text-foreground">
              Tools and skills
            </legend>
            <label className="flex items-center gap-2 text-sm text-foreground">
              <input
                checked={inheritFromDefinition === true}
                className={CONTROL_CLASS}
                name={`${idPrefix}-capability-inherit`}
                onChange={() => onInheritFromDefinitionChange?.(true)}
                type="radio"
              />
              Inherit from definition
              {inheritFromDefinition ? (
                <span className="text-xs text-muted-foreground">
                  ({summarizePolicy(definitionPolicy ?? null)})
                </span>
              ) : null}
            </label>
            <label className="flex items-center gap-2 text-sm text-foreground">
              <input
                checked={inheritFromDefinition === false}
                className={CONTROL_CLASS}
                name={`${idPrefix}-capability-inherit`}
                onChange={() => onInheritFromDefinitionChange?.(false)}
                type="radio"
              />
              Override for this agent
            </label>
          </fieldset>
        ) : (
          <p className="text-sm font-medium text-foreground">
            Tools and skills
          </p>
        )}
        <p className="text-xs text-muted-foreground">
          Tool and skill policies aren't available for provider-backed agents
          yet.
        </p>
        <div aria-live="polite" id={noteId} role="status">
          {variant === "definition" && policySet ? (
            <p className="text-xs text-destructive">
              A tool or skill policy is set on this definition, but
              provider-backed agents can't carry one yet. Save is blocked —
              switch the run destination back to local to keep or change the
              policy.
            </p>
          ) : null}
          {variant === "instance" &&
          effectiveBlocked &&
          inheritFromDefinition === false ? (
            <p className="text-xs text-destructive">
              This agent carries a tool or skill policy override, but
              provider-backed agents can't carry one yet. Save is blocked —
              choose Inherit from definition to clear the override.
            </p>
          ) : null}
          {variant === "instance" &&
          effectiveBlocked &&
          inheritFromDefinition !== false ? (
            <p className="text-xs text-destructive">
              The linked definition carries a tool or skill policy, and
              provider-backed agents can't carry one yet. Save is blocked — edit
              the definition to remove the policy, or override this agent with
              Harness defaults.
            </p>
          ) : null}
        </div>
      </section>
    );
  }

  return (
    <section aria-label="Tools and skills" className="space-y-3">
      {variant === "instance" ? (
        <fieldset className="space-y-1.5" disabled={disabled}>
          <legend className="text-sm font-medium text-foreground">
            Tools and skills
          </legend>
          <label className="flex items-center gap-2 text-sm text-foreground">
            <input
              checked={inheritFromDefinition === true}
              className={CONTROL_CLASS}
              name={`${idPrefix}-capability-inherit`}
              onChange={() => onInheritFromDefinitionChange?.(true)}
              type="radio"
            />
            Inherit from definition
            {inheritFromDefinition ? (
              <span className="text-xs text-muted-foreground">
                ({summarizePolicy(definitionPolicy ?? null)})
              </span>
            ) : null}
          </label>
          <label className="flex items-center gap-2 text-sm text-foreground">
            <input
              checked={inheritFromDefinition === false}
              className={CONTROL_CLASS}
              name={`${idPrefix}-capability-inherit`}
              onChange={() => onInheritFromDefinitionChange?.(false)}
              type="radio"
            />
            Override for this agent
          </label>
        </fieldset>
      ) : (
        <p className="text-sm font-medium text-foreground">Tools and skills</p>
      )}

      {variant === "definition" || inheritFromDefinition === false ? (
        <>
          {/*
            SPEC-R2-001: the fieldsets stay ENABLED when toolsLocked/
            skillsLocked so the safe reset — "Harness defaults" / "Inherit" —
            remains selectable (the §9 rollback); only the unsupported
            choices (None/Selected + the selection checkboxes) disable.
            Locking the whole fieldset trapped any stored non-default policy:
            the draft blocked save, and the control that could clear it was
            inside the disabled fieldset. The draft is still never
            auto-mutated (HC-003) — resetting is a deliberate user action.
          */}
          <fieldset
            aria-describedby={blocked ? noteId : undefined}
            className="space-y-1.5"
          >
            <legend className="text-sm font-medium text-foreground">
              Tools
            </legend>
            {(
              [
                ["harness_default", "Harness defaults"],
                ["none", "None"],
                ["selected", "Selected"],
              ] as const
            ).map(([mode, label]) => (
              <label
                className="flex items-center gap-2 text-sm text-foreground"
                key={mode}
              >
                <input
                  checked={draft.toolsMode === mode}
                  className={CONTROL_CLASS}
                  disabled={
                    disabled || (mode !== "harness_default" && toolsLocked)
                  }
                  name={`${idPrefix}-tools-mode`}
                  onChange={() => onDraftChange({ ...draft, toolsMode: mode })}
                  type="radio"
                />
                {label}
              </label>
            ))}
            {toolsLocked ? (
              <p className="text-xs text-muted-foreground">
                {support == null
                  ? "Choose a harness to edit the tool policy — without one, the app default manages its own tools."
                  : "This harness manages its own tools; a structured tool policy isn't available."}
              </p>
            ) : null}
            {draft.toolsMode === "selected" ? (
              <div className="grid max-h-40 grid-cols-1 gap-1 overflow-y-auto py-1 sm:grid-cols-2">
                {TOOL_CAPABILITY_IDS.map((id) => (
                  <label
                    className="flex items-center gap-2 text-sm text-foreground"
                    key={id}
                  >
                    <input
                      checked={draft.toolIds.includes(id)}
                      className={CONTROL_CLASS}
                      disabled={disabled || toolsLocked}
                      onChange={() =>
                        onDraftChange({
                          ...draft,
                          toolIds: toggleSelectionId(draft.toolIds, id),
                        })
                      }
                      type="checkbox"
                    />
                    {TOOL_CAPABILITY_LABELS[id]}
                  </label>
                ))}
              </div>
            ) : null}
          </fieldset>

          <fieldset className="space-y-1.5">
            <legend className="text-sm font-medium text-foreground">
              Skills
            </legend>
            {(
              [
                ["inherit", "Inherit"],
                ["none", "None"],
                ["selected", "Selected"],
              ] as const
            ).map(([mode, label]) => (
              <label
                className="flex items-center gap-2 text-sm text-foreground"
                key={mode}
              >
                <input
                  checked={draft.skillsMode === mode}
                  className={CONTROL_CLASS}
                  disabled={disabled || (mode !== "inherit" && skillsLocked)}
                  name={`${idPrefix}-skills-mode`}
                  onChange={() => onDraftChange({ ...draft, skillsMode: mode })}
                  type="radio"
                />
                {label}
              </label>
            ))}
            {draft.skillsMode === "selected" ? (
              <div className="space-y-1 py-1">
                {skills.map((skill) => (
                  <label
                    className="flex items-start gap-2 text-sm text-foreground"
                    key={skill.id}
                  >
                    <input
                      checked={draft.skillIds.includes(skill.id)}
                      className={cn(CONTROL_CLASS, "mt-0.5")}
                      disabled={disabled || skillsLocked}
                      onChange={() =>
                        onDraftChange({
                          ...draft,
                          skillIds: toggleSelectionId(draft.skillIds, skill.id),
                        })
                      }
                      type="checkbox"
                    />
                    <span>
                      <span className="font-medium">{skill.label}</span>
                      <span className="block text-xs text-muted-foreground">
                        {skill.description}
                      </span>
                    </span>
                  </label>
                ))}
              </div>
            ) : null}
            {draft.skillsMode !== "inherit" && support?.ambientSkillNote ? (
              <p className="text-xs text-muted-foreground">
                {support.ambientSkillNote}
              </p>
            ) : null}
            {skillsLocked ? (
              <p className="text-xs text-muted-foreground">
                Custom and preset harnesses manage their own skills; structured
                policies are only available for the built-in runtimes.
              </p>
            ) : null}
            {!skillsLocked &&
            draft.skillsMode !== "inherit" &&
            support != null &&
            !support.skillsDisable ? (
              <p className="text-xs text-muted-foreground">
                This harness's own skills stay enabled; selected Buzz skills are
                added to the agent's instructions instead.
              </p>
            ) : null}
          </fieldset>

          <p className="text-xs text-muted-foreground">
            Tool selection limits what the model can use; it is not a security
            sandbox.
          </p>

          <div aria-live="polite" id={noteId} role="status">
            {blocked ? (
              <p className="text-xs text-destructive">
                {skillsLocked
                  ? "Custom and preset harnesses manage their own tools and skills — structured policies aren't available for them. Choose Harness defaults and Inherit, or pick a built-in harness."
                  : compatibility?.toolPolicyUnsupported
                    ? "This harness can't honor a structured tool policy. Choose Harness defaults, or pick a different harness."
                    : `Not supported by this harness: ${compatibility?.unsupportedToolIds.join(", ")}. Save is blocked until the selection changes.`}
              </p>
            ) : null}
          </div>
        </>
      ) : null}
    </section>
  );
}
