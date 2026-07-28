#!/bin/bash
# fleet-watchdog — capacity + liveness monitor for the buzz-acp agent fleet.
#
# Runs every 5 min via fleet-watchdog.timer on the VPS. Checks:
#   1. every buzz-agent@* unit is active
#   2. MemAvailable against WARN/CRIT thresholds
#   3. per-agent process-group RSS against AGENT_RSS_WARN_MB
# Appends one CSV sample per run for trend analysis (the VPS resize signal).
# Alerts go to a Buzz ops channel via buzz-cli, only on state change, with a
# per-alert cooldown. A daily digest carries 24h peaks so silence is checkable.
#
# Config: /etc/fleet-watchdog.env (BUZZ_PRIVATE_KEY, BUZZ_RELAY_URL, CHANNEL_ID)
# Companion units (live on the VPS, see header of install history):
#   /etc/systemd/system/fleet-watchdog.{service,timer}
#
# Usage: fleet-watchdog.sh [--test-alert]
set -euo pipefail

ENV_FILE=/etc/fleet-watchdog.env
STATE_DIR=/var/lib/fleet-watchdog
CSV=/var/log/fleet-watchdog.csv
BUZZ=/usr/local/bin/buzz

WARN_AVAIL_MB=${WARN_AVAIL_MB:-1500}
CRIT_AVAIL_MB=${CRIT_AVAIL_MB:-800}
AGENT_RSS_WARN_MB=${AGENT_RSS_WARN_MB:-1024}
WARM_RSS_MB=${WARM_RSS_MB:-150}        # unit group RSS above this = active turn
COOLDOWN_S=${COOLDOWN_S:-1800}
DIGEST_HOUR_UTC=${DIGEST_HOUR_UTC:-9}

[[ -f $ENV_FILE ]] && set -a && . "$ENV_FILE" && set +a
mkdir -p "$STATE_DIR"

log() { logger -t fleet-watchdog -- "$*"; }

send() {
  [[ -n ${CHANNEL_ID:-} && -x $BUZZ ]] || { log "send skipped (no channel/binary): $1"; return 1; }
  "$BUZZ" messages send --channel "$CHANNEL_ID" --content "$1" >/dev/null 2>&1 \
    || log "send FAILED: $1"
}

if [[ ${1:-} == --test-alert ]]; then
  send "🔧 fleet-watchdog test alert — notification path works. ($(date -u +%FT%TZ))"
  exit $?
fi

# --- collect ---------------------------------------------------------------
alerts=()
total_rss_kb=0
warm=0
unit_count=0
while read -r unit _; do
  name=${unit#buzz-agent@}; name=${name%.service}
  unit_count=$((unit_count + 1))
  state=$(systemctl is-active "$unit" 2>/dev/null || true)
  [[ $state == active ]] || alerts+=("DOWN: $name is $state")
  main=$(systemctl show -p MainPID --value "$unit")
  rss_kb=0
  if [[ $main =~ ^[0-9]+$ && $main -gt 0 ]]; then
    pgid=$(ps -o pgid= -p "$main" 2>/dev/null | tr -d ' ' || true)
    [[ -n $pgid ]] && rss_kb=$(ps -o rss= -g "$pgid" 2>/dev/null | awk '{s+=$1} END {print s+0}')
  fi
  total_rss_kb=$((total_rss_kb + rss_kb))
  rss_mb=$((rss_kb / 1024))
  (( rss_mb > WARM_RSS_MB )) && warm=$((warm + 1))
  (( rss_mb > AGENT_RSS_WARN_MB )) && alerts+=("RAM: $name at ${rss_mb}MB (limit ${AGENT_RSS_WARN_MB}MB)")
done < <(systemctl list-units 'buzz-agent@*' --no-legend --plain)

avail_mb=$(awk '/MemAvailable/{print int($2/1024)}' /proc/meminfo)
(( avail_mb < CRIT_AVAIL_MB )) && alerts+=("CRITICAL: ${avail_mb}MB RAM available")
(( avail_mb >= CRIT_AVAIL_MB && avail_mb < WARN_AVAIL_MB )) && alerts+=("WARNING: ${avail_mb}MB RAM available")

# --- trend sample ------------------------------------------------------------
echo "$(date -u +%FT%TZ),$unit_count,$warm,$((total_rss_kb / 1024)),$avail_mb" >> "$CSV"

# --- alert on state change with cooldown -------------------------------------
now=$(date +%s)
fingerprint=$(printf '%s\n' "${alerts[@]:-}" | sort -u | md5sum | cut -d' ' -f1)
last_fp=$(cat "$STATE_DIR/fingerprint" 2>/dev/null || echo none)
last_sent=$(cat "$STATE_DIR/last_sent" 2>/dev/null || echo 0)

if [[ $fingerprint != "$last_fp" && $((now - last_sent)) -ge $COOLDOWN_S ]]; then
  if [[ ${#alerts[@]} -gt 0 ]]; then
    send "$(printf '🚨 *fleet-watchdog*\n%s' "$(printf '• %s\n' "${alerts[@]}")")"
  else
    send "✅ fleet-watchdog: all clear ($unit_count agents up, ${avail_mb}MB free, $warm active turns)"
  fi
  echo "$fingerprint" > "$STATE_DIR/fingerprint"
  echo "$now" > "$STATE_DIR/last_sent"
fi

# --- daily digest --------------------------------------------------------------
today=$(date -u +%F)
if [[ $(date -u +%H) == "$(printf '%02d' "$DIGEST_HOUR_UTC")" && $(cat "$STATE_DIR/last_digest" 2>/dev/null) != "$today" ]]; then
  peaks=$(tail -n 288 "$CSV" | awk -F, '{if($4>r)r=$4; if($3>w)w=$3; if($5<a||a=="")a=$5} END {printf "peak fleet RSS %dMB, peak concurrent turns %d, low-water RAM %dMB", r, w, a}')
  hint=""
  peak_rss=$(tail -n 288 "$CSV" | awk -F, 'BEGIN{m=0}{if($4>m)m=$4}END{print m}')
  (( peak_rss > 5120 )) && hint=" RESIZE SIGNAL: peak RSS past 5GB — plan the VPS upsize."
  send "📊 fleet-watchdog daily (24h): $peaks.$hint"
  echo "$today" > "$STATE_DIR/last_digest"
fi
