use crate::{
    models::AppState,
    utils::{get_error, to_hex},
};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use mongodb::bson::doc;
use serde::{Deserialize, Serialize};
use starknet::core::types::FieldElement;
use std::sync::Arc;

#[derive(Serialize)]
pub struct AddrToDomainData {
    has_rev: bool,
}

#[derive(Deserialize)]
pub struct AddrHasRevQuery {
    addr: FieldElement,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<AddrHasRevQuery>,
) -> impl IntoResponse {
    let domains = state
        .starknetid_db
        .collection::<mongodb::bson::Document>("domains");
    let hex_addr = to_hex(&query.addr);
    let document = domains
        .find_one(
            doc! {
              "_cursor.to" : null,
              "rev_address" : hex_addr
            },
            None,
        )
        .await;

    match document {
        Ok(doc) => (
            StatusCode::OK,
            Json(AddrToDomainData {
                has_rev: doc.is_some(),
            }),
        )
            .into_response(),
        Err(_) => get_error("Error while fetching from database".to_string()),
    }
}
