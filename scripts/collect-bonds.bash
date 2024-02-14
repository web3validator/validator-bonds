#!/bin/bash

set -e

if [ "$#" -ne 2 ]; then
    echo "Usage: $0 <COLLECT BINARY PATH> collect-bonds -u <RPC URL>"
    exit 1
fi

COLLECT_BINARY_PATH="$1"
RPC_URL="$2"

"$COLLECT_BINARY_PATH" collect-bonds -u "$RPC_URL" > bonds.yaml
BONDS_COUNT=$(yq '. | length' bonds.yaml)


if [ "$BONDS_COUNT" -eq 0 ]; then
    echo "❌ Collecting validator bonds failed."
    exit 1
else
    echo "✅️ Successfully collected $BONDS_COUNT validator bonds."
    exit 0 
fi