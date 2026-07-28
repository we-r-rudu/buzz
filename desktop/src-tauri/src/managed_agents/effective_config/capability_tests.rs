//! Capability-policy resolution + prompt-composition tests, split from
//! `tests.rs` (file-size cap).

use super::tests::{definition, global, record};
use super::*;

// ── Capability policy resolution (§2) ───────────────────────────────────────

use crate::managed_agents::{AgentCapabilityPolicy, SkillPolicy, ToolCapabilityId, ToolPolicy};

fn policy_with_tools(ids: &[ToolCapabilityId]) -> AgentCapabilityPolicy {
    AgentCapabilityPolicy {
        tools: ToolPolicy::Selected {
            selected: ids.to_vec(),
        },
        skills: SkillPolicy::Inherit,
    }
}

#[test]
fn linked_instance_override_wins_over_definition_policy() {
    let mut rec = record(Some("d1"), None, None, None);
    rec.capability_policy_override = Some(policy_with_tools(&[ToolCapabilityId::FilesRead]));
    let mut def = definition("d1", None, None, "prompt");
    def.capability_policy = policy_with_tools(&[ToolCapabilityId::Browser]);
    let defs = vec![def];

    let resolved = resolve_effective_capability_policy(&rec, &defs, &global(None, None));
    assert_eq!(resolved.source, ConfigSource::InstanceOverride);
    assert_eq!(
        resolved.policy.tools,
        ToolPolicy::Selected {
            selected: vec![ToolCapabilityId::FilesRead]
        }
    );
}

#[test]
fn linked_instance_falls_back_to_definition_policy() {
    let rec = record(Some("d1"), None, None, None);
    let mut def = definition("d1", None, None, "prompt");
    def.capability_policy = policy_with_tools(&[ToolCapabilityId::Browser]);
    let defs = vec![def];

    let resolved = resolve_effective_capability_policy(&rec, &defs, &global(None, None));
    assert_eq!(resolved.source, ConfigSource::Definition);
    assert_eq!(
        resolved.policy.tools,
        ToolPolicy::Selected {
            selected: vec![ToolCapabilityId::Browser]
        }
    );
}

#[test]
fn linked_instance_with_default_definition_policy_resolves_global_origin() {
    // "Global" labels the came-from-defaults origin; GlobalAgentConfig itself
    // carries NO policy layer.
    let rec = record(Some("d1"), None, None, None);
    let defs = vec![definition("d1", None, None, "prompt")];

    let resolved = resolve_effective_capability_policy(&rec, &defs, &global(None, None));
    assert_eq!(resolved.source, ConfigSource::Global);
    assert!(resolved.policy.is_default());
}

#[test]
fn definition_less_instance_override_is_instance_legacy() {
    let mut rec = record(None, None, None, None);
    rec.capability_policy_override = Some(policy_with_tools(&[ToolCapabilityId::CodeSearch]));

    let resolved = resolve_effective_capability_policy(&rec, &[], &global(None, None));
    assert_eq!(resolved.source, ConfigSource::InstanceLegacy);
    assert_eq!(
        resolved.policy.tools,
        ToolPolicy::Selected {
            selected: vec![ToolCapabilityId::CodeSearch]
        }
    );
}

#[test]
fn orphan_resolves_default_policy() {
    let rec = record(Some("missing"), None, None, None);
    let resolved = resolve_effective_capability_policy(&rec, &[], &global(None, None));
    assert!(resolved.policy.is_default());
}

#[test]
fn absent_everywhere_equals_pre_feature_resolution() {
    // A record + definition with no policy bytes resolves to the default
    // policy, and the composed prompt equals the raw persona prompt —
    // byte-identical to pre-feature behavior.
    let rec = record(Some("d1"), None, None, None);
    let defs = vec![definition(
        "d1",
        Some("gpt-x"),
        Some("openai"),
        "Persona prompt.",
    )];

    let cfg = resolve_effective_config(&rec, &defs, &global(None, None))
        .require_resolved()
        .unwrap();
    assert!(cfg.capability_policy.is_default());
    assert_eq!(cfg.system_prompt.value.as_deref(), Some("Persona prompt."));
    assert_eq!(
        cfg.base_system_prompt.value.as_deref(),
        Some("Persona prompt.")
    );
    assert_eq!(cfg.system_prompt, cfg.base_system_prompt);
}

