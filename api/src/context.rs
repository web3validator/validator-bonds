use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_postgres::Client;

pub struct Context {
    pub psql_client: Client,
}

impl Context {
    pub fn new(
        psql_client: Client,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            psql_client,
        })
    }
}

pub type WrappedContext = Arc<RwLock<Context>>;
