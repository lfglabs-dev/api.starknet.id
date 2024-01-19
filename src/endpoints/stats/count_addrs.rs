use crate::{models::AppState, utils::get_error};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    Json,
};
use axum_auto_routes::route;
use futures::StreamExt;
use mongodb::bson::{doc, Bson};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Serialize)]
pub struct CountAddrsData {
    count: i32,
}

#[derive(Deserialize)]
pub struct CountAddrsQuery {
    since: i64,
}

#[route(get, "/stats/count_addrs", crate::endpoints::stats::count_addrs)]
pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<CountAddrsQuery>,
) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert("Cache-Control", HeaderValue::from_static("max-age=60"));

    let domain_collection = state
        .starknetid_db
        .collection::<mongodb::bson::Document>("domains");
    let aggregate_cursor = domain_collection
        .aggregate(
            vec![
                doc! { "$match": {
                    "creation_date": { "$gte": query.since },
                    "$or": [
                        { "_cursor.to": { "$exists": false } },
                        { "_cursor.to": Bson::Null },
                    ],
                }},
                doc! { "$group": { "_id": "$legacy_address" }},
                doc! { "$count": "total" },
            ],
            None,
        )
        .await;

    match aggregate_cursor {
        Ok(mut cursor) => {
            if let Some(result) = cursor.next().await {
                match result {
                    Ok(doc_) => {
                        let count = doc_.get_i32("total").unwrap_or(0);
                        let response_data = CountAddrsData { count };
                        (StatusCode::OK, headers, Json(response_data)).into_response()
                    }
                    Err(e) => get_error(format!("Error while processing the document: {:?}", e)),
                }
            } else {
                get_error("No documents found".to_string())
            }
        }
        Err(e) => get_error(format!("Error while fetching from database: {:?}", e)),
    }
}
