/**
 * Capability-policy section of the agent definition dialog, split from
 * `AgentDefinitionDialog.tsx` (file-size guard). Pure presentation — the
 * `useDefinitionCapabilityPolicy` hook owns the draft and gates. Renders the
 * unresolved-catalog warning above the policy controls so a save that would
 * clear the stored provider/model is explained, not just disabled.
 */

import type {
  BuzzPromptSkillInfo,
  RuntimeCapabilitySupport,
} from "@/shared/api/types";
import { CapabilityPolicyFields } from "./CapabilityPolicyFields";
import type {
  CapabilityPolicyDraft,
  HarnessSource,
} from "./capabilityPolicyLogic";

export function DefinitionCapabilityPolicySection({
  catalogEntryUnresolved,
  disabled,
  draft,
  harnessSource,
  onDraftChange,
  providerBacked,
  skills,
  support,
}: {
  catalogEntryUnresolved: boolean;
  disabled: boolean;
  draft: CapabilityPolicyDraft;
  harnessSource: HarnessSource | undefined;
  onDraftChange: (next: CapabilityPolicyDraft) => void;
  providerBacked: boolean;
  skills: BuzzPromptSkillInfo[];
  support: RuntimeCapabilitySupport | undefined;
}) {
  return (
    <>
      {catalogEntryUnresolved ? (
        <p className="text-xs text-warning" role="status">
          This definition's harness can't be confirmed right now — the runtime
          catalog is still loading or unavailable. Save is disabled so the
          stored provider and model aren't lost.
        </p>
      ) : null}

      <CapabilityPolicyFields
        disabled={disabled}
        draft={draft}
        harnessSource={harnessSource}
        idPrefix="persona-capability"
        onDraftChange={onDraftChange}
        providerBacked={providerBacked}
        skills={skills}
        support={support}
        variant="definition"
      />
    </>
  );
}
