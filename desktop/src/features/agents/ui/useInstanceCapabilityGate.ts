/**
 * Instance-edit capability-override gates, split from
 * `AgentInstanceEditDialog.tsx` (file-size guard). The dialog owns the
 * override draft + inherit toggle; this hook owns the prompt-skills query
 * behind the picker, the effective-policy compatibility preview, the §7
 * provider release gate, and the tri-state submit projection.
 */

import type { AgentCapabilityPolicy } from "@/shared/api/capabilityPolicy";
import type { AcpRuntimeCatalogEntry } from "@/shared/api/types";
import { useBuzzPromptSkillsQuery } from "../hooks";
import {
  buildCapabilityPolicy,
  capabilityCompatibility,
  capabilityDraftValid,
  capabilityOverrideForSubmit,
  type CapabilityPolicyDraft,
  draftFromCapabilityPolicy,
} from "./capabilityPolicyLogic";

export function useInstanceCapabilityGate({
  draft,
  inheritFromDefinition,
  definitionPolicy,
  isProviderBacked,
  prospectiveRuntime,
  storedOverride,
  open,
}: {
  draft: CapabilityPolicyDraft;
  inheritFromDefinition: boolean;
  definitionPolicy: AgentCapabilityPolicy | null | undefined;
  isProviderBacked: boolean;
  prospectiveRuntime: AcpRuntimeCatalogEntry | undefined;
  storedOverride: AgentCapabilityPolicy | null | undefined;
  open: boolean;
}) {
  const promptSkillsQuery = useBuzzPromptSkillsQuery({ enabled: open });
  // The compatibility preview reads the EFFECTIVE policy: the override draft,
  // or the linked definition's policy when inheriting (a harness switch
  // incompatible with the inherited policy blocks save too — HC-003).
  const effectiveDraft = inheritFromDefinition
    ? draftFromCapabilityPolicy(definitionPolicy)
    : draft;
  const support = prospectiveRuntime?.capabilitySupport;
  const blocked =
    !isProviderBacked &&
    support != null &&
    capabilityCompatibility(effectiveDraft, support, prospectiveRuntime?.source)
      .blocked;
  // round2-general-003: provider-backed agents gate on the EFFECTIVE policy
  // (the override draft, or the inherited definition policy) — the backend
  // release gate resolves the same policy at deploy time, so an inherited
  // definition policy must block Save here too. CapabilityPolicyFields
  // renders the matching explicit message in its aria-live region.
  const providerBlocked =
    isProviderBacked && buildCapabilityPolicy(effectiveDraft) != null;
  const valid =
    isProviderBacked || inheritFromDefinition || capabilityDraftValid(draft);

  return {
    support,
    // No empty selections; the prospective runtime must honor the effective
    // (override or inherited) policy — HC-003. The backend re-validates as
    // the backstop.
    submitBlocked: !valid || blocked || providerBlocked,
    skills: promptSkillsQuery.data ?? [],
    // Tri-state capability override: omitted when unchanged (hash-quiet),
    // null clears a stored override back to inherit, a policy replaces the
    // group. Provider-backed agents submit through the SAME contract: the
    // effective-policy gate above guarantees only default-effective
    // submissions reach this point from the dialog (undefined = unchanged,
    // null = clear, {} = mask), and the backend release gate in
    // build_deploy_payload is the backstop for anything else
    // (round2-general-003).
    forSubmit: () =>
      capabilityOverrideForSubmit(inheritFromDefinition, draft, storedOverride),
  };
}