// ── Prompt composition with fixture skills (§5) ─────────────────────────────

const FIXTURE_SKILLS: &[crate::managed_agents::prompt_skills::BuzzPromptSkill] = &[
    crate::managed_agents::prompt_skills::BuzzPromptSkill {
        id: "alpha",
        label: "Alpha",
        description: "first",
        prompt: "alpha body",
    },
    crate::managed_agents::prompt_skills::BuzzPromptSkill {
        id: "zeta",
        label: "Zeta",
        description: "second",
        prompt: "zeta body",
    },
];

#[test]
fn composed_prompt_appends_sorted_skill_sections() {
    let composed = compose_final_prompt_from(
        Some("Base.".to_string()),
        &["zeta".to_string(), "alpha".to_string()],
        FIXTURE_SKILLS,
    )
    .unwrap();
    assert_eq!(
        composed.as_deref(),
        Some("Base.\n\n[Skill: Alpha]\nalpha body\n\n[Skill: Zeta]\nzeta body")
    );
}

#[test]
fn composed_prompt_without_persona_prompt_is_sections_only() {
    let composed = compose_final_prompt_from(None, &["alpha".to_string()], FIXTURE_SKILLS).unwrap();
    assert_eq!(composed.as_deref(), Some("\n\n[Skill: Alpha]\nalpha body"));
}

#[test]
fn composed_prompt_rejects_unknown_skill_id_loudly() {
    let err = compose_final_prompt_from(None, &["nope".to_string()], FIXTURE_SKILLS).unwrap_err();
    assert!(err.contains("nope"), "{err}");
}

#[test]
fn resolve_composes_definition_skill_selection_into_prompt() {
    let rec = record(Some("d1"), None, None, None);
    let mut def = definition("d1", None, None, "Persona.");
    def.capability_policy = AgentCapabilityPolicy {
        tools: ToolPolicy::HarnessDefault,
        skills: SkillPolicy::Selected {
            selected: vec!["buzz-cli".to_string()],
        },
    };
    let defs = vec![def];

    let cfg = resolve_effective_config(&rec, &defs, &global(None, None))
        .require_resolved()
        .unwrap();
    let composed = cfg.system_prompt.value.expect("composed prompt");
    assert!(
        composed.starts_with("Persona.\n\n[Skill: Buzz CLI]\n"),
        "{composed}"
    );
    // The base field preserves the raw persona value.
    assert_eq!(cfg.base_system_prompt.value.as_deref(), Some("Persona."));
    assert_eq!(
        cfg.capability_policy.skills,
        SkillPolicy::Selected {
            selected: vec!["buzz-cli".to_string()],
        }
    );
}

#[test]
fn unknown_skill_in_store_is_an_invalid_policy_error() {
    // A hand-edited store must not silently drop policy: resolution surfaces
    // a typed InvalidPolicy so spawn/deploy refuse visibly while
    // summary/hash degrade like the orphan arm.
    let rec = record(Some("d1"), None, None, None);
    let mut def = definition("d1", None, None, "Persona.");
    def.capability_policy = AgentCapabilityPolicy {
        tools: ToolPolicy::HarnessDefault,
        skills: SkillPolicy::Selected {
            selected: vec!["hand-edited-unknown".to_string()],
        },
    };
    let defs = vec![def];

    match resolve_effective_config(&rec, &defs, &global(None, None)) {
        EffectiveConfigResult::InvalidPolicy { error } => {
            assert!(error.contains("hand-edited-unknown"), "{error}");
        }
        other => panic!("expected InvalidPolicy, got {other:?}"),
    }
}

// ── SPEC-004: the composed-prompt save guard on the instance paths ──────────

