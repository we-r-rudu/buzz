import assert from "node:assert/strict";
import test from "node:test";

import {
  buildCapabilityPolicy,
  capabilityCompatibility,
  capabilityDraftValid,
  capabilityForSubmit,
  capabilityOverrideForSubmit,
  DEFAULT_CAPABILITY_POLICY_DRAFT,
  draftFromCapabilityPolicy,
  overrideForSave,
  toggleSelectionId,
} from "./capabilityPolicyLogic.ts";
import { HARNESS_MANAGED_CAPABILITY_SUPPORT } from "@/shared/api/capabilityPolicy.ts";

// Synthetic VERIFIED fixture — no production runtime ships a verified
// transport in v1 (omp was downgraded by HC-001), but the UI's verified
// branch must stay pinned for the first launch-tested runtime.
const VERIFIED_SUPPORT = {
  toolPolicy: "verified",
  supportedToolIds: [
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
  ],
  unsupportedToolIds: [],
  skillsDisable: true,
  ambientSkillNote:
    "Disabling ambient skills also disables the bundled buzz-cli skill.",
};

// ── draft ↔ wire round-trips ─────────────────────────────────────────────────

test("default draft builds null (absent-stable) and round-trips", () => {
  assert.equal(buildCapabilityPolicy(DEFAULT_CAPABILITY_POLICY_DRAFT), null);
  assert.deepEqual(
    draftFromCapabilityPolicy(null),
    DEFAULT_CAPABILITY_POLICY_DRAFT,
  );
});

test("selected-mode draft builds the tagged wire policy and round-trips", () => {
  const draft = {
    toolsMode: "selected",
    toolIds: ["files.read", "web.search"],
    skillsMode: "selected",
    skillIds: ["buzz-cli"],
  };
  const policy = buildCapabilityPolicy(draft);
  assert.deepEqual(policy, {
    tools: { mode: "selected", selected: ["files.read", "web.search"] },
    skills: { mode: "selected", selected: ["buzz-cli"] },
  });
  assert.deepEqual(draftFromCapabilityPolicy(policy), draft);
});

test("none-mode groups survive the round-trip; defaults are omitted", () => {
  const policy = buildCapabilityPolicy({
    toolsMode: "none",
    toolIds: [],
    skillsMode: "inherit",
    skillIds: [],
  });
  assert.deepEqual(policy, { tools: { mode: "none" } });
  assert.deepEqual(draftFromCapabilityPolicy(policy), {
    toolsMode: "none",
    toolIds: [],
    skillsMode: "inherit",
    skillIds: [],
  });
});

// ── dedupe / toggle ──────────────────────────────────────────────────────────

test("buildCapabilityPolicy dedupes selections preserving first-seen order", () => {
  const policy = buildCapabilityPolicy({
    toolsMode: "selected",
    toolIds: ["browser", "files.read", "browser", "web.search", "files.read"],
    skillsMode: "selected",
    skillIds: ["b-cli", "a-cli", "b-cli"],
  });
  assert.deepEqual(policy.tools.selected, [
    "browser",
    "files.read",
    "web.search",
  ]);
  assert.deepEqual(policy.skills.selected, ["b-cli", "a-cli"]);
});

test("toggleSelectionId appends absent ids and removes present ones", () => {
  assert.deepEqual(toggleSelectionId(["a", "b"], "c"), ["a", "b", "c"]);
  assert.deepEqual(toggleSelectionId(["a", "b"], "a"), ["b"]);
});

// ── compatibility preview ────────────────────────────────────────────────────

test("verified runtime names the unsupported selected ids in order", () => {
  const support = {
    ...VERIFIED_SUPPORT,
    supportedToolIds: ["files.read"],
    unsupportedToolIds: ["browser", "web.search"],
  };
  const compat = capabilityCompatibility(
    {
      toolsMode: "selected",
      toolIds: ["web.search", "files.read", "browser"],
      skillsMode: "inherit",
      skillIds: [],
    },
    support,
    "builtin",
  );
  assert.deepEqual(compat.unsupportedToolIds, ["web.search", "browser"]);
  assert.equal(compat.toolPolicyUnsupported, false);
  assert.equal(compat.blocked, true);
});

