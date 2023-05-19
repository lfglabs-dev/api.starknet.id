use crate::{models::AppState, utils::get_error};
use axum::{
    extract::{Extension, Query, State},
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
    domain: String,
    domain_expiry: i32,
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
    let dec_addr = query.addr.to_string();
    let document = domains
        .find_one(
            doc! {
                "addr": &dec_addr,
                "rev_addr": &dec_addr,
                "_chain.valid_to": null,
            },
            None,
        )
        .await;

    match document {
        Ok(doc) => {
            if let Some(doc) = doc {
                let domain = doc.get_str("domain").unwrap_or_default().to_owned();
                let expiry = doc.get_i32("expiry").unwrap_or_default();
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
