#!/usr/bin/env bash
# Moves the Jira tickets of a pull request or a red main, as enshwick-ci; the
# jira workflow passes the event in the environment. JIRA_DRY_RUN names a saved
# Jira search response to read instead of Jira, and prints the moves.
set -euo pipefail

dry=${JIRA_DRY_RUN:-}

# A pull request from a fork gets no secrets.
if [[ -z $dry && -z ${JIRA_TOKEN:-} ]]; then
  echo "::notice::no Jira credentials in this run, skipped"
  exit 0
fi

api() {
  if [[ -n $dry ]]; then
    cat "$dry"
    return
  fi
  curl -sS --fail-with-body -u "$JIRA_EMAIL:$JIRA_TOKEN" \
    -H 'Accept: application/json' -H 'Content-Type: application/json' \
    -X "$1" "https://api.atlassian.com/ex/jira/$JIRA_SITE/rest/api/3$2" \
    ${3:+--data "$3"}
}

move() {
  local status id
  if [[ -n $dry ]]; then
    echo "move $1 to $2"
    return
  fi
  status=$(api GET "/issue/$1?fields=status" | jq -r .fields.status.name)
  if [[ $status == "$2" ]]; then
    echo "$1 is already $2"
    return
  fi
  id=$(api GET "/issue/$1/transitions" \
    | jq -r --arg to "$2" 'first(.transitions[] | select(.to.name == $to) | .id) // empty')
  if [[ -z $id ]]; then
    echo "::error::$1 has no transition from $status to $2"
    return 1
  fi
  api POST "/issue/$1/transitions" "$(jq -nc --arg id "$id" '{transition: {id: $id}}')" > /dev/null
  echo "$1: $status -> $2"
}

# Every ticket this run closed: the search index may still show them open.
closed=()

# Closes a ticket, then each parent left with no open child, up to the epic.
close() {
  local node=$1 parent open
  move "$node" Done
  closed+=("$node")
  [[ -n $dry ]] && return
  while parent=$(api GET "/issue/$node?fields=parent" | jq -r '.fields.parent.key // empty') && [[ -n $parent ]]; do
    open=$(api POST /search/jql "$(jq -nc --arg p "$parent" --arg closed "$(IFS=,; echo "${closed[*]}")" \
      '{jql: "parent = \($p) AND key NOT IN (\($closed)) AND statusCategory != Done", maxResults: 1, fields: ["status"]}')" \
      | jq '.issues | length')
    if (( open > 0 )); then
      echo "$parent has open children"
      return
    fi
    move "$parent" Done
    closed+=("$parent")
    node=$parent
  done
}

# Every ticket the merged pull request delivered carries its label, built from
# the repository and number alone so no key enters the repository. Each loses
# that label; one still waiting on another pull request keeps that one's label
# and stays open.
close_delivered() {
  local label="pr-${GITHUB_REPOSITORY#*/}-$PR_NUMBER" token="" page key waits
  local found=()
  # All pages first: removing labels while paging would shift the pages.
  while :; do
    page=$(api POST /search/jql "$(jq -nc --arg l "$label" --arg t "$token" \
      '{jql: "labels = \"\($l)\"", fields: ["labels"], maxResults: 100}
        + if $t == "" then {} else {nextPageToken: $t} end')")
    mapfile -t -O "${#found[@]}" found < <(jq -r --arg l "$label" \
      '.issues[] | "\(.key) \(any(.fields.labels[]; test("^pr-[a-z0-9-]+-[0-9]+$") and . != $l))"' <<< "$page")
    token=$(jq -r '.nextPageToken // empty' <<< "$page")
    [[ -z $token ]] && break
  done
  for entry in "${found[@]}"; do
    # A jq on Windows ends its lines with CRLF.
    read -r key waits <<< "${entry%$'\r'}"
    if [[ -n $dry ]]; then
      echo "unlabel $key $label"
    else
      api PUT "/issue/$key" "$(jq -nc --arg l "$label" '{update: {labels: [{remove: $l}]}}')" > /dev/null
    fi
    if [[ $waits == true ]]; then
      echo "$key waits on another pull request"
    else
      close "$key"
    fi
  done
}

if [[ $GITHUB_EVENT_NAME == workflow_run ]]; then
  branch=$(gh api "repos/$GITHUB_REPOSITORY/commits/$RUN_SHA/pulls" --jq '.[0].head.ref // ""')
else
  branch=$HEAD_REF
fi

key=""
if [[ $branch =~ (^|/)($JIRA_PROJECT-[0-9]+)(-|/|$) ]]; then
  key=${BASH_REMATCH[2]}
fi

if [[ $GITHUB_EVENT_NAME == pull_request && $ACTION == closed && $MERGED == true ]]; then
  [[ -n $key ]] && close "$key"
  close_delivered
elif [[ -z $key ]]; then
  echo "::notice::branch '$branch' names no ticket, skipped"
elif [[ $GITHUB_EVENT_NAME == workflow_run ]]; then
  move "$key" "In Progress"
  api POST "/issue/$key/comment" "$(jq -nc --arg name "$RUN_NAME" --arg url "$RUN_URL" '{body: {
    type: "doc", version: 1, content: [{type: "paragraph", content: [
      {type: "text", text: "main is red after this ticket'"'"'s pull request merged: "},
      {type: "text", text: "\($name) run", marks: [{type: "link", attrs: {href: $url}}]}
    ]}]}}')" > /dev/null
else
  move "$key" "In Progress"
fi
