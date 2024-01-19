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
pub struct CountDomainsData {
    count: u64,
}

#[derive(Deserialize)]
pub struct CountDomainsQuery {
    since: i64,
}

#[route(get, "/stats/count_domains", crate::endpoints::stats::count_domains)]
pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<CountDomainsQuery>,
) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert("Cache-Control", HeaderValue::from_static("max-age=60"));

    let domain_collection = state
        .starknetid_db
        .collection::<mongodb::bson::Document>("domains");
    let filter = doc! {
        "expiry": { "$gte": chrono::Utc::now().timestamp() },
        "creation_date": { "$gte": query.since },
        "$or": [
            { "_cursor.to": { "$exists": false } },
            { "_cursor.to": Bson::Null },
        ],
    };

    let total = domain_collection.count_documents(filter, None).await;

    match total {
        Ok(count) => {
            let response_data = CountDomainsData { count };
            (StatusCode::OK, headers, Json(response_data)).into_response()
        }
        Err(e) => get_error(format!("Error while fetching from database: {:?}", e)),
    }
}
