#!/bin/bash
# Deterministic grant/revoke tool for the BUZZ_ACP_RESPOND_TO hot-reload gate.
# Runs ON the VPS (env files at /etc/buzz-agents/<name>.env).
#
# Usage:
#   buzz-allowlist-grant.sh grant  <agent|all> <csv-pubkeys>
#   buzz-allowlist-grant.sh revoke <agent|all> <csv-pubkeys>
#   buzz-allowlist-grant.sh show   <agent|all>
#
# grant  — merge pubkeys into each target's allowlist (union), set mode=allowlist.
# revoke — remove pubkeys from each target's allowlist; if it becomes empty,
#          strip both gate lines entirely (back to owner-only default).
# show   — per-agent mode + allowlist count + truncated pubkeys (first 8 chars).
#          NEVER prints any other env content (private keys stay private).
#
# Safety:
#   - Every pubkey is validated ^[0-9a-f]{64}$ (lowercase-normalized, deduped);
#     any invalid token aborts the whole run BEFORE any file is touched.
#   - Every modified file is backed up to <file>.bak-<timestamp> first.
#   - No systemctl calls: the hot-reload binary picks changes up live.
set -euo pipefail

ENV_DIR="/etc/buzz-agents"
MODE_KEY="BUZZ_ACP_RESPOND_TO"
LIST_KEY="BUZZ_ACP_RESPOND_TO_ALLOWLIST"

die() { echo "error: $*" >&2; exit 1; }

usage() {
  cat >&2 <<'EOF'
usage:
  buzz-allowlist-grant.sh grant  <agent|all> <csv-pubkeys>
  buzz-allowlist-grant.sh revoke <agent|all> <csv-pubkeys>
  buzz-allowlist-grant.sh show   <agent|all>
EOF
  exit 1
}

# --- target resolution -------------------------------------------------------

