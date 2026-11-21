#!/usr/bin/env bash
# Dry-runs jira.sh on a saved Jira search and checks the moves it plans.
set -euo pipefail

script="$(dirname "$0")/jira.sh"
search=$(mktemp)
trap 'rm -f "$search"' EXIT

# What Jira answers for labels = "pr-enshwick-7".
cat > "$search" << 'EOF'
{"issues": [
  {"key": "TEST-2", "fields": {"labels": ["batch-x", "pr-enshwick-7"]}},
  {"key": "TEST-3", "fields": {"labels": ["pr-enshwick-7", "pr-docs-12"]}},
  {"key": "TEST-4", "fields": {"labels": ["pr-enshwick-7", "pr-enshwick-70"]}},
  {"key": "TEST-5", "fields": {"labels": ["pr-enshwick-7", "preview", "pr-review"]}}
], "isLast": true}
EOF

run() {
  env -i PATH="$PATH" JIRA_DRY_RUN="$search" JIRA_PROJECT=TEST GITHUB_REPOSITORY=enshwick/enshwick PR_NUMBER=7 \
    GITHUB_EVENT_NAME=pull_request "$@" bash "$script"
}

expect() {
  local name=$1 want=$2 got
  shift 2
  got=$(run "$@")
  if [[ $got != "$want" ]]; then
    printf 'FAIL %s\n--- want\n%s\n--- got\n%s\n' "$name" "$want" "$got"
    exit 1
  fi
}

expect "a merge closes the branch's ticket and each delivered one no other pull request holds" \
'move TEST-1 to Done
unlabel TEST-2 pr-enshwick-7
move TEST-2 to Done
unlabel TEST-3 pr-enshwick-7
TEST-3 waits on another pull request
unlabel TEST-4 pr-enshwick-7
TEST-4 waits on another pull request
unlabel TEST-5 pr-enshwick-7
move TEST-5 to Done' \
  ACTION=closed MERGED=true HEAD_REF=feat/TEST-1-thing

expect "a merge of a branch without a key still closes the delivered tickets" \
'unlabel TEST-2 pr-enshwick-7
move TEST-2 to Done
unlabel TEST-3 pr-enshwick-7
TEST-3 waits on another pull request
unlabel TEST-4 pr-enshwick-7
TEST-4 waits on another pull request
unlabel TEST-5 pr-enshwick-7
move TEST-5 to Done' \
  ACTION=closed MERGED=true HEAD_REF=renovate/serde

expect "a pull request marked ready moves only the branch's ticket" \
'move TEST-1 to In Progress' \
  ACTION=ready_for_review MERGED=false HEAD_REF=feat/TEST-1-thing
