#!/bin/bash

SCRIPT_PATH=`readlink -f "$0"`
CURRENT_DIR=`dirname "$SCRIPT_PATH"`

SDK_PACKAGE_JSON="$CURRENT_DIR/../packages/validator-bonds-sdk/package.json"
CLI_PACKAGE_JSON="$CURRENT_DIR/../packages/validator-bonds-cli/package.json"
CLI_INDEX="$CURRENT_DIR/../packages/validator-bonds-cli/src/index.ts"

VERSION=`cat $SDK_PACKAGE_JSON | grep version | cut -d '"' -f 4`
# version 1.1.6 update to string 1.1.7, but the version can be whatever number split with .
NEW_VERSION=`echo $VERSION | awk -F. '{$NF = $NF + 1;} 1' | sed 's/ /./g'`

echo "Updating CLI version $VERSION to $NEW_VERSION"
read -p "Press enter to continue"

sed -i "s/$VERSION/$NEW_VERSION/" $SDK_PACKAGE_JSON
sed -i "s/$VERSION/$NEW_VERSION/" $CLI_PACKAGE_JSON
sed -i "s/$VERSION/$NEW_VERSION/" $CLI_INDEX

if [ -e "./package.json" ]; then
    pnpm install
fi
echo "Done"
