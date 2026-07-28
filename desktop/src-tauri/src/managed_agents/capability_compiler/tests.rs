use super::*;
use crate::managed_agents::types::ToolCapabilityId as Id;

/// The verified-transport fixture (HC-001: no production runtime ships a
/// verified transport in v1 — omp was downgraded, see
/// `docs/hc001-omp-capability-transport.md`). Compiler semantics —
/// Leading placement, union/dedupe order, the raw-flag conflict guard —
/// are pinned against this fixture so they stay locked for the first
/// runtime that lands launch-test evidence.
fn verified_fixture() -> KnownAcpRuntime {
    KnownAcpRuntime {
        id: "verified-fixture",
        label: "Verified Fixture",
        capability_transport: crate::managed_agents::discovery::VERIFIED_FIXTURE_TRANSPORT,
        ..crate::managed_agents::EMPTY_RUNTIME
    }
}

fn omp() -> Option<&'static KnownAcpRuntime> {
    known_acp_runtime_exact("omp")
}

fn goose() -> Option<&'static KnownAcpRuntime> {
    known_acp_runtime_exact("goose")
}

fn policy(tools: ToolPolicy, skills: SkillPolicy) -> AgentCapabilityPolicy {
    AgentCapabilityPolicy { tools, skills }
}

#[test]
fn default_policy_leaves_args_untouched() {
    let base = vec!["acp".to_string()];
    let fixture = verified_fixture();
    assert_eq!(
        compile_capability_policy(
            Some(&fixture),
            &AgentCapabilityPolicy::default(),
            base.clone()
        )
        .unwrap(),
        base
    );
    // Even on a custom harness (None), a default policy is a no-op.
    assert_eq!(
        compile_capability_policy(None, &AgentCapabilityPolicy::default(), base.clone()).unwrap(),
        base
    );
}

#[test]
fn selected_tools_compile_ahead_of_subcommand() {
    let fixture = verified_fixture();
    let compiled = compile_capability_policy(
        Some(&fixture),
        &policy(
            ToolPolicy::Selected {
                selected: vec![Id::FilesRead, Id::CodeSearch],
            },
            SkillPolicy::Inherit,
        ),
        vec!["acp".to_string()],
    )
    .unwrap();
    assert_eq!(compiled, vec!["--tools=read,grep,glob,ast_grep", "acp"]);
    let subcommand = compiled.iter().position(|a| a == "acp").unwrap();
    let flag = compiled
        .iter()
        .position(|a| a.starts_with("--tools="))
        .unwrap();
    assert!(flag < subcommand, "capability flags must precede `acp`");
}

#[test]
fn selected_tools_dedupe_native_tools_in_mapping_table_order() {
    let fixture = verified_fixture();
    let compiled = compile_capability_policy(
        Some(&fixture),
        &policy(
            ToolPolicy::Selected {
                // grep appears in code.search only, but read/bash span
                // mappings — union must dedupe, order by the table.
                selected: vec![Id::FilesRead, Id::FilesWrite, Id::ShellExecute],
            },
            SkillPolicy::Inherit,
        ),
        vec!["acp".to_string()],
    )
    .unwrap();
    assert_eq!(
        compiled,
        vec!["--tools=read,edit,write,ast_edit,bash,eval", "acp"]
    );
}

#[test]
fn tools_none_compiles_none_flag_first() {
    let fixture = verified_fixture();
    let compiled = compile_capability_policy(
        Some(&fixture),
        &policy(ToolPolicy::None, SkillPolicy::Inherit),
        vec!["acp".to_string()],
    )
    .unwrap();
    assert_eq!(compiled, vec!["--no-tools", "acp"]);
}

#[test]
fn skills_none_and_selected_compile_disable_flag() {
    let fixture = verified_fixture();
    for skills in [
        SkillPolicy::None,
        SkillPolicy::Selected {
            selected: vec!["buzz-cli".to_string()],
        },
    ] {
        let compiled = compile_capability_policy(
            Some(&fixture),
            &policy(ToolPolicy::HarnessDefault, skills),
            vec!["acp".to_string()],
        )
        .unwrap();
        assert_eq!(compiled, vec!["--no-skills", "acp"]);
    }
}

