use crate::{context::WrappedContext, data::data::get_bonds, dto::ValidatorBondRecord};
use serde::{Deserialize, Serialize};
use warp::{
    reject::Reject,
    reply::{json, Reply},
};

#[derive(Serialize, Debug, utoipa::ToSchema)]
pub struct BondsResponse {
    bonds: Vec<ValidatorBondRecord>,
}

#[derive(Deserialize, Serialize, Debug, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct QueryParams {}

#[derive(Debug)]
struct CustomError {
    message: String,
}

impl Reject for CustomError {}

#[utoipa::path(
    get,
    tag = "Bonds",
    operation_id = "List validator bonds",
    path = "/bonds",
    responses(
        (status = 200, body = BondsResponse),
    )
)]
pub async fn handler(
    _query_params: QueryParams,
    context: WrappedContext,
) -> Result<impl Reply, warp::Rejection> {
    match get_bonds(&context.read().await.psql_client).await {
        Ok(bonds) => Ok(json(&BondsResponse { bonds })),
        Err(_) => Err(warp::reject::custom(CustomError {
            message: "Failed to fetch bonds".into(),
        })),
    }
}
