#!/bin/sh
# Claims an issue of raoulg/atlas, or gives it back, so that its label, its assignee and its Status on
# the board (the project "Atlas", https://github.com/users/raoulg/projects/1) say the same:
#   sh tools/claim.sh 329           in progress: label `in progress`, assigned to you, Status "In Progress"
#   sh tools/claim.sh 329 release   waiting again: the label and your assignment go, Status "Todo"
# Closing an issue moves it to "Done"; the board's own workflow does that. The Coordination comment is
# written by hand: what it names differs per piece of work.
set -eu
repo=raoulg/atlas
owner=raoulg
board=1
issue=${1:?usage: sh tools/claim.sh <issue> [release]}
case "${2:-claim}" in
  claim) column="In Progress"; gh issue edit "$issue" -R "$repo" --add-label "in progress" --add-assignee @me >/dev/null ;;
  release) column="Todo"; gh issue edit "$issue" -R "$repo" --remove-label "in progress" --remove-assignee @me >/dev/null ;;
  *) echo "usage: sh tools/claim.sh <issue> [release]" >&2; exit 2 ;;
esac
project=$(gh project view "$board" --owner "$owner" --format json --jq .id)
field=$(gh project field-list "$board" --owner "$owner" --format json --jq '.fields[] | select(.name == "Status") | .id')
option=$(gh project field-list "$board" --owner "$owner" --format json --jq ".fields[] | select(.name == \"Status\") | .options[] | select(.name == \"$column\") | .id")
# adding an issue the board already holds gives back the item it is
item=$(gh project item-add "$board" --owner "$owner" --url "https://github.com/$repo/issues/$issue" --format json --jq .id)
gh project item-edit --id "$item" --project-id "$project" --field-id "$field" --single-select-option-id "$option" >/dev/null
echo "#$issue: $column"
