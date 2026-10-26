#!/bin/zsh
# commit.sh <repo-dir> <message>: add everything and commit
set -e
git -C "$1" add -A
git -C "$1" -c user.name=audit -c user.email=audit@example.org -c commit.gpgsign=false commit -q -m "$2"
git -C "$1" rev-parse --short HEAD
