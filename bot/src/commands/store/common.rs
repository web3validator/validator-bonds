use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct CommonStoreOptions {
    #[structopt(long = "input-file")]
    pub input_path: String,

    #[structopt(long = "postgres-url")]
    pub postgres_url: String,
}