#[test]
fn tools_and_skills_flags_both_precede_subcommand() {
    let fixture = verified_fixture();
    let compiled = compile_capability_policy(
        Some(&fixture),
        &policy(
            ToolPolicy::Selected {
                selected: vec![Id::FilesRead, Id::WebSearch],
            },
            SkillPolicy::None,
        ),
        vec!["acp".to_string()],
    )
    .unwrap();
    assert_eq!(
        compiled,
        vec!["--tools=read,web_search", "--no-skills", "acp"]
    );
}

#[test]
fn unsupported_capability_error_lists_ids() {
    let err = compile_capability_policy(
        goose(),
        &policy(
            ToolPolicy::Selected {
                selected: vec![Id::Browser, Id::ImageInspect],
            },
            SkillPolicy::Inherit,
        ),
        vec!["acp".to_string()],
    )
    .unwrap_err();
    assert!(err.starts_with(UNSUPPORTED_CAPABILITIES_PREFIX), "{err}");
    assert!(err.contains("browser"), "{err}");
    assert!(err.contains("image.inspect"), "{err}");
    let sentence = user_facing_capability_error(&err);
    assert!(
        !sentence.contains(UNSUPPORTED_CAPABILITIES_PREFIX),
        "{sentence}"
    );
    assert!(sentence.contains("unsupported"), "{sentence}");
}

#[test]
fn tools_none_on_harness_managed_runtime_is_an_error() {
    let err = compile_capability_policy(
        goose(),
        &policy(ToolPolicy::None, SkillPolicy::Inherit),
        vec!["acp".to_string()],
    )
    .unwrap_err();
    assert!(err.starts_with(UNSUPPORTED_CAPABILITIES_PREFIX), "{err}");
}

#[test]
fn skills_only_policy_on_harness_managed_runtime_does_not_fail() {
    // No disable flag exists for goose — the UI surfaces the limitation
    // instead; args pass through unchanged.
    let base = vec!["acp".to_string()];
    assert_eq!(
        compile_capability_policy(
            goose(),
            &policy(
                ToolPolicy::HarnessDefault,
                SkillPolicy::Selected {
                    selected: vec!["buzz-cli".to_string()]
                }
            ),
            base.clone()
        )
        .unwrap(),
        base
    );
}

#[test]
fn custom_runtime_with_explicit_policy_is_an_error() {
    let err = compile_capability_policy(
        None,
        &policy(
            ToolPolicy::Selected {
                selected: vec![Id::FilesRead],
            },
            SkillPolicy::Inherit,
        ),
        vec!["serve".to_string()],
    )
    .unwrap_err();
    assert!(err.starts_with(UNSUPPORTED_CAPABILITIES_PREFIX), "{err}");
    assert!(err.contains("files.read"), "{err}");
}

#[test]
fn raw_capability_flag_conflict_is_named() {
    let fixture = verified_fixture();
    for (arg, policy_word) in [
        ("--tools=read", "Tools"),
        ("--no-tools", "Tools"),
        ("--skills=buzz-cli", "Skills"),
        ("--no-skills", "Skills"),
    ] {
        let err = compile_capability_policy(
            Some(&fixture),
            &policy(
                ToolPolicy::Selected {
                    selected: vec![Id::FilesRead],
                },
                SkillPolicy::None,
            ),
            vec![arg.to_string(), "acp".to_string()],
        )
        .unwrap_err();
        assert!(err.contains(arg), "{err}");
        assert!(err.contains(policy_word), "{err}");
        assert!(err.contains("Advanced arguments"), "{err}");
    }
}

#[test]
fn raw_capability_flag_conflict_guard_scopes_to_verified_transports() {
    let policy = policy(
        ToolPolicy::Selected {
            selected: vec![Id::FilesRead],
        },
        SkillPolicy::Inherit,
    );
    let args = vec!["--tools=read".to_string(), "acp".to_string()];
    // Verified fixture: the conflict is named (SPEC-001 save-path guard).
    let fixture = verified_fixture();
    let err = raw_capability_flag_conflict(Some(&fixture), &policy, &args).unwrap_err();
    assert!(err.contains("--tools=read"), "{err}");
    // Harness-managed (omp, post-HC-001) and custom runtimes: raw args
    // are the only capability mechanism — never a conflict.
    assert!(raw_capability_flag_conflict(omp(), &policy, &args).is_ok());
    assert!(raw_capability_flag_conflict(None, &policy, &args).is_ok());
    // A default policy never conflicts.
    assert!(
        raw_capability_flag_conflict(Some(&fixture), &AgentCapabilityPolicy::default(), &args)
            .is_ok()
    );
}