resolve_targets() {
  local target="$1"
  if [[ "$target" == "all" ]]; then
    local f
    shopt -s nullglob
    for f in "$ENV_DIR"/*.env; do
      basename "$f" .env
    done
    shopt -u nullglob
  else
    [[ "$target" =~ ^[a-z0-9][a-z0-9-]*$ ]] \
      || die "invalid agent name '$target' (must match ^[a-z0-9][a-z0-9-]*\$)"
    [[ -f "$ENV_DIR/$target.env" ]] \
      || die "no env file for agent '$target' ($ENV_DIR/$target.env)"
    echo "$target"
  fi
}

# --- pubkey validation -------------------------------------------------------

# Normalizes (lowercase), validates, and dedupes a CSV of pubkeys.
# Prints one pubkey per line. Aborts the run on any invalid token.
parse_pubkeys() {
  local csv="$1"
  local normalized
  normalized=$(printf '%s' "$csv" | tr 'A-F' 'a-f')
  [[ -n "$csv" ]] || die "empty pubkey list"
  local normalized
  normalized=$(printf '%s' "$csv" | tr 'A-F' 'a-f')
  local -a tokens=()
  IFS=',' read -ra tokens <<< "$normalized"
  local tok
  for tok in "${tokens[@]}"; do
    [[ "$tok" =~ ^[0-9a-f]{64}$ ]] \
      || die "invalid pubkey '$tok' (must match ^[0-9a-f]{64}\$) — aborting, no files touched"
  done
  printf '%s\n' "${tokens[@]}" | awk '!seen[$0]++'
}

# --- env-file primitives -----------------------------------------------------

current_mode() { # <file> -> mode value (quotes stripped) or empty
  local v
  v=$(grep "^${MODE_KEY}=" "$1" 2>/dev/null | tail -1 | cut -d= -f2-) || true
  v="${v#\'}"; v="${v%\'}"; v="${v#\"}"; v="${v%\"}"
  printf '%s' "$v"
}

current_allowlist() { # <file> -> one pubkey per line (normalized, deduped)
  local raw
  raw=$(grep "^${LIST_KEY}=" "$1" 2>/dev/null | tail -1 | cut -d= -f2- || true)
  [[ -n "$raw" ]] || return 0
  printf '%s' "$raw" | tr 'A-F' 'a-f' | tr ',' '\n' | awk 'NF && !seen[$0]++'
}

# strip_gate_lines <file>: remove both gate keys, ensure trailing newline.
strip_gate_lines() {
  local f="$1"
  sed -i -e "/^${MODE_KEY}=/d" -e "/^${LIST_KEY}=/d" "$f"
  if [[ -n "$(tail -c1 "$f")" ]]; then
    printf '\n' >> "$f"
  fi
}

# apply_state <file> <mode> <allowlist-csv|empty>
# Backs up, strips gate lines, and appends the desired state.
# Empty allowlist => both gate lines stay stripped (owner-only default).
apply_state() {
  local f="$1" mode="$2" csv="$3"
  local ts
  ts=$(date +%Y%m%d%H%M%S)
  cp "$f" "$f.bak-$ts"
  strip_gate_lines "$f"
  if [[ -n "$csv" ]]; then
    printf '%s=%s\n%s=%s\n' "$MODE_KEY" "$mode" "$LIST_KEY" "$csv" >> "$f"
  fi
}

# csv_join: stdin (one item per line) -> single CSV line on stdout
csv_join() { paste -sd, -; }

# --- commands ----------------------------------------------------------------

# Captures stdout of a helper that may `die`; propagates the failure so a
# die inside the helper aborts the whole run (process substitution would
# swallow it in a subshell).
capture() { # <varname> <cmd...>
  local __var="$1"; shift
  local __out
  __out=$("$@") || exit 1
  printf -v "$__var" '%s' "$__out"
}

cmd_grant() {
  local target="$1" csv="$2"
  local parsed agents_out
  capture parsed parse_pubkeys "$csv"
  capture agents_out resolve_targets "$target"
  local -a new_keys=() agents=()
  mapfile -t new_keys <<< "$parsed"
  mapfile -t agents <<< "$agents_out"
  [[ ${#agents[@]} -gt 0 ]] || die "no target env files found in $ENV_DIR"

  local agent f merged
  for agent in "${agents[@]}"; do
    f="$ENV_DIR/$agent.env"
    merged=$( { current_allowlist "$f"; printf '%s\n' "${new_keys[@]}"; } \
              | awk 'NF && !seen[$0]++' | csv_join )
    apply_state "$f" "allowlist" "$merged"
    echo "grant: $agent -> allowlist=$(awk -F, '{print NF}' <<< "$merged") key(s)"
  done
}

cmd_revoke() {
  local target="$1" csv="$2"
  local parsed agents_out
  capture parsed parse_pubkeys "$csv"
  capture agents_out resolve_targets "$target"
  local -a rev_keys=() agents=()
  mapfile -t rev_keys <<< "$parsed"
  mapfile -t agents <<< "$agents_out"
  [[ ${#agents[@]} -gt 0 ]] || die "no target env files found in $ENV_DIR"

  local revoke_set=" ${rev_keys[*]} "
  local agent f mode key remaining changed cur
  for agent in "${agents[@]}"; do
    f="$ENV_DIR/$agent.env"
    mode=$(current_mode "$f")
    remaining=""
    changed=0
    while IFS= read -r key; do
      [[ -n "$key" ]] || continue
      if [[ "$revoke_set" == *" $key "* ]]; then
        changed=1
      else
        remaining+="$key"$'\n'
      fi
    done < <(current_allowlist "$f")
    cur=$(current_allowlist "$f" | csv_join)
    remaining=$(printf '%s' "$remaining" | awk 'NF' | csv_join)

    if [[ "$changed" -eq 0 && -z "$mode" ]]; then
      echo "revoke: $agent -> unchanged (no gate lines, none of the keys present)"
      continue
    fi
    if [[ "$remaining" == "$cur" && "$changed" -eq 0 ]]; then
      echo "revoke: $agent -> unchanged (none of the keys present)"
      continue
    fi
    if [[ -n "$remaining" ]]; then
      apply_state "$f" "${mode:-allowlist}" "$remaining"
      echo "revoke: $agent -> allowlist=$(awk -F, '{print NF}' <<< "$remaining") key(s) remaining"
    else
      apply_state "$f" "" ""
      echo "revoke: $agent -> allowlist empty, gate lines stripped (owner-only default)"
    fi
  done
}

cmd_show() {
  local target="$1"
  local agents_out
  capture agents_out resolve_targets "$target"
  local -a agents=()
  mapfile -t agents <<< "$agents_out"
  [[ ${#agents[@]} -gt 0 ]] || die "no target env files found in $ENV_DIR"

  local agent f mode keys count
  for agent in "${agents[@]}"; do
    f="$ENV_DIR/$agent.env"
    mode=$(current_mode "$f")
    keys=$(current_allowlist "$f" | cut -c1-8 | paste -sd' ' -)
    count=$(current_allowlist "$f" | awk 'NF' | wc -l | tr -d ' ')
    if [[ -z "$mode" ]]; then
      printf '%-12s mode=%s\n' "$agent" "owner-only (default)"
    else
      printf '%-12s mode=%s allowlist=%s key(s): %s\n' \
        "$agent" "$mode" "$count" "${keys:- (none)}"
    fi
  done
}

# --- entrypoint --------------------------------------------------------------

[[ $# -ge 2 ]] || usage
cmd="$1"; target="$2"
case "$cmd" in
  grant|revoke)
    [[ $# -eq 3 ]] || usage
    "cmd_$cmd" "$target" "$3"
    ;;
  show)
    [[ $# -eq 2 ]] || usage
    cmd_show "$target"
    ;;
  *)
    usage
    ;;
esac
