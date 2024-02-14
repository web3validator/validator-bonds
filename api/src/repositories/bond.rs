use std::collections::{HashMap, HashSet};

use log::info;
use rust_decimal::Decimal;
use tokio_postgres::{types::ToSql, Client, NoTls};

use crate::{
    dto::ValidatorBondRecord,
    repositories::utils::{InsertQueryCombiner, UpdateQueryCombiner},
};

use super::common::CommonStoreOptions;

pub async fn get_bonds(psql_client: &Client) -> anyhow::Result<Vec<ValidatorBondRecord>> {
    let rows = psql_client
        .query(
            "
            WITH cluster AS (SELECT MAX(epoch) as last_epoch FROM bonds)
            SELECT
                pubkey, vote_account, authority, cpmpe, updated_at, epoch
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
            epoch: row.get::<_, i32>("epoch").try_into()?,
            cpmpe: row.get::<_, Decimal>("cpmpe").try_into()?,
            updated_at: row.get("updated_at"),
        })
    }

    Ok(bonds)
}

const DEFAULT_CHUNK_SIZE: usize = 500;

pub async fn store_bonds(options: CommonStoreOptions) -> anyhow::Result<()> {
    let (mut psql_client, psql_conn) =
        tokio_postgres::connect(&options.postgres_url, NoTls).await?;

    tokio::spawn(async move {
        if let Err(err) = psql_conn.await {
            log::error!("Connection error: {}", err);
            std::process::exit(1);
        }
    });

    let input = std::fs::File::open(options.input_path)?;
    let bonds: Vec<ValidatorBondRecord> = serde_yaml::from_reader(input)?;
    let bonds_records: HashMap<_, _> = bonds
        .iter()
        .map(|record| (record.pubkey.clone(), record))
        .collect();
    let epoch = bonds[0].epoch as i32;

    let mut updated_bonds: HashSet<_> = Default::default();

    for chunk in psql_client
        .query(
            "
        SELECT pubkey
        FROM bonds
        WHERE epoch = $1
    ",
            &[&epoch],
        )
        .await?
        .chunks(DEFAULT_CHUNK_SIZE)
    {
        let mut query = UpdateQueryCombiner::new(
            "bonds".to_string(),
            "
            pubkey = u.pubkey,
            vote_account = u.vote_account,
            authority = u.authority,
            cpmpe = u.cpmpe,
            updated_at = u.updated_at,
            epoch = u.epoch
            "
            .to_string(),
            "u(
                pubkey,
                vote_account,
                authority,
                cpmpe,
                updated_at,
                epoch
            )"
            .to_string(),
            "bonds.pubkey = u.pubkey AND bonds.epoch = u.epoch".to_string(),
        );
        for row in chunk {
            let pubkey: &str = row.get("pubkey");

            if let Some(b) = bonds_records.get(pubkey) {
                let mut params: Vec<&(dyn ToSql + Sync)> = vec![
                    &b.pubkey,
                    &b.vote_account,
                    &b.authority,
                    &b.cpmpe,
                    &b.updated_at,
                    &epoch,
                ];
                query.add(
                    &mut params,
                    HashMap::from_iter([
                        (0, "TEXT".into()),                     // pubkey
                        (1, "TEXT".into()),                     // vote_account
                        (2, "TEXT".into()),                     // authority
                        (3, "NUMERIC".into()),                  // cpmpe
                        (4, "TIMESTAMP WITH TIME ZONE".into()), // updated_at
                        (5, "INTEGER".into()),                  // epoch
                    ]),
                );
                updated_bonds.insert(pubkey.to_string());
            }
        }
        query.execute(&mut psql_client).await?;
        info!(
            "Updated previously existing Bond records: {}",
            updated_bonds.len()
        );
    }
    let bonds_records: Vec<_> = bonds_records
        .into_iter()
        .filter(|(pubkey, _)| !updated_bonds.contains(pubkey))
        .collect();
    let mut insertions = 0;

    for chunk in bonds_records.chunks(DEFAULT_CHUNK_SIZE) {
        let mut query = InsertQueryCombiner::new(
            "bonds".to_string(),
            "
                pubkey,
                vote_account,
                authority,
                cpmpe,
                updated_at,
                epoch
        "
            .to_string(),
        );

        for (pubkey, b) in chunk {
            if updated_bonds.contains(pubkey) {
                continue;
            }
            let mut params: Vec<&(dyn ToSql + Sync)> = vec![
                &b.pubkey,
                &b.vote_account,
                &b.authority,
                &b.cpmpe,
                &b.updated_at,
                &epoch,
            ];
            query.add(&mut params);
        }
        insertions += query.execute(&mut psql_client).await?.unwrap_or(0);
        info!("Stored {} new Bond records", insertions);
    }

    Ok(())
}
