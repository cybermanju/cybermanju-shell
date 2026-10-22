#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Disposable diagnostic guests only. Samples are not boot-success evidence.
# No wall-clock bound is promised if the guest kernel/scheduler itself stalls.
set +e +u +x

usage() {
  printf 'usage: bash %s SLEEP_ABSOLUTE -- COMMAND [ARG ...]\n' "$0" >&2
  exit 2
}
[[ $# -ge 3 && $1 == /* && $2 == -- ]] || usage
sleep_bin=$1
shift 2
if [[ ! -f $sleep_bin || ! -x $sleep_bin ]]; then
  printf 'CYBERMANJU_ACTIVATION_PROBE:disabled sleep=unavailable\n' >&2
  exec "$@"
fi

child= monitor= finished=0
# Avoid signaling old, completed jobs when a monitor reached its sample limit
# long before the command ended. This is not a pidfd/descendant supervisor.
running_job() {
  local wanted=$1 pid
  for pid in $(jobs -pr); do
    [[ $pid == "$wanted" ]] && return 0
  done
  return 1
}
stop_monitor() {
  local pid
  # Only this helper's running jobs: target and monitor. Include a monitor
  # spawned just before its PID assignment was interrupted by a signal.
  for pid in $(jobs -pr); do
    [[ $pid == "$child" ]] && continue
    kill -TERM "$pid" 2>/dev/null || :
    wait "$pid" 2>/dev/null || :
  done
  [[ -n $monitor ]] && { wait "$monitor" 2>/dev/null || :; }
  monitor=
}

read_field() {
  local LC_ALL=C
  local file=$1 count=$2
  REPLY=
  if [[ -r $file ]]; then
    # read -N returns nonzero at EOF even when it read useful data.
    IFS= read -r -N "$count" REPLY 2>/dev/null < "$file" || :
  fi
  [[ -n $REPLY ]] || REPLY=unavailable
}
sample() {
  local LC_ALL=C
  local stat_text status_text wchan_text
  read_field "/proc/$child/stat" 512; stat_text=$REPLY
  read_field "/proc/$child/status" 1024; status_text=$REPLY
  read_field "/proc/$child/wchan" 128; wchan_text=$REPLY
  # %q escapes newlines/control characters; proc text is never evaluated.
  printf 'CYBERMANJU_ACTIVATION_PROBE:sample pid=%s elapsed=%s stat=%q status=%q wchan=%q\n' \
    "$child" "$SECONDS" "$stat_text" "$status_text" "$wchan_text" >&2
}
watch_command() {
  local sleeper= count=0 sleep_status
  monitor_cleanup() {
    local pid
    trap '' TERM INT HUP
    # Include a sleep spawned just before sleeper=$!; never signal the target.
    for pid in $(jobs -pr); do
      [[ $pid == "$child" ]] && continue
      kill -TERM "$pid" 2>/dev/null || :
      wait "$pid" 2>/dev/null || :
    done
    exit 0
  }
  trap monitor_cleanup TERM INT HUP
  while (( count < 12 )); do
    kill -0 "$child" 2>/dev/null || return 0
    sample
    ((count += 1))
    if (( count == 12 )); then
      printf 'CYBERMANJU_ACTIVATION_PROBE:limit pid=%s samples=%s\n' "$child" "$count" >&2
      return 0
    fi
    "$sleep_bin" 5 &
    sleeper=$!
    wait "$sleeper" 2>/dev/null
    sleep_status=$?
    sleeper=
    if (( sleep_status != 0 )); then
      printf 'CYBERMANJU_ACTIVATION_PROBE:disabled sleep_status=%s\n' "$sleep_status" >&2
      return 0
    fi
  done
}
forward_signal() {
  local signal=$1 fallback=$2 result pid
  trap '' TERM INT HUP
  # The target is the sole running job in the gap before child=$!.
  if [[ -z $child && $finished == 0 ]]; then
    for pid in $(jobs -pr); do
      [[ $pid == "$monitor" ]] && continue
      child=$pid
      break
    done
  fi
  if [[ -n $child ]]; then
    if running_job "$child"; then kill -s "$signal" "$child" 2>/dev/null || :; fi
    stop_monitor
    wait "$child" 2>/dev/null
    result=$?
    # Preserve a target handler's own status, including 127 (also a failed wait).
  else
    stop_monitor
    result=$fallback
  fi
  printf 'CYBERMANJU_ACTIVATION_PROBE:end pid=%s status=%s signal=%s\n' "${child:-unavailable}" "$result" "$signal" >&2
  exit "$result"
}
trap 'forward_signal TERM 143' TERM
trap 'forward_signal INT 130' INT
trap 'forward_signal HUP 129' HUP
printf 'CYBERMANJU_ACTIVATION_PROBE:begin wrapper_pid=%s\n' "$$" >&2
# An explicit redirection prevents Bash's async-command /dev/null stdin default.
# The wrapper forwards signals to the direct child, not an arbitrary process tree.
( trap - TERM INT HUP; exec "$@" ) <&0 &
child=$!
printf 'CYBERMANJU_ACTIVATION_PROBE:start pid=%s\n' "$child" >&2
watch_command &
monitor=$!
wait "$child"
command_status=$?
finished=1
finished_pid=$child
child=
stop_monitor
printf 'CYBERMANJU_ACTIVATION_PROBE:end pid=%s status=%s\n' "$finished_pid" "$command_status" >&2
exit "$command_status"
