#!/bin/bash

SCRIPT_PATH=`readlink -f "$0"`
CURRENT_DIR=`dirname "$SCRIPT_PATH"`

SDK_PACKAGE_JSON="$CURRENT_DIR/../packages/validator-bonds-sdk/package.json"
CLI_PACKAGE_JSON="$CURRENT_DIR/../packages/validator-bonds-cli/package.json"
CLI_INDEX="$CURRENT_DIR/../packages/validator-bonds-cli/src/index.ts"
README="$CURRENT_DIR/../README.md"

VERSION=`cat $SDK_PACKAGE_JSON | grep version | cut -d '"' -f 4`
NEW_VERSION=`echo $VERSION | awk -F. '{$NF = $NF + 1;} 1' | sed 's/ /./g'`

echo "Updating CLI version $VERSION to $NEW_VERSION"
read -p "Press enter to continue"

for I in $SDK_PACKAGE_JSON $CLI_PACKAGE_JSON $CLI_INDEX $README; do
    sed -i "s/$VERSION/$NEW_VERSION/" $I
done

if [ -e "./package.json" ]; then
    pnpm install
fi
echo "Done"
