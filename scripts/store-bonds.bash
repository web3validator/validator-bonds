#!/bin/bash

set -e

if [ "$#" -ne 3 ]; then
    echo "Usage: $0 <STORE BINARY PATH> store-bonds --postgres-url <POSTGRES URL> --input-file <INPUT FILE>"
    exit 1
fi

STORE_BINARY_PATH="$1"
POTSGRES_URL="$2"
INPUT_FILE="$3"

"$STORE_BINARY_PATH" store-bonds --postgres-url "$POTSGRES_URL" --input-file "$INPUT_FILE"