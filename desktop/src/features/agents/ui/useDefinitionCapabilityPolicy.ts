/**
 * Definition-dialog capability-policy state, split from
 * `AgentDefinitionDialog.tsx` (file-size guard). Owns the draft+seed pair
 * (same contract as the behavior quad in `personaBehaviorDraft.ts`), the
 * prompt-skills query behind the picker, and the submit gates: draft
 * validity, the runtime-compatibility preview, the SPEC-007 provider
 * release gate, and the general-003 unresolved-catalog-entry gate.
 */

import * as React from "react";

import type { AgentCapabilityPolicy } from "@/shared/api/capabilityPolicy";
import type { AcpRuntimeCatalogEntry } from "@/shared/api/types";
import { useBuzzPromptSkillsQuery } from "../hooks";
import { definitionSaveBlockedForUnresolvedRuntime } from "./agentConfigOptions";
import {
  buildCapabilityPolicy,
  capabilityCompatibility,
  capabilityDraftValid,
  capabilityForSubmit,
  DEFAULT_CAPABILITY_POLICY_DRAFT,
  draftFromCapabilityPolicy,
} from "./capabilityPolicyLogic";

export function useDefinitionCapabilityPolicy({
  runtime,
  selectedRuntime,
  isCreateMode,
  createProviderBacked,
  open,
}: {
  runtime: string;
  selectedRuntime: AcpRuntimeCatalogEntry | undefined;
  isCreateMode: boolean;
  createProviderBacked: boolean;
  open: boolean;
}) {
  // Same draft+seed contract as the behavior quad: an untouched draft
  // submits no capability group, keeping unrelated edits hash-quiet.
  const [draft, setDraft] = React.useState(DEFAULT_CAPABILITY_POLICY_DRAFT);
  const seedRef = React.useRef(DEFAULT_CAPABILITY_POLICY_DRAFT);

  const reseed = React.useCallback(
    (policy: AgentCapabilityPolicy | null | undefined) => {
      const next = draftFromCapabilityPolicy(policy);
      seedRef.current = next;
      setDraft(next);
    },
    [],
  );

  const reset = React.useCallback(() => {
    seedRef.current = DEFAULT_CAPABILITY_POLICY_DRAFT;
    setDraft(DEFAULT_CAPABILITY_POLICY_DRAFT);
  }, []);

  const promptSkillsQuery = useBuzzPromptSkillsQuery({ enabled: open });
  const support = selectedRuntime?.capabilitySupport;
  const blocked =
    support != null &&
    capabilityCompatibility(draft, support, selectedRuntime?.source).blocked;
  // SPEC-007: the provider release gate also covers CREATION — a provider
  // destination + a non-default policy draft blocks submit (never silently
  // drop the draft; the controls are replaced by the release-gate note).
  const providerGateBlocked =
    isCreateMode &&
    createProviderBacked &&
    buildCapabilityPolicy(draft) != null;
  // general-003 (P0): while the catalog entry is unresolved, the runtime's
  // capabilities are UNKNOWN — never treat them as false. Submitting here
  // would clear the stored provider/model via the fail-closed payload and
  // republish the clear fleet-wide.
  const catalogEntryUnresolved = definitionSaveBlockedForUnresolvedRuntime(
    runtime,
    selectedRuntime,
  );
  // No empty selections; the prospective runtime must honor the draft
  // (HC-003 — the backend re-validates as the backstop).
  const submitBlocked =
    !capabilityDraftValid(draft) ||
    blocked ||
    providerGateBlocked ||
    catalogEntryUnresolved;

  return {
    draft,
    setDraft,
    reseed,
    reset,
    support,
    catalogEntryUnresolved,
    submitBlocked,
    skills: promptSkillsQuery.data ?? [],
    forSubmit: (isEdit: boolean) =>
      capabilityForSubmit(draft, seedRef.current, isEdit),
  };
}
