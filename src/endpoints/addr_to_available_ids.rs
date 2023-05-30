use crate::{models::AppState, utils::get_error};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use futures::StreamExt;
use mongodb::bson::doc;
use serde::{Deserialize, Serialize};
use starknet::core::types::FieldElement;
use std::sync::Arc;

#[derive(Serialize)]
pub struct AvailableIds {
    ids: Vec<String>,
}

#[derive(Deserialize)]
pub struct AddrQuery {
    addr: FieldElement,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<AddrQuery>,
) -> impl IntoResponse {
    let starknet_ids = state
        .db
        .collection::<mongodb::bson::Document>("starknet_ids");
    let domains = state.db.collection::<mongodb::bson::Document>("domains");

    let addr = query.addr.to_string();
    let documents = starknet_ids
        .find(
            doc! {
                "owner": &addr,
                "_chain.valid_to": null,
            },
            None,
        )
        .await;

    let mut ids: Vec<String> = Vec::new();

    match documents {
        Ok(mut cursor) => {
            while let Some(doc) = cursor.next().await {
                if let Ok(doc) = doc {
                    let token_id = doc.get_str("token_id").unwrap_or_default().to_owned();
                    let domain_doc = domains
                        .find_one(
                            doc! {
                                "token_id": &token_id,
                                "_chain.valid_to": null,
                            },
                            None,
                        )
                        .await;

                    if let Ok(doc) = domain_doc {
                        if doc.is_none() {
                            ids.push(token_id);
                        }
                    }
                }
            }
            (StatusCode::OK, Json(AvailableIds { ids })).into_response()
        }
        Err(_) => get_error("Error while fetching from database".to_string()),
    }
}
