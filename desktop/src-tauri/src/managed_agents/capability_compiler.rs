//! Capability-policy compiler: the single pure function that turns a
//! resolved harness-neutral [`AgentCapabilityPolicy`] into harness-specific
//! CLI flags, plus the save-time compatibility check.
//!
//! Wired into `resolve_effective_harness_descriptor` (readiness.rs) AFTER base
//! args resolution, so spawn, summary, hash, model probes, and readiness all
//! inherit the behavior from one call site.
//!
//! Hard rules (plan §0):
//! - Flags are spliced at index 0 of the final vector (Leading placement):
//!   omp's global flags must precede the `acp` subcommand token — anything
//!   after it is silently ignored by omp's lenient parse (false capability
//!   parity). Pinned by tests asserting position relative to the subcommand.
//! - Compiler output is comma-bearing (`--tools=read,grep`) by construction.
//!   It is injected at the descriptor layer, post-validation, so it never
//!   passes through `validate_harness_definition`'s comma rejection — that
//!   rejection stays in force for raw user-entered args. Lossless delivery
//!   rides `BUZZ_ACP_AGENT_ARGS_JSON` (Phase B1).

use std::collections::BTreeSet;

use super::discovery::{
    known_acp_runtime, known_acp_runtime_exact, CapabilityFlagPlacement, KnownAcpRuntime,
};
use super::types::{AgentCapabilityPolicy, SkillPolicy, ToolCapabilityId, ToolPolicy};

/// Typed sentinel prefix for the unsupported-capability descriptor error.
/// Internal Rust contract, like `DANGLING_HARNESS_PREFIX`: user-facing
/// surfaces convert it to a sentence via [`user_facing_capability_error`],
/// never show it raw.
pub(crate) const UNSUPPORTED_CAPABILITIES_PREFIX: &str = "UNSUPPORTED_CAPABILITIES:";

/// Compile `policy` into CLI flags prepended (Leading) to `base_args`.
///
/// - `HarnessDefault`/`Inherit` → `base_args` unchanged.
/// - `ToolPolicy::None` → prepend the runtime's none-flag; a none policy on a
///   harness-managed runtime degrades to `Err(UNSUPPORTED_CAPABILITIES:…)`
///   listing the semantic group (it cannot be honored).
/// - `ToolPolicy::Selected` → every selected capability must have a mapping,
///   else `Err(UNSUPPORTED_CAPABILITIES:<ids>)`; else prepend
///   `--tools=<union of native tools, deduped, first-seen mapping-table order>`.
/// - `SkillPolicy::None`/`Selected` → prepend the skills-disable flag when the
///   runtime has one; when it doesn't, skills still deliver via prompt
///   sections (Selected) / no sections (None) and the UI surfaces the
///   ambient-skill limitation — do NOT fail.
/// - `runtime: None` (custom harness) with any explicit policy → `Err`:
///   custom raw args are the only capability mechanism there (plan C.4/§11.3).
/// - Raw-args conflict guard: on a known runtime with a verified transport
///   and a non-default policy, base args carrying `--tools`/`--no-tools`/
///   `--skills`/`--no-skills` tokens are rejected with the conflicting
///   argument named. Custom runtimes never get this guard.
pub(crate) fn compile_capability_policy(
    runtime: Option<&KnownAcpRuntime>,
    policy: &AgentCapabilityPolicy,
    base_args: Vec<String>,
) -> Result<Vec<String>, String> {
    if policy.is_default() {
        return Ok(base_args);
    }

    let Some(runtime) = runtime else {
        return Err(format!(
            "{UNSUPPORTED_CAPABILITIES_PREFIX}{}",
            unsupported_ids_for_policy(policy)
        ));
    };
    let transport = &runtime.capability_transport;

    raw_capability_flag_conflict(Some(runtime), policy, &base_args)?;

    let mut flags: Vec<String> = Vec::new();

    match &policy.tools {
        ToolPolicy::HarnessDefault => {}
        ToolPolicy::None => match transport.tools_none_flag {
            Some(flag) => flags.push(flag.to_string()),
            None => {
                return Err(format!(
                    "{UNSUPPORTED_CAPABILITIES_PREFIX}{}",
                    ToolCapabilityId::ALL
                        .iter()
                        .map(|id| id.as_str())
                        .collect::<Vec<_>>()
                        .join(",")
                ));
            }
        },
        ToolPolicy::Selected { selected } => {
            let missing: Vec<&str> = selected
                .iter()
                .filter(|id| {
                    !transport
                        .tool_mappings
                        .iter()
                        .any(|mapping| mapping.capability == **id)
                })
                .map(|id| id.as_str())
                .collect();
            if !missing.is_empty() {
                return Err(format!(
                    "{UNSUPPORTED_CAPABILITIES_PREFIX}{}",
                    missing.join(",")
                ));
            }
            let Some(select_flag) = transport.tools_select_flag else {
                return Err(format!(
                    "{UNSUPPORTED_CAPABILITIES_PREFIX}{}",
                    selected
                        .iter()
                        .map(|id| id.as_str())
                        .collect::<Vec<_>>()
                        .join(",")
                ));
            };
            // Union of native tools in mapping-table order, deduped first-seen.
            let mut seen = BTreeSet::new();
            let mut native: Vec<&str> = Vec::new();
            for mapping in transport.tool_mappings {
                if selected.contains(&mapping.capability) {
                    for tool in mapping.native_tools {
                        if seen.insert(*tool) {
                            native.push(tool);
                        }
                    }
                }
            }
            flags.push(format!("{select_flag}={}", native.join(",")));
        }
    }

    match &policy.skills {
        SkillPolicy::Inherit => {}
        SkillPolicy::None | SkillPolicy::Selected { .. } => {
            if let Some(flag) = transport.skills_disable_flag {
                flags.push(flag.to_string());
            }
            // No disable flag: do NOT fail — the policy's core content is
            // honored via the composed prompt; the UI shows the
            // ambient-skill limitation instead.
        }
    }

    if flags.is_empty() {
        return Ok(base_args);
    }
    // Splice per the runtime's declared placement. v1 has only Leading:
    // capability flags precede the subcommand token (omp's lenient parse
    // silently ignores argv after `acp` — false capability parity, §0.2).
    match transport.flag_placement {
        CapabilityFlagPlacement::Leading => {
            flags.extend(base_args);
            Ok(flags)
        }
    }
}