#[test]
fn instance_save_guard_rejects_over_cap_composed_prompt() {
    use crate::managed_agents::prompt_skills::MAX_COMPOSED_PROMPT_BYTES;

    // Linked instance: the guard composes the LIVE definition prompt against
    // the resolved (definition or override) policy — an override is not
    // required for the cap to bite.
    let rec = record(Some("d1"), None, None, None);
    let mut def = definition("d1", None, None, &"x".repeat(MAX_COMPOSED_PROMPT_BYTES));
    def.capability_policy = AgentCapabilityPolicy {
        tools: ToolPolicy::HarnessDefault,
        skills: SkillPolicy::Selected {
            selected: vec!["buzz-cli".to_string()],
        },
    };
    let defs = vec![def];
    let err = validate_effective_composed_prompt(&rec, &defs, &global(None, None)).unwrap_err();
    assert!(err.contains("prompt") || err.contains("limit"), "{err}");

    // Same definition policy, prompt that fits → the guard passes.
    let mut def = definition("d1", None, None, "short");
    def.capability_policy = AgentCapabilityPolicy {
        tools: ToolPolicy::HarnessDefault,
        skills: SkillPolicy::Selected {
            selected: vec!["buzz-cli".to_string()],
        },
    };
    let defs = vec![def];
    validate_effective_composed_prompt(&rec, &defs, &global(None, None)).unwrap();

    // Definition-less instance: the record's own prompt + override policy.
    let mut rec = record(None, None, None, None);
    rec.system_prompt = Some("x".repeat(MAX_COMPOSED_PROMPT_BYTES));
    rec.capability_policy_override = Some(AgentCapabilityPolicy {
        tools: ToolPolicy::HarnessDefault,
        skills: SkillPolicy::Selected {
            selected: vec!["buzz-cli".to_string()],
        },
    });
    let err = validate_effective_composed_prompt(&rec, &[], &global(None, None)).unwrap_err();
    assert!(err.contains("prompt") || err.contains("limit"), "{err}");

    // An orphaned link passes the guard — spawn/deploy refuse it elsewhere
    // with the shared orphan error.
    let rec = record(Some("missing"), None, None, None);
    validate_effective_composed_prompt(&rec, &[], &global(None, None)).unwrap();
}

// ── SPEC-005: the create path validates the INHERITED effective policy ──────

#[test]
fn create_path_inherited_policy_is_validated_against_the_prospective_command() {
    // The create-site sequence (agents.rs): resolve the effective policy for
    // the prospective record — override when present, else the linked
    // definition's — then run the same save-time compatibility check the
    // update path runs. A linked create normally sends NO override, so only
    // the resolved definition policy can catch a divergent harness pick.
    let rec = record(Some("d1"), None, None, None); // no override
    let mut def = definition("d1", None, None, "Persona.");
    def.capability_policy = AgentCapabilityPolicy {
        tools: ToolPolicy::HarnessDefault,
        skills: SkillPolicy::Selected {
            selected: vec!["buzz-cli".to_string()],
        },
    };
    let defs = vec![def];

    let resolved = resolve_effective_capability_policy(&rec, &defs, &global(None, None));
    assert_eq!(resolved.source, ConfigSource::Definition);
    // A skills-only inherited policy is honor-able by any built-in command
    // (prompt delivery) — the compatible create proceeds.
    crate::managed_agents::capability_compiler::validate_policy_against_command(
        &resolved.policy,
        "goose",
    )
    .unwrap();
    // …but is rejected for a custom harness command (§11.3).
    let err = crate::managed_agents::capability_compiler::validate_policy_against_command(
        &resolved.policy,
        "my-custom-harness",
    )
    .unwrap_err();
    assert!(err.contains("built-in"), "{err}");

    // An inherited TOOL policy (only storable pre-HC-001 or by hand-edit) is
    // rejected against every v1 command, omp included.
    let mut def = definition("d1", None, None, "Persona.");
    def.capability_policy = policy_with_tools(&[ToolCapabilityId::FilesRead]);
    let defs = vec![def];
    let resolved = resolve_effective_capability_policy(&rec, &defs, &global(None, None));
    assert!(
        crate::managed_agents::capability_compiler::validate_policy_against_command(
            &resolved.policy,
            "omp",
        )
        .is_err()
    );
}
