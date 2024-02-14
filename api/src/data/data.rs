use rust_decimal::Decimal;
use tokio_postgres::Client;

use crate::dto::ValidatorBondRecord;

pub async fn get_bonds(psql_client: &Client) -> anyhow::Result<Vec<ValidatorBondRecord>> {
    let rows = psql_client
        .query(
            "
            WITH cluster AS (SELECT MAX(epoch) as last_epoch FROM bonds)
            SELECT
                pubkey, vote_account, authority, revenue_share, updated_at, epoch
            FROM bonds, cluster WHERE epoch = cluster.last_epoch",
            &[],
        )
        .await?;

    let mut bonds: Vec<ValidatorBondRecord> = vec![];
    for row in rows {
        bonds.push(ValidatorBondRecord {
            pubkey: row.get("pubkey"),
            vote_account: row.get("vote_account"),
            authority: row.get("authority"),
            revenue_share: row.get::<_, Decimal>("revenue_share").try_into()?,
            epoch: row.get::<_, i32>("epoch").try_into()?,
        })
    }

    Ok(bonds)
}
