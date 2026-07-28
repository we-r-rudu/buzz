//! Buzz prompt-skill catalog IPC, split from `personas/mod.rs` (file-size
//! guard). The catalog itself lives in `managed_agents::prompt_skills`.

/// Static Buzz prompt-skill catalog for the capability-policy pickers.
/// Prompt text is deliberately NOT sent to the frontend — ids, labels, and
/// descriptions only; the prompt bodies are composed server-side at resolve
/// time (`effective_config`).
#[tauri::command]
pub fn list_buzz_prompt_skills() -> Vec<crate::managed_agents::prompt_skills::BuzzPromptSkillInfo> {
    crate::managed_agents::prompt_skills::buzz_prompt_skill_infos()
}
