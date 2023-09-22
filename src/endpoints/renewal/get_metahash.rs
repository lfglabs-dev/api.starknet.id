use crate::{
    models::AppState,
    utils::{get_error, to_hex},
};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Json},
};
use mongodb::{
    bson::{doc, Bson},
    options::FindOneOptions,
};
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

    let find_options = FindOneOptions::builder()
        .sort(doc! { "timestamp": -1 })
        .build();
    let document = renew_collection
        .find_one(
            doc! {
                "payer": to_hex(&query.addr),
                "$or": [
                    { "_cursor.to": { "$exists": false } },
                    { "_cursor.to": Bson::Null },
                ],
            },
            find_options,
        )
        .await;

    if let Ok(sales_doc) = document {
        if let Some(doc) = sales_doc {
            let mut headers = HeaderMap::new();
            headers.insert("Cache-Control", HeaderValue::from_static("max-age=30"));
            match doc.get_str("meta_hash") {
                Ok(meta_hash_str) => {
                    let res = GetMetaHashData {
                        meta_hash: meta_hash_str.to_string(),
                    };
                    (StatusCode::OK, headers, Json(res)).into_response()
                }
                Err(e) => get_error(format!("No meta_hash found: {:?}", e)),
            }
        } else {
            get_error("No results found".to_string())
        }
    } else {
        get_error("Error while fetching from database".to_string())
    }
}