/// `--flag` or `--flag=value` token match (no prefix false-positives like
/// `--toolsmith`).
fn is_flag_token(arg: &str, flag: &str) -> bool {
    arg == flag || arg.starts_with(&format!("{flag}="))
}

/// Raw-args conflict guard: on a known runtime with a verified transport and
/// a non-default policy, args carrying `--tools`/`--no-tools`/`--skills`/
/// `--no-skills` tokens are rejected with the conflicting argument named — a
/// structured policy would otherwise silently override the raw flags. Custom
/// runtimes never get this guard (their raw args are the only mechanism).
///
/// Shared by the descriptor seam ([`compile_capability_policy`]) and the
/// save path (`update_managed_agent`), so a hand-typed capability flag fails
/// at save, not at the next spawn (SPEC-001 defense in depth). Inert while
/// no runtime ships a verified transport (post-HC-001 v1) — it arms again
/// with the first launch-tested transport.
pub(crate) fn raw_capability_flag_conflict(
    runtime: Option<&KnownAcpRuntime>,
    policy: &AgentCapabilityPolicy,
    args: &[String],
) -> Result<(), String> {
    if policy.is_default() {
        return Ok(());
    }
    let Some(runtime) = runtime else {
        return Ok(());
    };
    if !runtime.has_verified_capability_transport() {
        return Ok(());
    }
    for arg in args {
        if is_flag_token(arg, "--tools") || is_flag_token(arg, "--no-tools") {
            return Err(format!(
                "argument \"{arg}\" conflicts with the structured Tools policy; remove it from Advanced arguments"
            ));
        }
        if is_flag_token(arg, "--skills") || is_flag_token(arg, "--no-skills") {
            return Err(format!(
                "argument \"{arg}\" conflicts with the structured Skills policy; remove it from Advanced arguments"
            ));
        }
    }
    Ok(())
}

/// The semantic ids a policy asks for, for the unsupported error: selected
/// tool ids when present, otherwise every id (a tools-none or skills-only
/// policy on a harness that cannot express it).
fn unsupported_ids_for_policy(policy: &AgentCapabilityPolicy) -> String {
    match &policy.tools {
        ToolPolicy::Selected { selected } if !selected.is_empty() => selected
            .iter()
            .map(|id| id.as_str())
            .collect::<Vec<_>>()
            .join(","),
        _ => ToolCapabilityId::ALL
            .iter()
            .map(|id| id.as_str())
            .collect::<Vec<_>>()
            .join(","),
    }
}

/// Convert a descriptor capability error to a user-facing sentence, beside
/// `user_facing_harness_error` (discovery.rs). Non-sentinel errors pass
/// through unchanged, so the two compose.
pub(crate) fn user_facing_capability_error(error: &str) -> String {
    match error.strip_prefix(UNSUPPORTED_CAPABILITIES_PREFIX) {
        Some(ids) => format!(
            "this harness cannot enforce the selected tool policy (unsupported: {ids}) — pick different tools or use Harness defaults"
        ),
        None => error.to_string(),
    }
}

/// Save-time compatibility check (HC-003 "blocked at save"): resolve the
/// prospective runtime and reject policies it cannot honor, naming the
/// unsupported semantic ids. Spawn's descriptor error is the backstop.
///
/// `prospective_runtime_id: None` means "no runtime pinned" — the default
/// command (buzz-agent) applies. A `Some(id)` that resolves to no builtin is
/// a custom/preset harness: explicit policy is rejected with a named error
/// (§11.3).
pub(crate) fn validate_policy_against_runtime(
    policy: &AgentCapabilityPolicy,
    prospective_runtime_id: Option<&str>,
) -> Result<(), String> {
    let runtime = match prospective_runtime_id {
        Some(id) => known_acp_runtime_exact(id),
        None => known_acp_runtime(&crate::managed_agents::default_agent_command()),
    };
    validate_policy_against_known_runtime(policy, runtime)
}

