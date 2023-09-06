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
use mongodb::bson::{doc, Bson};
use serde::{Deserialize, Serialize};
use starknet::core::types::FieldElement;
use std::sync::Arc;

#[derive(Serialize)]
pub struct AddrToDomainData {
    domain: String,
    domain_expiry: i64,
}

#[derive(Deserialize)]
pub struct AddrToDomainQuery {
    addr: FieldElement,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<AddrToDomainQuery>,
) -> impl IntoResponse {
    let domains = state.db.collection::<mongodb::bson::Document>("domains");
    let hex_addr = to_hex(&query.addr);
    let document = domains
        .find_one(
            doc! {
                "legacy_address": &hex_addr,
                "rev_address": &hex_addr,
                "_chain.valid_to": Bson::Null,
            },
            None,
        )
        .await;

    match document {
        Ok(doc) => {
            if let Some(doc) = doc {
                let domain = doc.get_str("domain").unwrap_or_default().to_owned();
                let expiry = doc.get_i64("expiry").unwrap_or_default();
                let data = AddrToDomainData {
                    domain,
                    domain_expiry: expiry,
                };
                (StatusCode::OK, Json(data)).into_response()
            } else {
                get_error("No domain found".to_string())
            }
        }
        Err(_) => get_error("Error while fetching from database".to_string()),
    }
}
