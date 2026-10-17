#!/bin/bash
# Fail if tracked files contain personal data: home paths, the macOS account
# name, or local hostnames. The GitHub handle (with the "k") is allowed.
# Patterns are assembled from parts so this script doesn't match itself.
set -u
user="ryan""schultze"
home_prefix="/Us""ers/"
pattern="${home_prefix}|(^|[^a-z])${user}|[A-Za-z0-9-]+\\.lo""cal([^A-Za-z]|$)"
if hits=$(git grep -nIE -- "$pattern"); then
  echo "Personal data in tracked files:" >&2
  echo "$hits" >&2
  exit 1
fi
echo "check-personal: clean"
