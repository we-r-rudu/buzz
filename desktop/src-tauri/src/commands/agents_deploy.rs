//! Provider deploy payload construction, split from `agents.rs` (file-size
//! guard). `build_deploy_payload` gathers live state; `deploy_payload_json`
//! is the pure serialization half so payload completeness stays testable.

use tauri::AppHandle;

#[cfg(test)]
use crate::managed_agents::AgentDefinition;
use crate::{
    app_state::AppState,
    managed_agents::{
        discover_provider_candidates, load_managed_agents, load_personas, provider_deploy,
        resolve_provider_binary, save_managed_agents, ManagedAgentRecord,
    },
    relay::relay_ws_url_with_override,
    util::now_iso,
};

/// Deploy an agent to a provider backend. Resolves the binary, calls deploy via
/// spawn_blocking, and persists the result (backend_agent_id or last_error).
///
/// Idempotency: calling deploy on an already-deployed agent sends the same payload
/// again. Providers are expected to handle this as an update-in-place or no-op —
/// the protocol does not include an explicit `undeploy` operation (deferred to v2).
///
/// Returns Ok(()) on success, Err(message) on failure. Either way the record is
/// updated and saved before returning.
pub(crate) async fn deploy_to_provider(
    app: &AppHandle,
    state: &AppState,
    pubkey: &str,
    provider_id: &str,
    config: &serde_json::Value,
    agent_json: serde_json::Value,
    cached_binary_path: Option<&str>,
) -> Result<(), String> {
    // Resolve via discovered candidates only. Cached path must match BOTH
    // "is a discovered candidate" AND "belongs to this provider_id". A tampered
    // record cannot redirect deploys to a different provider's binary.
    let bin_path = cached_binary_path
        .map(std::path::PathBuf::from)
        .filter(|p| p.exists())
        .map(|p| p.canonicalize().unwrap_or(p))
        .filter(|canonical| {
            discover_provider_candidates().iter().any(|(id, cp)| {
                id == provider_id && cp.canonicalize().ok().as_ref() == Some(canonical)
            })
        })
        .map_or_else(|| resolve_provider_binary(provider_id), Ok)?;

    let config_clone = config.clone();
    let deploy_result =
        tokio::task::spawn_blocking(move || provider_deploy(&bin_path, &agent_json, &config_clone))
            .await
            .map_err(|e| format!("spawn_blocking failed: {e}"))?;

    // Persist result under lock.
    let _store_guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|e| e.to_string())?;
    let mut records = load_managed_agents(app)?;
    let rec = records
        .iter_mut()
        .find(|r| r.pubkey == pubkey)
        .ok_or_else(|| format!("agent {pubkey} not found"))?;

    match deploy_result {
        Ok(backend_agent_id) => {
            rec.backend_agent_id = Some(backend_agent_id);
            rec.last_started_at = Some(now_iso());
            rec.updated_at = now_iso();
            rec.last_error = None;
        }
        Err(ref e) => {
            rec.last_error = Some(e.clone());
            rec.updated_at = now_iso();
            save_managed_agents(app, &records)?;
            return Err(e.clone());
        }
    }
    save_managed_agents(app, &records)?;
    Ok(())
}

/// Resolve the deploy-specific structured model/provider for a managed agent.
///
/// Delegates to the single effective-config resolver which enforces
/// definition-authoritative semantics for linked instances:
///   - **Linked:** definition → global. Stale record bytes are never consulted.
///   - **Definition-less:** instance → global.
///   - **Orphaned:** returns `(None, None)` — spawn is blocked elsewhere.
///
/// Both local spawn and deploy now use the same resolver, so they can never
/// disagree on what model/provider an agent runs with.
///
/// Exported `pub(crate)` for unit testing.
#[cfg(test)]
pub(crate) fn resolve_deploy_model_provider(
    record: &ManagedAgentRecord,
    personas: &[AgentDefinition],
    global: &crate::managed_agents::GlobalAgentConfig,
) -> (Option<String>, Option<String>) {
    crate::managed_agents::effective_config::resolve_effective_model_provider_pair(
        record, personas, global,
    )
    .unwrap_or((None, None))
}

/// §7 provider release gate (SPEC-007, extended to the EFFECTIVE policy by
/// round2-general-003): capability policies stay desktop-local until the
/// external provider script's lossless args transport is verified — the UI
/// hides the sections and the dialog gates the draft, but the effective
/// policy resolves server-side, so a definition-level policy added after
/// creation would otherwise ride redeploy-on-edit inheritance to the VPS.
/// The gate is deliberately ALL-POLICY (tools AND skills): narrowing it to
/// tools-only would declare skills provider-safe without spec authority.
pub(crate) const PROVIDER_CAPABILITY_GATE_MESSAGE: &str =
    "tool and skill policies aren't available for provider-backed agents yet";

/// Pure half of the release gate (unit-testable without an `AppHandle`):
/// `Some(error)` when the EFFECTIVE policy is non-default. The effective
/// policy already folds the instance override over the linked definition
/// (`resolve_effective_capability_policy`), so both the override and the
/// inherited cases funnel through the one check.
pub(crate) fn provider_capability_gate_error(
    policy: &crate::managed_agents::AgentCapabilityPolicy,
) -> Option<String> {
    if policy.is_default() {
        return None;
    }
    Some(format!(
        "{PROVIDER_CAPABILITY_GATE_MESSAGE} — remove the tool/skill policy from the linked definition (or clear the instance override) before deploying"
    ))
}

