use collect::dto::ValidatorBondRecord;
use log::info;
use serde_yaml;
use std::collections::{HashMap, HashSet};
use tokio_postgres::types::ToSql;
use tokio_postgres::NoTls;

use crate::commands::utils::{InsertQueryCombiner, UpdateQueryCombiner};

use super::common::CommonStoreOptions;

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
            revenue_share = u.revenue_share,
            updated_at = u.updated_at,
            epoch = u.epoch
            "
            .to_string(),
            "u(
                pubkey,
                vote_account,
                authority,
                revenue_share,
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
                    &b.revenue_share,
                    &b.updated_at,
                    &epoch,
                ];
                query.add(
                    &mut params,
                    HashMap::from_iter([
                        (0, "TEXT".into()),                     // pubkey
                        (1, "TEXT".into()),                     // vote_account
                        (2, "TEXT".into()),                     // authority
                        (3, "NUMERIC".into()),                  // revenue_share
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
                revenue_share,
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
                &b.revenue_share,
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
