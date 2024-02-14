use solana_sdk::commitment_config::CommitmentLevel;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct CommonCollectOptions {
    #[structopt(short = "u", env = "RPC_URL")]
    pub rpc_url: String,

    #[structopt(long = "commitment", default_value = "confirmed")]
    pub commitment: CommitmentLevel,
}
