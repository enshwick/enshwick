#!/usr/bin/env bash
# Dry-runs jira.sh on a saved Jira search and checks the moves it plans.
set -euo pipefail

script="$(dirname "$0")/jira.sh"
search=$(mktemp)
none=$(mktemp)
trap 'rm -f "$search" "$none"' EXIT
echo '{"issues": [], "isLast": true}' > "$none"

# What Jira answers for labels = "pr-enshwick-7".
cat > "$search" << 'EOF'
{"issues": [
  {"key": "TEST-2", "fields": {"labels": ["batch-x", "pr-enshwick-7"]}},
  {"key": "TEST-3", "fields": {"labels": ["pr-enshwick-7", "pr-docs-12"]}},
  {"key": "TEST-4", "fields": {"labels": ["pr-enshwick-7", "pr-enshwick-70"]}},
  {"key": "TEST-5", "fields": {"labels": ["pr-enshwick-7", "preview", "pr-review"]}},
  {"key": "TEST-6", "fields": {"labels": ["pr-enshwick-7", "pr-.github-3"]}}
], "isLast": true}
EOF

run() {
  env -i PATH="$PATH" JIRA_DRY_RUN="$search" JIRA_PROJECT=TEST GITHUB_REPOSITORY=enshwick/enshwick PR_NUMBER=7 \
    GITHUB_EVENT_NAME=pull_request "$@" bash "$script"
}

expect() {
  local name=$1 want_status=$2 want=$3 got status=0
  shift 3
  got=$(run "$@" 2>&1) || status=$?
  if [[ $status != "$want_status" || $got != "$want" ]]; then
    printf 'FAIL %s\n--- want (exit %s)\n%s\n--- got (exit %s)\n%s\n' "$name" "$want_status" "$want" "$status" "$got"
    exit 1
  fi
}

delivered='move TEST-2 to Done
unlabel TEST-2 pr-enshwick-7
unlabel TEST-3 pr-enshwick-7
TEST-3 waits on another pull request
unlabel TEST-4 pr-enshwick-7
TEST-4 waits on another pull request
move TEST-5 to Done
unlabel TEST-5 pr-enshwick-7
unlabel TEST-6 pr-enshwick-7
TEST-6 waits on another pull request'

left_open="::notice::TEST-1 stays open: the pull request delivered other tickets"

expect "a merge closes each delivered ticket no other pull request holds and leaves the branch's ticket open" 0 \
"$delivered
$left_open" \
  ACTION=closed MERGED=true HEAD_REF=feat/TEST-1-thing

expect "a merge that labelled no ticket closes the branch's ticket" 0 \
'move TEST-1 to Done' \
  ACTION=closed MERGED=true HEAD_REF=feat/TEST-1-thing JIRA_DRY_RUN="$none"

expect "a re-run that finds no labelled ticket leaves the branch's ticket open" 0 \
"::warning::TEST-1 stays open: a re-run cannot tell whether the pull request delivered it; close it by hand if it did" \
  ACTION=closed MERGED=true HEAD_REF=feat/TEST-1-thing JIRA_DRY_RUN="$none" GITHUB_RUN_ATTEMPT=2

expect "a merge of a branch without a key still closes the delivered tickets" 0 \
"$delivered" \
  ACTION=closed MERGED=true HEAD_REF=renovate/serde

expect "a merge closes a delivered branch ticket once, with the others" 0 \
"$delivered" \
  ACTION=closed MERGED=true HEAD_REF=feat/TEST-2-thing

expect "a merge leaves the branch's ticket open while another pull request holds it" 0 \
"$delivered" \
  ACTION=closed MERGED=true HEAD_REF=feat/TEST-3-thing

expect "a ticket that fails to close keeps its label, and the rest still move" 1 \
"::error::TEST-2 has no transition to Done
::error::TEST-2 stays open with pr-enshwick-7; re-run this job
unlabel TEST-3 pr-enshwick-7
TEST-3 waits on another pull request
unlabel TEST-4 pr-enshwick-7
TEST-4 waits on another pull request
move TEST-5 to Done
unlabel TEST-5 pr-enshwick-7
unlabel TEST-6 pr-enshwick-7
TEST-6 waits on another pull request
$left_open" \
  ACTION=closed MERGED=true HEAD_REF=feat/TEST-1-thing JIRA_DRY_FAIL=TEST-2

expect "a pull request marked ready moves only the branch's ticket" 0 \
'move TEST-1 to In Progress' \
  ACTION=ready_for_review MERGED=false HEAD_REF=feat/TEST-1-thing
