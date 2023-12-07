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
use futures::StreamExt;
use mongodb::{
    bson::{doc, Bson},
    options::AggregateOptions,
};
use serde::{Deserialize, Serialize};
use starknet::core::types::FieldElement;
use std::sync::Arc;

#[derive(Serialize)]
pub struct AddrToDomainData {
    domain: String,
    domain_expiry: Option<i64>,
}

#[derive(Deserialize)]
pub struct AddrToDomainQuery {
    addr: FieldElement,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<AddrToDomainQuery>,
) -> impl IntoResponse {
    let domains = state
        .starknetid_db
        .collection::<mongodb::bson::Document>("domains");
    let hex_addr = to_hex(&query.addr);

    let domains = state
        .starknetid_db
        .collection::<mongodb::bson::Document>("domains");

    let pipeline = vec![
        doc! { "$match": { "_cursor.to": null, "rev_address": hex_addr } },
        doc! { "$lookup": {
            "from": "id_owners",
            "localField": "rev_address",
            "foreignField": "owner",
            "as": "identity"
        }},
        doc! { "$unwind": "$identity" },
        doc! { "$lookup": {
            "from": "id_user_data",
            "let": { "id": "$identity.id" },
            "pipeline": [
                doc! { "$match": {
                    "_cursor.to": { "$exists": false },
                    "field": "0x000000000000000000000000000000000000000000000000737461726b6e6574",
                    "$expr": { "$eq": ["$id", "$$id"] }
                } }
            ],
            "as": "starknet_data"
        }},
        doc! { "$unwind": "$starknet_data" },
        doc! { "$match": {
            "$expr": { "$eq": ["$rev_address", "$starknet_data.data"] }
        }},
        doc! { "$project": {
            "domain": 1,
            "domain_expiry" : "$expiry"
        }},
    ];

    let cursor: Result<mongodb::Cursor<mongodb::bson::Document>, &str> = domains
        .aggregate(pipeline, AggregateOptions::default())
        .await
        .map_err(|_| "Error while executing aggregation pipeline");

    match cursor {
        Ok(mut cursor) => {
            while let Some(result) = cursor.next().await {
                return match result {
                    Ok(doc) => {
                        let domain = doc.get_str("domain").unwrap_or_default().to_owned();
                        let domain_expiry = doc.get_i64("domain_expiry").ok();
                        let data = AddrToDomainData {
                            domain,
                            domain_expiry,
                        };
                        (StatusCode::OK, Json(data)).into_response()
                    }
                    Err(e) => get_error(format!("Error calling the db: {}", e)),
                };
            }
            return get_error("No document found for the given address".to_string());
        }
        Err(e) => return get_error(format!("Error accessing the database: {}", e)),
    }
}
