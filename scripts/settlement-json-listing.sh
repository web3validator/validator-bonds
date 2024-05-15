#!/bin/bash

### ---- Call with argumetn merkle trees json
# settlement-json-listing.sh ""$JSON_FILE""
### ----

solsdecimal() {
  DECIMALS=9
  N="$@"
  if [ ${#N} -lt $DECIMALS ]; then
    FILLING_ZEROS=$(printf "%0.s0" $(seq 1 $((9-${#N}))))
    echo "0.${FILLING_ZEROS}${N}"
  else
    SOLS="${N::-$DECIMALS}"
    echo "${SOLS:-0}.${N:${#SOLS}}"
  fi
}


while [[ "$#" -gt 0 ]]; do
    case $1 in
        --settlements) SETTLEMENTS_JSON_FILE="$2"; shift ;;
        --merkle-trees) MERKLE_TREES_JSON_FILE="$2"; shift ;;
        *) echo "Unknown parameter passed: $1"; exit 1 ;;
    esac
    shift
done

if [ -z "$SETTLEMENTS_JSON_FILE" ] || [ -z "$MERKLE_TREES_JSON_FILE" ]; then
    echo "Both --settlements and --merkle-tree parameters are required"
    exit 1
fi

# 272 bytes
CLAIM_ACCOUNT_DATA_RENT=0.002784

SETTLEMENTS_EPOCH=$(jq '.epoch' "$SETTLEMENTS_JSON_FILE")
MERKLE_TREES_EPOCH=$(jq '.epoch' "$MERKLE_TREES_JSON_FILE")
if [ "$SETTLEMENTS_EPOCH" != "$MERKLE_TREES_EPOCH" ]; then
    echo "Epochs of files '$SETTLEMENTS_JSON_FILE' and '$MERKLE_TREES_JSON_FILE' are not matching: Settlements epoch: $SETTLEMENTS_EPOCH, Merkle trees epoch: $MERKLE_TREES_EPOCH"
    exit 1
fi

# sum of max total claim from json
echo "EPOCH: $SETTLEMENTS_EPOCH"
echo -n "Sum of max total claim at '$MERKLE_TREES_JSON_FILE': "
LAMPORTS=$(jq '.merkle_trees[].max_total_claim_sum' "$MERKLE_TREES_JSON_FILE" | paste -s -d+ | bc)
solsdecimal $LAMPORTS
NUMBER_OF_CLAIMS=$(jq '.merkle_trees[].tree_nodes | length'  "$MERKLE_TREES_JSON_FILE" |  paste -s -d+ | bc)
RENT=$(echo "scale=4; $NUMBER_OF_CLAIMS * $CLAIM_ACCOUNT_DATA_RENT" | bc)
echo "Number of all claims: $NUMBER_OF_CLAIMS, expected rent for newly created: $RENT"
COUNT=$(jq '.merkle_trees | length'  "$MERKLE_TREES_JSON_FILE")
echo "Number of merkle trees: $COUNT at $MERKLE_TREES_JSON_FILE"
echo '----------------'

# listing data of claims
# echo 'Data of vote account and max total sum claim:'
# grep "$MERKLE_TREES_JSON_FILE" -e 'vote_account' -e 'max_total_claim_sum'
# jq '.merkle_trees[] | {sum: .max_total_claim_sum, vote_account: .vote_account, claims: [.tree_nodes[].claim]}' "$MERKLE_TREES_JSON_FILE"

for I in $(seq 0 $((COUNT-1)) ); do
  echo "Index: $I"
  VOTE_ACCOUNT=$(jq   ".merkle_trees[$I] | .vote_account" "$MERKLE_TREES_JSON_FILE")
  echo "Vote account: $VOTE_ACCOUNT"
  echo -n 'Max claim sum: '
  LAMPORTS=$(jq   ".merkle_trees[$I] | .max_total_claim_sum" "$MERKLE_TREES_JSON_FILE")
  solsdecimal $LAMPORTS
  echo -n 'Number of claims: '
  jq   ".merkle_trees[$I] | .tree_nodes | length" "$MERKLE_TREES_JSON_FILE"
  echo -n 'Claims sum: '
  LAMPORTS=$(jq ".merkle_trees[$I] | .tree_nodes[].claim" "$MERKLE_TREES_JSON_FILE" | paste -s -d+ | bc)
  solsdecimal $LAMPORTS
  echo -n 'Funder: '
  jq -c '.settlements[] | select (.vote_account == '$VOTE_ACCOUNT') | .meta.funder' "$SETTLEMENTS_JSON_FILE"
  echo '----------------'
done


# TODO: utilize nodejs CLI to get the data
# get settlement
# pnpm --silent cli -u$RPC_URL show-settlement --epoch 608 -f json > /tmp/a.json
# get max claiming amount
# jq '.[].account.maxTotalClaim' /tmp/a.json | paste -s -d+ | bc
