#!/bin/zsh
# mkrepo.sh <repo-dir>: init a git repo (fresh) at <repo-dir>
set -e
dir=$1
command rm -rf "$dir"
mkdir -p "$dir"
git -C "$dir" init -q -b main