test("builtin harness-managed runtime blocks tools, allows skills", () => {
  const selected = capabilityCompatibility(
    {
      toolsMode: "selected",
      toolIds: ["files.read"],
      skillsMode: "none",
      skillIds: [],
    },
    HARNESS_MANAGED_CAPABILITY_SUPPORT,
    "builtin",
  );
  assert.equal(selected.toolPolicyUnsupported, true);
  assert.equal(selected.blocked, true);
  assert.deepEqual(selected.unsupportedToolIds, []);

  const none = capabilityCompatibility(
    { toolsMode: "none", toolIds: [], skillsMode: "inherit", skillIds: [] },
    HARNESS_MANAGED_CAPABILITY_SUPPORT,
    "builtin",
  );
  assert.equal(none.blocked, true);

  // Skills policies never block on a BUILTIN harness-managed runtime: they
  // deliver via prompt sections (the ambient-skill limitation is copy).
  const harnessDefault = capabilityCompatibility(
    {
      toolsMode: "harness_default",
      toolIds: [],
      skillsMode: "selected",
      skillIds: ["buzz-cli"],
    },
    HARNESS_MANAGED_CAPABILITY_SUPPORT,
    "builtin",
  );
  assert.equal(harnessDefault.blocked, false);
  assert.equal(harnessDefault.skillsPolicyUnsupported, false);
});

test("preset/custom harnesses block ANY non-default policy (SPEC-006)", () => {
  for (const source of ["preset", "custom"]) {
    // Tools none and selected both blocked.
    for (const toolsDraft of [
      { toolsMode: "none", toolIds: [] },
      { toolsMode: "selected", toolIds: ["files.read"] },
    ]) {
      const compat = capabilityCompatibility(
        { ...toolsDraft, skillsMode: "inherit", skillIds: [] },
        HARNESS_MANAGED_CAPABILITY_SUPPORT,
        source,
      );
      assert.equal(compat.toolPolicyUnsupported, true, source);
      assert.equal(compat.blocked, true, source);
    }
    // Skills none and selected both blocked — the backend rejects every
    // explicit policy on preset/custom harnesses (§11.3).
    for (const skillsDraft of [
      { skillsMode: "none", skillIds: [] },
      { skillsMode: "selected", skillIds: ["buzz-cli"] },
    ]) {
      const compat = capabilityCompatibility(
        { toolsMode: "harness_default", toolIds: [], ...skillsDraft },
        HARNESS_MANAGED_CAPABILITY_SUPPORT,
        source,
      );
      assert.equal(compat.skillsPolicyUnsupported, true, source);
      assert.equal(compat.blocked, true, source);
    }
    // A fully-default draft stays unblocked (and the draft is preserved).
    const compat = capabilityCompatibility(
      DEFAULT_CAPABILITY_POLICY_DRAFT,
      HARNESS_MANAGED_CAPABILITY_SUPPORT,
      source,
    );
    assert.equal(compat.blocked, false, source);
  }
});

test("compatible draft is not blocked", () => {
  const compat = capabilityCompatibility(
    {
      toolsMode: "selected",
      toolIds: ["files.read", "web.search"],
      skillsMode: "selected",
      skillIds: ["buzz-cli"],
    },
    VERIFIED_SUPPORT,
    "builtin",
  );
  assert.deepEqual(compat.unsupportedToolIds, []);
  assert.equal(compat.blocked, false);
});

// ── blocked-switch preservation (HC-003) ─────────────────────────────────────

