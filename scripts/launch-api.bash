#!/bin/bash

set -e

if [ "$#" -ne 2 ]; then
    echo "Usage: $0 <API BINARY PATH> --postgres-url <POSTGRES URL>"
    exit 1
fi

API_BINARY_PATH="$1"
POTSGRES_URL="$2"

"$API_BINARY_PATH" --postgres-url "$POTSGRES_URL"