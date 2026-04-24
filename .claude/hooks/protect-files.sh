#!/bin/bash

set -euo pipefail

INPUT=$(cat)
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // empty')

if [[ -z "$FILE_PATH" ]]; then
  exit 0
fi

case "$FILE_PATH" in
  .env.example|*/.env.example)
    ;;
  */.env|*/.env.*|*/secrets/*|*/.git/*)
    echo "Blocked: $FILE_PATH is protected and must not be edited directly." >&2
    exit 2
    ;;
esac

exit 0