/// Build the standard agent JSON payload for provider deploy calls.
///
/// Like local spawn, provider deploy re-reads live persona env vars and
/// structured model/provider so remote agents receive current credentials
/// and the same authoritative values that local spawn derives from
/// `runtime_metadata_env_vars`. `agent_command`/`agent_args` come from the
/// SAME `resolve_effective_harness_descriptor` local spawn uses — including
/// compiled capability-policy flags — so local and provider launches cannot
/// diverge (HC-006); the provider contract keeps the same payload keys and
/// args already travel as a lossless JSON array there. The only remaining
/// read-time resolution is `relay_url`: a blank pin resolves to the active
/// workspace relay here, matching the create-path contract.
///
/// Dangling-harness and unsupported-capability-policy errors propagate as
/// deploy failures (visible, never a silent fallback).
///
/// Fails closed when the private key is unavailable (keyring outage leaves
/// it empty after hydration): without this guard a provider deploy would
/// serialize `"private_key_nsec": ""` and launch the agent with no
/// identity — the same hazard the local spawn path refuses via
/// `spawn_key_refusal`.
pub(crate) fn build_deploy_payload(
    app: &AppHandle,
    state: &AppState,
    record: &ManagedAgentRecord,
) -> Result<serde_json::Value, String> {
    // Fails closed when the private key is unavailable — same guard as local
    // spawn. Without this, a keyring outage would serialize `"private_key_nsec": ""`
    // and launch the agent with no identity.
    if let Some(err) = crate::managed_agents::spawn_key_refusal(record) {
        return Err(err);
    }

    // Merge global + persona + agent env_vars for provider deploy — the same
    // live-persona-under-overrides semantics as local spawn. Global env vars
    // are the lowest user-settable layer: global < persona < agent (last-wins
    // on key collision). Without this, provider-backed agents wouldn't receive
    // credentials saved on the persona or the agent itself. Kept AS IS
    // deliberately: substituting `descriptor.env` would double-inject
    // runtime-metadata keys the provider payload projects separately from
    // structured fields.
    let global_config = crate::managed_agents::load_global_agent_config(app).unwrap_or_default();
    let global_env = global_config.env_vars.clone();
    let persona_env =
        crate::managed_agents::resolve_persona_env(app, record.persona_id.as_deref())?;
    // Merge: global < persona (persona wins over global).
    let global_persona_merged = crate::managed_agents::merged_user_env(&global_env, &persona_env);
    // Merge: global+persona < agent (agent wins over everything).
    let merged_env =
        crate::managed_agents::merged_user_env(&global_persona_merged, &record.env_vars);

    let personas = load_personas(app).unwrap_or_default();
    let cfg = crate::managed_agents::effective_config::resolve_effective_config(
        record,
        &personas,
        &global_config,
    )
    .require_resolved()?;
    // The release gate reads the EFFECTIVE policy from the same resolved
    // config the payload serializes — a non-default policy (instance
    // override OR inherited from the definition) refuses the deploy visibly
    // at every call site (create, redeploy-on-edit, provider start); the
    // redeploy path stamps "Redeploy required: …" per HC-005. Default-policy
    // records are unaffected.
    if let Some(gate_error) = provider_capability_gate_error(&cfg.capability_policy) {
        return Err(gate_error);
    }
    let effective_model = cfg.model.value;
    let effective_provider = cfg.provider.value;
    let effective_prompt = cfg.system_prompt.value;

    // The same typed descriptor local spawn consumes: persona/pin-resolved
    // command, normalized args with compiled capability-policy flags, and a
    // typed Err for dangling harnesses or unhonorable policies.
    let descriptor = crate::managed_agents::resolve_effective_harness_descriptor(
        record,
        &personas,
        &global_config,
    )
    .map_err(|e| {
        crate::managed_agents::capability_compiler::user_facing_capability_error(
            &crate::managed_agents::user_facing_harness_error(&e),
        )
    })?;

    Ok(deploy_payload_json(
        record,
        crate::relay::effective_agent_relay_url(
            &record.relay_url,
            &relay_ws_url_with_override(state),
        ),
        effective_model,
        effective_provider,
        effective_prompt,
        merged_env,
        &descriptor,
    ))
}

/// Pure serialization half of [`build_deploy_payload`] — every field the
/// provider harness receives is deliberately listed here, so payload
/// completeness is testable without an `AppHandle`. From the descriptor only
/// the resolved `command` and compiled `args` are serialized (the provider
/// contract keys); `base_args`/`env` never ride the provider payload — env
/// travels via `merged_env` instead.
pub(super) fn deploy_payload_json(
    record: &ManagedAgentRecord,
    relay_url: String,
    effective_model: Option<String>,
    effective_provider: Option<String>,
    effective_prompt: Option<String>,
    merged_env: std::collections::BTreeMap<String, String>,
    descriptor: &crate::managed_agents::readiness::EffectiveHarnessDescriptor,
) -> serde_json::Value {
    serde_json::json!({
        "name": &record.name,
        "relay_url": relay_url,
        "private_key_nsec": &record.private_key_nsec,
        "auth_tag": &record.auth_tag,
        "agent_command": &descriptor.command,
        "agent_args": &descriptor.args,
        "system_prompt": effective_prompt,
        "model": effective_model,
        "provider": effective_provider,
        "turn_timeout_seconds": record.turn_timeout_seconds,
        "idle_timeout_seconds": record.idle_timeout_seconds,
        "max_turn_duration_seconds": record.max_turn_duration_seconds,
        "parallelism": record.parallelism,
        "respond_to": record.respond_to,
        "respond_to_allowlist": &record.respond_to_allowlist,
        "env_vars": merged_env,
    })
}