#[test]
fn similar_flag_prefixes_do_not_conflict() {
    let fixture = verified_fixture();
    let compiled = compile_capability_policy(
        Some(&fixture),
        &policy(
            ToolPolicy::Selected {
                selected: vec![Id::FilesRead],
            },
            SkillPolicy::Inherit,
        ),
        vec!["--toolsmith".to_string(), "acp".to_string()],
    )
    .unwrap();
    assert_eq!(compiled, vec!["--tools=read", "--toolsmith", "acp"]);
}

#[test]
fn harness_switch_leaves_stored_policy_bytes_untouched() {
    // The compiler consumes but never mutates the policy — switching
    // runtimes cannot silently rewrite the stored intent.
    let fixture = verified_fixture();
    let stored = policy(
        ToolPolicy::Selected {
            selected: vec![Id::FilesRead],
        },
        SkillPolicy::None,
    );
    let snapshot = stored.clone();
    let _ = compile_capability_policy(Some(&fixture), &stored, vec!["acp".to_string()]).unwrap();
    let _ = compile_capability_policy(goose(), &stored, vec!["acp".to_string()]);
    assert_eq!(stored, snapshot);
}

#[test]
fn omp_tool_policy_is_rejected_harness_managed() {
    // HC-001 downgrade (`docs/hc001-omp-capability-transport.md`): omp's
    // flag surface does not enforce the selected set in `acp` mode, so
    // explicit tool policies are rejected like every other
    // harness-managed runtime — the save-time check agrees.
    let err = compile_capability_policy(
        omp(),
        &policy(
            ToolPolicy::Selected {
                selected: vec![Id::FilesRead],
            },
            SkillPolicy::Inherit,
        ),
        vec!["acp".to_string()],
    )
    .unwrap_err();
    assert!(err.starts_with(UNSUPPORTED_CAPABILITIES_PREFIX), "{err}");
    assert!(err.contains("files.read"), "{err}");
    let err = validate_policy_against_runtime(
        &policy(ToolPolicy::None, SkillPolicy::Inherit),
        Some("omp"),
    )
    .unwrap_err();
    assert!(err.contains("manages its own tool set"), "{err}");
    // Skills still deliver via prompt sections — no flags, no failure.
    let base = vec!["acp".to_string()];
    assert_eq!(
        compile_capability_policy(
            omp(),
            &policy(
                ToolPolicy::HarnessDefault,
                SkillPolicy::Selected {
                    selected: vec!["buzz-cli".to_string()]
                }
            ),
            base.clone()
        )
        .unwrap(),
        base
    );
    assert!(validate_policy_against_runtime(
        &policy(
            ToolPolicy::HarnessDefault,
            SkillPolicy::Selected {
                selected: vec!["buzz-cli".to_string()]
            }
        ),
        Some("omp")
    )
    .is_ok());
}

#[test]
fn save_time_check_names_unsupported_ids() {
    let p = policy(
        ToolPolicy::Selected {
            selected: vec![Id::Browser],
        },
        SkillPolicy::Inherit,
    );
    // v1 has no verified runtime (HC-001): every built-in — omp included —
    // rejects an explicit tool policy, naming the unsupported ids.
    for runtime_id in ["omp", "goose"] {
        let err = validate_policy_against_runtime(&p, Some(runtime_id)).unwrap_err();
        assert!(err.contains("browser"), "{runtime_id}: {err}");
    }
    // No runtime pinned → default (buzz-agent, harness-managed) → Err.
    assert!(validate_policy_against_runtime(&p, None).is_err());
    // Custom harness id → named error.
    let err = validate_policy_against_runtime(&p, Some("my-custom")).unwrap_err();
    assert!(err.contains("built-in"), "{err}");
    // Default policy always passes, everywhere.
    assert!(
        validate_policy_against_runtime(&AgentCapabilityPolicy::default(), Some("my-custom"))
            .is_ok()
    );
    // Skills-only passes on harness-managed runtimes (prompt delivery).
    assert!(validate_policy_against_runtime(
        &policy(
            ToolPolicy::HarnessDefault,
            SkillPolicy::Selected {
                selected: vec!["buzz-cli".to_string()]
            }
        ),
        Some("goose")
    )
    .is_ok());
    // Tools none on harness-managed is rejected.
    assert!(validate_policy_against_runtime(
        &policy(ToolPolicy::None, SkillPolicy::Inherit),
        Some("goose")
    )
    .is_err());
}

