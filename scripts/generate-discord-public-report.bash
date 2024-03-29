#!/bin/bash

settlement_collection_file="$1"

if [[ -z $settlement_collection_file ]]
then
    echo "Usage: $0 <settlement collection file>" >&2
    exit 1
fi

epoch="$(<"$settlement_collection_file" jq '.epoch' -r)"
if (( $(<"$settlement_collection_file" jq '.settlements | length' -r) == 0 ))
then
    echo "No settlements in epoch $epoch."
    exit
fi

echo "Total settlements in epoch $epoch: ☉$(<"$settlement_collection_file" jq '[.settlements[].claims_amount / 1e9] | add')"
echo
while read -r settlement
do
    vote_account=$(<<<"$settlement" jq '.vote_account' -r)
    claims_amount=$(<<<"$settlement" jq '.claims_amount / 1e9' -r)
    reason_code=$(<<<"$settlement" jq '.reason | keys[0]' -r)

    if ! [[ $reason_code == "ProtectedEvent" ]]
    then
        continue
    fi

    protected_event_code=$(<<<"$settlement" jq '.reason.ProtectedEvent | keys[0]' -r)
    protected_event_attributes=$(<<<"$settlement" jq '.reason.ProtectedEvent | to_entries[0].value' -r)

    case $protected_event_code in
        LowCredits)
          actual_credits=$(<<<"$protected_event_attributes" jq '.actual_credits')
          expected_credits=$(<<<"$protected_event_attributes" jq '.expected_credits')
          reason="Uptime $(bc <<<"scale=2; 100 * $actual_credits / $expected_credits")%"
          ;;

        CommissionIncrease)
          reason="Commission $(<<<"$protected_event_attributes" jq '.previous_commission')% -> $(<<<"$protected_event_attributes" jq '.current_commission')%"
          ;;

        *)
          echo "Unexpected protected event code: '$protected_event_code'" >&2
          exit 1
          ;;
    esac

    funder=$(<<<"$settlement" jq '.meta.funder' -r)
    case $funder in
        Marinade)
          funder_info="(settlement funded by Marinade DAO)"
          ;;

        ValidatorBond)
          funder_info="(settlement funded by the validator)"
          ;;

        *)
          echo "Unexpected funder: '$funder'" >&2
          exit 1
          ;;
    esac

    

    echo -e "$vote_account\tsettlement: ☉$claims_amount\t$reason\t$funder_info"
done < <(<"$settlement_collection_file" jq '.settlements | sort_by((.reason.ProtectedEvent | to_entries[0].value.actual_epr), (-.claims_amount)) | .[]' -c)
