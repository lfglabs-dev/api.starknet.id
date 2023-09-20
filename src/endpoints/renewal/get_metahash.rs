use crate::{
    models::AppState,
    utils::{get_error, to_hex},
};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Json},
};
use futures::StreamExt;
use mongodb::bson::{doc, Bson};
use serde::{Deserialize, Serialize};
use starknet::core::types::FieldElement;
use std::sync::Arc;

#[derive(Serialize)]
pub struct GetMetaHashData {
    meta_hash: String,
}

#[derive(Deserialize)]
pub struct GetMetaHashQuery {
    addr: FieldElement,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<GetMetaHashQuery>,
) -> impl IntoResponse {
    let renew_collection = state
        .sales_db
        .collection::<mongodb::bson::Document>("sales");

    let documents = renew_collection
        .find(
            doc! {
                "payer": to_hex(&query.addr),
                "$or": [
                    { "_cursor.to": { "$exists": false } },
                    { "_cursor.to": Bson::Null },
                ],
            },
            None,
        )
        .await;

    match documents {
        Ok(mut cursor) => {
            let mut headers = HeaderMap::new();
            headers.insert("Cache-Control", HeaderValue::from_static("max-age=30"));

            if let Some(result) = cursor.next().await {
                match result {
                    Ok(res) => {
                        let meta_hash_str = res.get_str("meta_hash").unwrap();
                        let res = GetMetaHashData {
                            meta_hash: meta_hash_str.to_string(),
                        };
                        (StatusCode::OK, headers, Json(res)).into_response()
                    }
                    Err(e) => get_error(format!("Error while processing the document: {:?}", e)),
                }
            } else {
                (
                    StatusCode::OK,
                    headers,
                    Json(GetMetaHashData {
                        meta_hash: "".to_string(),
                    }),
                )
                    .into_response()
            }
        }
        Err(_) => get_error("Error while fetching from database".to_string()),
    }
}