test("a runtime switch reports incompatibility without touching the draft", () => {
  // Draft authored against a verified runtime.
  const draft = {
    toolsMode: "selected",
    toolIds: ["files.read", "browser"],
    skillsMode: "selected",
    skillIds: ["buzz-cli"],
  };
  const before = structuredClone(draft);

  // User switches to a harness-managed runtime: save blocks, but the draft
  // is preserved verbatim so switching back restores the selections.
  const compat = capabilityCompatibility(
    draft,
    HARNESS_MANAGED_CAPABILITY_SUPPORT,
    "builtin",
  );
  assert.equal(compat.blocked, true);
  assert.deepEqual(draft, before);
  assert.deepEqual(buildCapabilityPolicy(draft), {
    tools: { mode: "selected", selected: ["files.read", "browser"] },
    skills: { mode: "selected", selected: ["buzz-cli"] },
  });

  // Back on a verified runtime the same draft is compatible again.
  assert.equal(
    capabilityCompatibility(draft, VERIFIED_SUPPORT, "builtin").blocked,
    false,
  );
});

// ── instance override tri-state ──────────────────────────────────────────────

test("overrideForSave: inherit clears to null, explicit draft sends the group", () => {
  assert.equal(
    overrideForSave(true, {
      toolsMode: "selected",
      toolIds: ["files.read"],
      skillsMode: "inherit",
      skillIds: [],
    }),
    null,
  );
  assert.deepEqual(
    overrideForSave(false, {
      toolsMode: "none",
      toolIds: [],
      skillsMode: "inherit",
      skillIds: [],
    }),
    { tools: { mode: "none" } },
  );
  // SPEC-002: an explicit all-defaults override is `{}` — the backend stores
  // Some(default), an InstanceOverride that MASKS a non-default definition
  // policy. `null` is reserved for "Inherit from definition" (clear).
  assert.deepEqual(overrideForSave(false, DEFAULT_CAPABILITY_POLICY_DRAFT), {});
});

// ── definition submit contract (behaviorForSubmit parity) ────────────────────

test("capabilityDraftValid blocks empty selections", () => {
  assert.equal(
    capabilityDraftValid({
      toolsMode: "selected",
      toolIds: [],
      skillsMode: "inherit",
      skillIds: [],
    }),
    false,
  );
  assert.equal(
    capabilityDraftValid({
      toolsMode: "harness_default",
      toolIds: [],
      skillsMode: "selected",
      skillIds: [],
    }),
    false,
  );
  assert.equal(capabilityDraftValid(DEFAULT_CAPABILITY_POLICY_DRAFT), true);
});

test("capabilityForSubmit: create sends nothing when default, group when set", () => {
  assert.equal(
    capabilityForSubmit(
      DEFAULT_CAPABILITY_POLICY_DRAFT,
      DEFAULT_CAPABILITY_POLICY_DRAFT,
      false,
    ),
    undefined,
  );
  assert.deepEqual(
    capabilityForSubmit(
      { toolsMode: "none", toolIds: [], skillsMode: "inherit", skillIds: [] },
      DEFAULT_CAPABILITY_POLICY_DRAFT,
      false,
    ),
    { tools: { mode: "none" } },
  );
});

test("capabilityForSubmit: edit is hash-quiet when unchanged, clears with {}", () => {
  const seed = {
    toolsMode: "selected",
    toolIds: ["files.read"],
    skillsMode: "inherit",
    skillIds: [],
  };
  assert.equal(
    capabilityForSubmit(structuredClone(seed), seed, true),
    undefined,
    "unchanged policy must submit nothing (hash-quiet)",
  );
  assert.deepEqual(
    capabilityForSubmit(DEFAULT_CAPABILITY_POLICY_DRAFT, seed, true),
    {},
    "resetting to defaults must send an explicit empty group",
  );
});

