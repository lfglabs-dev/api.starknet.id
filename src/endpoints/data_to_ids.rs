use crate::{models::AppState, utils::get_error};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Json},
};
use mongodb::bson::doc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Serialize)]
pub struct StarknetIdData {
    starknet_id: String,
}

#[derive(Deserialize)]
pub struct StarknetIdQuery {
    verifier: String,
    field: String,
    data: String,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<StarknetIdQuery>,
) -> impl IntoResponse {
    let ids_data = state
        .starknetid_db
        .collection::<mongodb::bson::Document>("starknet_ids_data");

    let document = ids_data
        .find_one(
            doc! {
                "verifier": &query.verifier,
                "field": &query.field,
                "data": &query.data,
                "_cursor.to": null,
            },
            None,
        )
        .await;

    match document {
        Ok(doc) => {
            let mut headers = HeaderMap::new();
            headers.insert("Cache-Control", HeaderValue::from_static("max-age=30"));

            if let Some(doc) = doc {
                let starknet_id = doc.get_str("token_id").unwrap_or_default().to_owned();
                let data = StarknetIdData { starknet_id };
                (StatusCode::OK, headers, Json(data)).into_response()
            } else {
                get_error("no tokenid associated to this data was found".to_string())
            }
        }
        Err(_) => get_error("Error while fetching from database".to_string()),
    }
}
