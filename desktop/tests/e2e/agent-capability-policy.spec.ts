import type { Page } from "@playwright/test";
import { expect, test } from "@playwright/test";

import { installMockBridge, TEST_IDENTITIES } from "../helpers/bridge";

// Capability-policy dialog coverage (HC-008): keyboard-only mode switch +
// selection + incompatibility announcement, plus the provider-backed release
// gate. The shared CapabilityPolicyFields component is native-input only
// (radios/checkboxes in fieldsets), so the whole flow is keyboard-driven.
//
// Bridge facts: the mock `list_buzz_prompt_skills` handler mirrors the Rust
// static catalog; the runtime's `capability_support` rides the existing
// `acpRuntimesCatalog` seed (pass-through fixture records) — no new mock
// infrastructure. The seeded agent's command is "goose" (bridge default), so
// the seeded goose entry carries the verified support fixture.

const AGENT_PUBKEY = TEST_IDENTITIES.tyler.pubkey;
const AGENT_NAME = "Tyler Agent";
const PERSONA_ID = "persona-capability-e2e";

/** Goose with a verified capability transport: browser + image.inspect unmapped. */
const GOOSE_VERIFIED_CAPABILITY_CATALOG = [
  {
    id: "goose",
    label: "Goose",
    avatar_url: "",
    availability: "available",
    command: "goose",
    binary_path: "/usr/local/bin/goose",
    default_args: ["acp"],
    mcp_command: null,
    install_hint: "Install Goose via the official install script.",
    install_instructions_url: "https://block.github.io/goose/",
    can_auto_install: true,
    requires_external_cli: true,
    underlying_cli_path: null,
    source: "builtin",
    provider_selection: true,
    capability_support: {
      tool_policy: "verified",
      supported_tool_ids: [
        "files.read",
        "files.write",
        "code.search",
        "code.intelligence",
        "shell.execute",
        "web.search",
        "subagents",
        "task.tracking",
      ],
      unsupported_tool_ids: ["browser", "image.inspect"],
      skills_disable: true,
      ambient_skill_note:
        "Disabling ambient skills also disables the bundled buzz-cli skill.",
    },
  },
];

async function openEditDialog(page: Page) {
  await page.goto("/");
  await page.getByTestId("open-agents-view").click();

  const agentButton = page.getByRole("button", {
    name: `${AGENT_NAME} agent profile`,
  });
  await expect(agentButton).toBeVisible({ timeout: 10_000 });
  await agentButton.click();

  await expect(page.getByTestId("user-profile-panel")).toBeVisible({
    timeout: 10_000,
  });
  await page.getByTestId("user-profile-edit-agent").click();

  await expect(page.getByTestId("edit-agent-dialog")).toBeVisible({
    timeout: 10_000,
  });
  // Provider field visible = runtime catalog loaded and form settled.
  await expect(page.locator("#edit-agent-llm-provider")).toBeVisible({
    timeout: 10_000,
  });
}

