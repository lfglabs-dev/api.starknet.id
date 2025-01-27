use crate::{models::AppState, utils::get_error};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    Json,
};
use axum_auto_routes::route;
use mongodb::bson::{doc, Bson};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Serialize)]
pub struct CountMintedDomainsData {
    count: u64,
}

#[route(get, "/stats/count_minted_domains", crate::endpoints::stats::count_minted_domains)]
pub async fn handler(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert("Cache-Control", HeaderValue::from_static("max-age=60"));

    let domain_collection = state
        .starknetid_db
        .collection::<mongodb::bson::Document>("domains");
    let filter = doc! {
        "domain": { "$exists": true },
        "$or": [
            { "_cursor.to": { "$exists": false } },
            { "_cursor.to": Bson::Null },
        ],
    };

    let total = domain_collection.count_documents(filter, None).await;

    match total {
        Ok(count) => {
            let response_data = CountMintedDomainsData { count };
            (StatusCode::OK, headers, Json(response_data)).into_response()
        }
        Err(e) => get_error(format!("Error while fetching from database: {:?}", e)),
    }
}