/// Agent-side variant: the prospective effective command is already resolved
/// (persona/pin/default), so match it directly. Unknown commands (custom
/// harness binaries) reject explicit policy. Test-only since SPEC-R2-002
/// moved the save guards to identity-based [`validate_policy_against_record`]
/// — kept for the command-resolution test seams.
#[cfg(test)]
pub(crate) fn validate_policy_against_command(
    policy: &AgentCapabilityPolicy,
    effective_command: &str,
) -> Result<(), String> {
    validate_policy_against_known_runtime(policy, known_acp_runtime(effective_command))
}

/// Resolve the runtime whose capability TRANSPORT governs this record — by
/// runtime IDENTITY, not by the resolved command (SPEC-R2-002).
///
/// Precedence mirrors command resolution (`try_record_agent_command`):
/// 1. An explicit raw-command override pin has no runtime identity — only
///    the command string was persisted — so pins keep the pre-existing
///    command-based lookup.
/// 2. `record.runtime` — a builtin id maps to its row; a preset/custom id
///    maps to NO known runtime (the unmapped arm: any explicit policy is
///    rejected, §11.3) even when the harness's command collides with a
///    builtin command (aliases and case/`.exe` normalization included).
/// 3. The linked persona's `runtime` — same rule.
/// 4. No runtime identity anywhere (legacy records) — the pre-existing
///    command-based lookup.
///
/// Scope: ONLY the capability transport. Command-based env/mcp/args metadata
/// for command-colliding customs is pre-existing intended behavior and keeps
/// resolving through `known_acp_runtime(effective_command)` unchanged.
pub(crate) fn capability_transport_runtime(
    record: &crate::managed_agents::types::ManagedAgentRecord,
    personas: &[crate::managed_agents::types::AgentDefinition],
    effective_command: &str,
) -> Option<&'static KnownAcpRuntime> {
    let has_pin = record
        .agent_command_override
        .as_deref()
        .is_some_and(|pin| !pin.trim().is_empty());
    if !has_pin {
        if let Some(id) = record.runtime.as_deref() {
            return known_acp_runtime_exact(id);
        }
        if let Some(persona) = record
            .persona_id
            .as_deref()
            .and_then(|pid| personas.iter().find(|p| p.id == pid))
        {
            if let Some(id) = persona.runtime.as_deref() {
                return known_acp_runtime_exact(id);
            }
        }
    }
    known_acp_runtime(effective_command)
}

/// Managed-agent save guard (SPEC-R2-002): validate against the capability
/// transport resolved by runtime identity ([`capability_transport_runtime`]),
/// so a custom harness id whose command collides with a builtin is rejected
/// with the named custom-harness error instead of borrowing the builtin's
/// transport. `effective_command` is the already-resolved prospective
/// command, consulted only when no runtime identity exists.
pub(crate) fn validate_policy_against_record(
    policy: &AgentCapabilityPolicy,
    record: &crate::managed_agents::types::ManagedAgentRecord,
    personas: &[crate::managed_agents::types::AgentDefinition],
    effective_command: &str,
) -> Result<(), String> {
    validate_policy_against_known_runtime(
        policy,
        capability_transport_runtime(record, personas, effective_command),
    )
}

fn validate_policy_against_known_runtime(
    policy: &AgentCapabilityPolicy,
    runtime: Option<&KnownAcpRuntime>,
) -> Result<(), String> {
    if policy.is_default() {
        return Ok(());
    }
    let Some(runtime) = runtime else {
        return Err(
            "capability policies need one of the built-in runtimes — custom harnesses receive \
             tool/skill flags through their own argument list"
                .to_string(),
        );
    };
    let transport = &runtime.capability_transport;
    match &policy.tools {
        ToolPolicy::HarnessDefault => {}
        ToolPolicy::None => {
            if transport.tools_none_flag.is_none() {
                return Err(format!(
                    "{} manages its own tool set and cannot disable tools — use Harness defaults",
                    runtime.label
                ));
            }
        }
        ToolPolicy::Selected { selected } => {
            let missing: Vec<&str> = selected
                .iter()
                .filter(|id| {
                    !transport
                        .tool_mappings
                        .iter()
                        .any(|mapping| mapping.capability == **id)
                })
                .map(|id| id.as_str())
                .collect();
            if !missing.is_empty() {
                return Err(format!(
                    "{} does not support: {}",
                    runtime.label,
                    missing.join(", ")
                ));
            }
        }
    }
    // Skills always pass — delivered via prompt sections on every runtime.
    Ok(())
}

#[cfg(test)]
#[path = "capability_compiler/tests.rs"]
mod tests;