test.describe("agent capability policy", () => {
  test("keyboard-only override, selection, and incompatibility announcement", async ({
    page,
  }) => {
    await installMockBridge(page, {
      managedAgents: [
        {
          pubkey: AGENT_PUBKEY,
          name: AGENT_NAME,
          status: "stopped",
          channelNames: ["agents"],
        },
      ],
      acpRuntimesCatalog: GOOSE_VERIFIED_CAPABILITY_CATALOG,
    });

    await openEditDialog(page);

    // Default tri-state: inherit (the stored override is null).
    const inheritRadio = page.getByRole("radio", {
      name: /^Inherit from definition/,
    });
    await expect(inheritRadio).toBeChecked();
    // Definition-less agent: the inherit summary reads Harness defaults.
    await expect(
      page.getByRole("group", { name: "Tools and skills" }),
    ).toContainText("Harness defaults");

    // Keyboard: focus the checked radio, ArrowDown moves+checks within the
    // native radio group (override reveals the policy fieldsets).
    await inheritRadio.focus();
    await page.keyboard.press("ArrowDown");
    const overrideRadio = page.getByRole("radio", {
      name: "Override for this agent",
    });
    await expect(overrideRadio).toBeChecked();
    await expect(overrideRadio).toBeFocused();

    const toolsGroup = page.getByRole("group", { name: "Tools", exact: true });
    const submit = page.getByTestId("edit-agent-dialog-submit");

    // Tab into the tools mode group (its checked radio is the tab stop) and
    // arrow to Selected — the checkbox grid appears without a mouse.
    await page.keyboard.press("Tab");
    const harnessDefaultRadio = toolsGroup.getByRole("radio", {
      name: "Harness defaults",
    });
    await expect(harnessDefaultRadio).toBeFocused();
    await page.keyboard.press("ArrowDown");
    await expect(toolsGroup.getByRole("radio", { name: "None" })).toBeChecked();
    await page.keyboard.press("ArrowDown");
    await expect(
      toolsGroup.getByRole("radio", { name: "Selected" }),
    ).toBeChecked();

    // Tab through the selection grid. An empty Selected mode blocks Save
    // (the server rejects empty selections, so the client gates too).
    await expect(submit).toBeDisabled();
    await page.keyboard.press("Tab");
    const readFilesCheckbox = toolsGroup.getByRole("checkbox", {
      name: "Read files",
    });
    await expect(readFilesCheckbox).toBeFocused();
    // A supported selection unblocks Save.
    await page.keyboard.press("Space");
    await expect(readFilesCheckbox).toBeChecked();
    await expect(submit).toBeEnabled();
    for (let i = 0; i < 5; i += 1) {
      await page.keyboard.press("Tab");
    }
    const browserCheckbox = toolsGroup.getByRole("checkbox", {
      name: "Use the browser",
    });
    await expect(browserCheckbox).toBeFocused();

    // Select an unsupported capability: the aria-live region announces the
    // named ids and Save blocks (HC-003/HC-008).
    await page.keyboard.press("Space");
    await expect(browserCheckbox).toBeChecked();
    const capabilityNote = page.locator(
      "#edit-agent-capability-capability-note",
    );
    await expect(capabilityNote).toContainText(
      "Not supported by this harness: browser",
    );
    await expect(submit).toBeDisabled();

    // Deselect: the announcement clears and Save unblocks. The draft is
    // preserved across the blocked state (not reset).
    await page.keyboard.press("Space");
    await expect(browserCheckbox).not.toBeChecked();
    await expect(capabilityNote).not.toContainText("Not supported");
    await expect(submit).toBeEnabled();
    await expect(
      toolsGroup.getByRole("radio", { name: "Selected" }),
    ).toBeChecked();
  });

  test("provider-backed agents see the release-gate note and safe clears stay reachable", async ({
    page,
  }) => {
    await installMockBridge(page, {
      managedAgents: [
        {
          pubkey: AGENT_PUBKEY,
          name: AGENT_NAME,
          status: "stopped",
          channelNames: ["agents"],
          backend: { type: "provider", id: "mock-provider", config: {} },
        },
      ],
    });

    await openEditDialog(page);

    await expect(
      page.getByText(
        "Tool and skill policies aren't available for provider-backed agents yet.",
      ),
    ).toBeVisible();
    // The inherit/override radios stay rendered so the safe clears remain
    // reachable (round2-general-003); the tools/skills mode controls don't.
    const capabilitySection = page.locator(
      'section[aria-label="Tools and skills"]',
    );
    await expect(
      capabilitySection.getByRole("radio", {
        name: /^Inherit from definition/,
      }),
    ).toBeChecked();
    await expect(
      capabilitySection.getByRole("radio", { name: "Override for this agent" }),
    ).toBeVisible();
    await expect(
      capabilitySection.getByRole("radio", {
        name: "Harness defaults",
        exact: true,
      }),
    ).toHaveCount(0);
    // No stored or inherited policy → no block announcement.
    await expect(
      page.locator("#edit-agent-capability-capability-note"),
    ).not.toContainText("Save is blocked");
  });

  test("definition dialog renders the policy section (harness-managed locks tools)", async ({
    page,
  }) => {
    // Persona-linked agents route Edit to the DEFINITION editor (see the
    // edit-agent routing pin). The seeded persona has no runtime pinned, so
    // the dialog auto-seeds the app default (mock catalog: goose, which is
    // harness-managed) — the tool policy locks while skills stay editable
    // and keyboard-switchable.
    await installMockBridge(page, {
      globalAgentConfig: {
        provider: "anthropic",
        model: "claude-opus-4-5",
        env_vars: { ANTHROPIC_API_KEY: "sk-ant-test" },
      },
      managedAgents: [
        {
          pubkey: AGENT_PUBKEY,
          name: AGENT_NAME,
          personaId: PERSONA_ID,
          status: "stopped",
          channelNames: ["agents"],
        },
      ],
      personas: [
        {
          id: PERSONA_ID,
          displayName: "Capability E2E Persona",
          systemPrompt: "You are the capability e2e persona.",
        },
      ],
    });

    await page.goto("/");
    await page.getByTestId("open-agents-view").click();
    const agentButton = page.getByRole("button", {
      name: "Capability E2E Persona agent profile",
    });
    await expect(agentButton).toBeVisible({ timeout: 10_000 });
    await agentButton.click();
    await page.getByTestId("user-profile-edit-agent").click();
    await expect(page.getByTestId("persona-dialog")).toBeVisible({
      timeout: 10_000,
    });

    const toolsGroup = page.getByRole("group", { name: "Tools", exact: true });
    await expect(toolsGroup).toContainText("manages its own tools");
    // SPEC-R2-001: the unsafe choices disable on a harness-managed runtime,
    // but the "Harness defaults" reset stays selectable (the §9 rollback).
    await expect(
      toolsGroup.getByRole("radio", { name: "Harness defaults" }),
    ).toBeEnabled();
    await expect(
      toolsGroup.getByRole("radio", { name: "None" }),
    ).toBeDisabled();
    await expect(
      toolsGroup.getByRole("radio", { name: "Selected" }),
    ).toBeDisabled();

    // Skills stay editable: keyboard-switch Inherit → None via arrow keys.
    const skillsGroup = page.getByRole("group", { name: "Skills" });
    const inheritSkillRadio = skillsGroup.getByRole("radio", {
      name: "Inherit",
    });
    await expect(inheritSkillRadio).toBeChecked();
    await inheritSkillRadio.focus();
    await page.keyboard.press("ArrowDown");
    await expect(
      skillsGroup.getByRole("radio", { name: "None" }),
    ).toBeChecked();

    // No incompatibility: a default draft on a harness-managed runtime saves
    // unblocked.
    await expect(
      page.locator("#persona-capability-capability-note"),
    ).not.toContainText("Not supported");
    await expect(page.getByTestId("persona-dialog-submit")).toBeEnabled();
  });

  test("stored incompatible policy resets to Harness defaults and saves (SPEC-R2-001)", async ({
    page,
  }) => {
    // Seed a persona carrying an incompatible stored tools policy — the
    // mixed-version/inbound state that used to trap the dialog: the draft
    // blocks save, and the reset control was inside the disabled fieldset.
    await installMockBridge(page, {
      globalAgentConfig: {
        provider: "anthropic",
        model: "claude-opus-4-5",
        env_vars: { ANTHROPIC_API_KEY: "sk-ant-test" },
      },
      managedAgents: [
        {
          pubkey: AGENT_PUBKEY,
          name: AGENT_NAME,
          personaId: PERSONA_ID,
          status: "stopped",
          channelNames: ["agents"],
        },
      ],
      personas: [
        {
          id: PERSONA_ID,
          displayName: "Capability E2E Persona",
          systemPrompt: "You are the capability e2e persona.",
          capabilityPolicy: {
            tools: { mode: "selected", selected: ["files.read"] },
          },
        },
      ],
    });

    await page.goto("/");
    await page.getByTestId("open-agents-view").click();
    const agentButton = page.getByRole("button", {
      name: "Capability E2E Persona agent profile",
    });
    await expect(agentButton).toBeVisible({ timeout: 10_000 });
    await agentButton.click();
    await page.getByTestId("user-profile-edit-agent").click();
    await expect(page.getByTestId("persona-dialog")).toBeVisible({
      timeout: 10_000,
    });

    const toolsGroup = page.getByRole("group", { name: "Tools", exact: true });
    const submit = page.getByTestId("persona-dialog-submit");
    const harnessDefaultRadio = toolsGroup.getByRole("radio", {
      name: "Harness defaults",
    });
    // The stored policy loads as the blocked draft: the unsupported choices
    // stay disabled, the safe reset stays enabled, and Save is gated.
    await expect(
      toolsGroup.getByRole("radio", { name: "Selected" }),
    ).toBeChecked();
    await expect(
      toolsGroup.getByRole("radio", { name: "Selected" }),
    ).toBeDisabled();
    await expect(harnessDefaultRadio).toBeEnabled();
    await expect(submit).toBeDisabled();

    // The §9 rollback: select Harness defaults → Save unblocks → save
    // succeeds and the policy clears to baseline.
    await harnessDefaultRadio.click();
    await expect(harnessDefaultRadio).toBeChecked();
    await expect(submit).toBeEnabled();
    await submit.click();
    await expect(page.getByTestId("persona-dialog")).toBeHidden({
      timeout: 10_000,
    });

    // Reopening shows the cleared default draft — the backend stored the
    // default and re-omits the field (hashes return to baseline).
    await expect(page.getByTestId("user-profile-panel")).toBeVisible({
      timeout: 10_000,
    });
    await page.getByTestId("user-profile-edit-agent").click();
    await expect(page.getByTestId("persona-dialog")).toBeVisible({
      timeout: 10_000,
    });
    await expect(
      page
        .getByRole("group", { name: "Tools", exact: true })
        .getByRole("radio", { name: "Harness defaults" }),
    ).toBeChecked();
    await expect(
      page.locator("#persona-capability-capability-note"),
    ).not.toContainText("can't honor");
    await expect(page.getByTestId("persona-dialog-submit")).toBeEnabled();
  });

  test("create flow: provider destination blocks a non-default policy draft (SPEC-007)", async ({
    page,
  }) => {
    await installMockBridge(page, {
      globalAgentConfig: {
        provider: "anthropic",
        model: "claude-opus-4-5",
        env_vars: { ANTHROPIC_API_KEY: "sk-ant-test" },
      },
      backendProviders: [{ id: "mock-provider" }],
    });

    await page.goto("/");
    await page.getByTestId("open-agents-view").click();
    await page.getByTestId("new-agent-card").click();
    await page.getByRole("menuitem", { name: "Create from scratch" }).click();
    const dialog = page.getByTestId("persona-dialog");
    await expect(dialog).toBeVisible({ timeout: 10_000 });
    await page.locator("#persona-display-name").fill("Policy Gate Agent");

    // A non-default draft the local destination accepts (skills deliver via
    // prompt sections on harness-managed runtimes).
    const skillsGroup = page.getByRole("group", { name: "Skills" });
    await skillsGroup.getByRole("radio", { name: "Selected" }).click();
    await skillsGroup.getByRole("checkbox", { name: /Buzz CLI/ }).check();
    const submit = page.getByTestId("persona-dialog-submit");
    await expect(submit).toBeEnabled();

    // Provider destination: the release-gate note replaces the controls and
    // submit blocks — the draft is preserved, never silently dropped.
    await page.locator("#agent-run-on").selectOption("mock-provider");
    await expect(
      page.getByText(
        "Tool and skill policies aren't available for provider-backed agents yet.",
      ),
    ).toBeVisible();
    await expect(
      page.getByText(
        /A tool or skill policy is set on this definition.*Save is blocked/s,
      ),
    ).toBeVisible();
    await expect(submit).toBeDisabled();

    // Back to local: the controls return with the draft intact and submit
    // unblocks.
    await page.locator("#agent-run-on").selectOption("local");
    await expect(
      skillsGroup.getByRole("radio", { name: "Selected" }),
    ).toBeChecked();
    await expect(
      skillsGroup.getByRole("checkbox", { name: /Buzz CLI/ }),
    ).toBeChecked();
    await expect(submit).toBeEnabled();
  });
});
