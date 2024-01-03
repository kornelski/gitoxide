#!/bin/bash
set -eu -o pipefail


function baseline() {
  local name=${1:?First argument is the repo path to get the status baseline from}
  git -C $name status --porcelain=2 > ${name}.baseline
}

git init untracked-in-root
(cd untracked-in-root
  touch file
  mkdir dir
  touch dir/a dir/b
  mkdir empty
  mkdir -p sub/still/empty
)
baseline untracked-in-root