// ── SPEC-R2-002: capability transport resolves by runtime identity ────
//
// A custom harness id whose command collides with a builtin command
// ("my-goose" → `goose`, aliases and case/`.exe` normalization included)
// must map to the unmapped arm (§11.3 named error) on every seam — the
// builtin's transport must never be borrowed. Genuine builtin ids keep
// their own rows; raw override pins (no persisted identity) keep the
// pre-existing command-based lookup.

fn record_fixture() -> crate::managed_agents::types::ManagedAgentRecord {
    serde_json::from_str(
        r#"{
            "pubkey": "abcd1234",
            "name": "test-agent",
            "private_key_nsec": "nsec1fake",
            "relay_url": "wss://localhost:3000",
            "acp_command": "buzz-acp",
            "agent_command": "goose",
            "agent_args": [],
            "mcp_command": "",
            "turn_timeout_seconds": 320,
            "system_prompt": null,
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z",
            "last_started_at": null,
            "last_stopped_at": null,
            "last_exit_code": null,
            "last_error": null
        }"#,
    )
    .expect("record fixture")
}

fn persona_fixture(runtime: Option<&str>) -> crate::managed_agents::types::AgentDefinition {
    crate::managed_agents::types::AgentDefinition {
        id: "p1".to_string(),
        display_name: "P1".to_string(),
        avatar_url: None,
        system_prompt: "prompt".to_string(),
        runtime: runtime.map(str::to_string),
        model: None,
        provider: None,
        name_pool: vec![],
        is_builtin: false,
        is_active: true,
        source_team: None,
        source_team_persona_slug: None,
        env_vars: Default::default(),
        respond_to: None,
        respond_to_allowlist: vec![],
        parallelism: None,
        capability_policy: AgentCapabilityPolicy::default(),
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    }
}

/// Install a custom harness whose command collides with the builtin
/// `goose` command; restore the registry (presets only) on scope exit.
fn with_colliding_custom_harness<T>(run: impl FnOnce() -> T) -> T {
    let _lock = crate::managed_agents::custom_harnesses::registry_test_lock();
    crate::managed_agents::custom_harnesses::update_loaded_harness_registry(vec![
        crate::managed_agents::custom_harnesses::HarnessDefinition {
            id: "my-goose".to_string(),
            label: "My Goose".to_string(),
            command: "goose".to_string(),
            args: vec![],
            env: Default::default(),
            install_instructions_url: String::new(),
            install_hint: String::new(),
        },
    ]);
    let out = run();
    crate::managed_agents::custom_harnesses::warm_harness_registry_from_dir(None);
    out
}

#[test]
fn transport_resolves_by_runtime_identity_not_command() {
    with_colliding_custom_harness(|| {
        // Record pinned to the colliding custom id: the resolved command
        // IS the builtin's, but the transport identity is unmapped.
        let mut record = record_fixture();
        record.runtime = Some("my-goose".to_string());
        assert!(capability_transport_runtime(&record, &[], "goose").is_none());

        // The persona-linked form resolves identically.
        record.runtime = None;
        record.persona_id = Some("p1".to_string());
        let personas = vec![persona_fixture(Some("my-goose"))];
        assert!(capability_transport_runtime(&record, &personas, "goose").is_none());

        // Genuine builtin ids keep their own transport row — the exact
        // lookup matches what the command would have found.
        let mut builtin = record_fixture();
        builtin.runtime = Some("goose".to_string());
        assert_eq!(
            capability_transport_runtime(&builtin, &[], "goose").map(|r| r.id),
            Some("goose")
        );

        // A raw override pin has no persisted identity — command-based
        // lookup is kept (legacy pins).
        let mut pinned = record_fixture();
        pinned.runtime = Some("my-goose".to_string());
        pinned.agent_command_override = Some("goose".to_string());
        assert_eq!(
            capability_transport_runtime(&pinned, &[], "goose").map(|r| r.id),
            Some("goose")
        );

        // No identity anywhere (legacy record) — command-based lookup.
        let legacy = record_fixture();
        assert_eq!(
            capability_transport_runtime(&legacy, &[], "goose").map(|r| r.id),
            Some("goose")
        );
    });
}

