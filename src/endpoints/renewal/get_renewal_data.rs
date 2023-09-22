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
use mongodb::bson::doc;
use serde::{Deserialize, Serialize};
use starknet::core::types::FieldElement;
use std::sync::Arc;

#[derive(Serialize)]
pub struct StarknetIdData {
    starknet_id: String,
}

#[derive(Deserialize)]
pub struct StarknetIdQuery {
    addr: FieldElement,
    domain: String,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<StarknetIdQuery>,
) -> impl IntoResponse {
    let renew_collection = state
        .starknetid_db
        .collection::<mongodb::bson::Document>("auto_renew_flows");

    let documents = renew_collection
        .find(
            doc! {
                "renewer_address": to_hex(&query.addr),
                "domain": query.domain,
                "$or": [
                    { "_cursor.to": { "$exists": false } },
                    { "_cursor.to": null },
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
                        let mut res = res;
                        res.remove("_id");
                        res.remove("_cursor");
                        (StatusCode::OK, headers, Json(res)).into_response()
                    }
                    Err(e) => get_error(format!("Error while processing the document: {:?}", e)),
                }
            } else {
                get_error("no results founds".to_string())
            }
        }
        Err(_) => get_error("Error while fetching from database".to_string()),
    }
}