test("capabilityOverrideForSubmit: unchanged is hash-quiet, tri-state is explicit", () => {
  const stored = {
    tools: { mode: "selected", selected: ["files.read"] },
  };
  const matchingDraft = {
    toolsMode: "selected",
    toolIds: ["files.read"],
    skillsMode: "inherit",
    skillIds: [],
  };
  // Overriding with the same policy as stored → omit (hash-quiet).
  assert.equal(
    capabilityOverrideForSubmit(false, matchingDraft, stored),
    undefined,
  );
  // Inheriting with NO stored override → already null → omit.
  assert.equal(
    capabilityOverrideForSubmit(true, DEFAULT_CAPABILITY_POLICY_DRAFT, null),
    undefined,
  );
  // Inheriting WITH a stored override → explicit null clears it.
  assert.equal(
    capabilityOverrideForSubmit(true, DEFAULT_CAPABILITY_POLICY_DRAFT, stored),
    null,
  );
  // Overriding a definition-less record (stored null) → the group.
  assert.deepEqual(
    capabilityOverrideForSubmit(false, matchingDraft, null),
    stored,
  );
  // SPEC-002 tri-state: an explicit all-defaults override over a stored
  // policy submits `{}` — Some(default) — masking a non-default definition
  // policy instead of clearing back to inherit.
  assert.deepEqual(
    capabilityOverrideForSubmit(false, DEFAULT_CAPABILITY_POLICY_DRAFT, stored),
    {},
  );
  // An unchanged explicit-default override is hash-quiet.
  assert.equal(
    capabilityOverrideForSubmit(false, DEFAULT_CAPABILITY_POLICY_DRAFT, {}),
    undefined,
  );
  // …and creating that explicit-default override over plain inherit
  // (stored null) submits `{}`, never `null`.
  assert.deepEqual(
    capabilityOverrideForSubmit(false, DEFAULT_CAPABILITY_POLICY_DRAFT, null),
    {},
  );
});

test("SPEC-R2-001: the Harness-defaults/Inherit reset clears a stored policy and unblocks save", () => {
  // A stored non-default policy on a harness-managed runtime blocks save —
  // the backend guards (validate_policy_against_runtime) agree, that part
  // is load-bearing and unchanged.
  const storedDraft = {
    toolsMode: "selected",
    toolIds: ["files.read"],
    skillsMode: "selected",
    skillIds: ["buzz-cli"],
  };
  assert.equal(
    capabilityCompatibility(
      storedDraft,
      HARNESS_MANAGED_CAPABILITY_SUPPORT,
      "builtin",
    ).blocked,
    true,
  );
  assert.equal(
    capabilityCompatibility(
      storedDraft,
      HARNESS_MANAGED_CAPABILITY_SUPPORT,
      "custom",
    ).blocked,
    true,
  );

  // The reset draft — what the always-enabled "Harness defaults" / "Inherit"
  // radios produce. Stale selections stay in the draft untouched (HC-003);
  // the modes alone decide the wire value.
  const resetDraft = {
    toolsMode: "harness_default",
    toolIds: ["files.read"],
    skillsMode: "inherit",
    skillIds: ["buzz-cli"],
  };
  for (const source of ["builtin", "preset", "custom"]) {
    assert.equal(
      capabilityCompatibility(
        resetDraft,
        HARNESS_MANAGED_CAPABILITY_SUPPORT,
        source,
      ).blocked,
      false,
      `reset must unblock save on ${source}`,
    );
  }
  // Edit-mode submit of the reset sends the explicit clearing group — the
  // backend stores the default and re-omits the field, so hashes return to
  // the pre-feature baseline (the §9 rollback, reachable again).
  assert.deepEqual(capabilityForSubmit(resetDraft, storedDraft, true), {});
  // Definition side: the wire value of the reset is null (fully default).
  assert.equal(buildCapabilityPolicy(resetDraft), null);
  // Instance side: "Inherit from definition" clears a stored override to
  // null; an all-default OVERRIDE masks a non-default definition policy
  // with `{}` (SPEC-002), which is how a provider-backed agent escapes the
  // inherited-policy gate (round2-general-003).
  assert.equal(
    capabilityOverrideForSubmit(true, resetDraft, {
      tools: { mode: "selected", selected: ["files.read"] },
    }),
    null,
  );
  assert.deepEqual(
    capabilityOverrideForSubmit(false, resetDraft, {
      tools: { mode: "selected", selected: ["files.read"] },
    }),
    {},
  );
});