#[test]
fn custom_id_collision_rejects_skills_policies_at_the_save_guard() {
    with_colliding_custom_harness(|| {
        // Both skills shapes that previously slipped through via the
        // colliding builtin row now get the named §11.3 error — at the
        // guard shared by create and update (`validate_policy_against_record`).
        for skills in [
            SkillPolicy::None,
            SkillPolicy::Selected {
                selected: vec!["buzz-cli".to_string()],
            },
        ] {
            let p = policy(ToolPolicy::HarnessDefault, skills);
            let mut record = record_fixture();
            record.runtime = Some("my-goose".to_string());
            let err = validate_policy_against_record(&p, &record, &[], "goose").unwrap_err();
            assert!(err.contains("custom harnesses"), "record-level: {err}");

            record.runtime = None;
            record.persona_id = Some("p1".to_string());
            let personas = vec![persona_fixture(Some("my-goose"))];
            let err = validate_policy_against_record(&p, &record, &personas, "goose").unwrap_err();
            assert!(err.contains("custom harnesses"), "persona-level: {err}");
        }

        // A default policy passes everywhere (no behavior change for
        // pre-feature records on colliding customs).
        let mut record = record_fixture();
        record.runtime = Some("my-goose".to_string());
        assert!(validate_policy_against_record(
            &AgentCapabilityPolicy::default(),
            &record,
            &[],
            "goose"
        )
        .is_ok());
    });
}

#[test]
fn custom_id_collision_refuses_descriptor_compilation() {
    with_colliding_custom_harness(|| {
        // The descriptor backstop (spawn/hash/deploy) resolves the same
        // identity: skills.none and skills.selected both refuse with the
        // unsupported-capability sentinel instead of compiling against
        // the colliding builtin row.
        for skills in [
            SkillPolicy::None,
            SkillPolicy::Selected {
                selected: vec!["buzz-cli".to_string()],
            },
        ] {
            let mut record = record_fixture();
            record.runtime = Some("my-goose".to_string());
            record.capability_policy_override = Some(policy(ToolPolicy::HarnessDefault, skills));
            let error = crate::managed_agents::resolve_effective_harness_descriptor(
                &record,
                &[],
                &Default::default(),
            )
            .unwrap_err();
            assert!(
                error.starts_with(UNSUPPORTED_CAPABILITIES_PREFIX),
                "{error}"
            );
        }

        // A default policy still resolves byte-identical base args on the
        // colliding custom (pre-feature records unchanged).
        let mut record = record_fixture();
        record.runtime = Some("my-goose".to_string());
        let descriptor = crate::managed_agents::resolve_effective_harness_descriptor(
            &record,
            &[],
            &Default::default(),
        )
        .unwrap();
        assert_eq!(descriptor.command, "goose");
        assert_eq!(descriptor.args, descriptor.base_args);

        // A genuine builtin record with a skills policy is UNCHANGED:
        // skills deliver via the composed prompt, args stay untouched.
        let mut builtin = record_fixture();
        builtin.runtime = Some("goose".to_string());
        builtin.capability_policy_override = Some(policy(
            ToolPolicy::HarnessDefault,
            SkillPolicy::Selected {
                selected: vec!["buzz-cli".to_string()],
            },
        ));
        let descriptor = crate::managed_agents::resolve_effective_harness_descriptor(
            &builtin,
            &[],
            &Default::default(),
        )
        .unwrap();
        assert_eq!(descriptor.args, descriptor.base_args);
    });
}
