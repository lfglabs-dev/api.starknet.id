use crate::{models::AppState, utils::to_hex};
use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::IntoResponse,
};
use futures::stream::StreamExt;
use mongodb::{bson::doc, options::AggregateOptions};
use serde::{Deserialize, Serialize};
use starknet::core::types::FieldElement;
use std::sync::Arc;

#[derive(Serialize)]
pub struct AddrToDomainData {
    domain: Option<String>,
    address: String,
}

#[derive(Deserialize)]
pub struct AddrToDomainsQuery {
    addresses: Vec<FieldElement>,
}
pub async fn handler(
    State(state): State<Arc<AppState>>,
    Json(query): Json<AddrToDomainsQuery>,
) -> impl IntoResponse {
    let domains_collection = state
        .starknetid_db
        .collection::<mongodb::bson::Document>("domains");
    let id_owners_collection = state
        .starknetid_db
        .collection::<mongodb::bson::Document>("id_owners");

    let addresses: Vec<String> = query.addresses.iter().map(|addr| to_hex(addr)).collect();
    println!("addresses: {:?}", addresses);

    // Initialize results with all addresses set to domain: None
    let mut results: Vec<AddrToDomainData> = addresses
        .iter()
        .map(|addr| AddrToDomainData {
            domain: None,
            address: addr.clone(),
        })
        .collect();

    // Primary Query (Legacy)
    let legacy_pipeline = vec![
        doc! { "$match": { "_cursor.to": null, "rev_address": { "$in": &addresses } } },
        doc! { "$lookup": {
            "from": "id_owners",
            "let": { "rev_address": "$rev_address" },
            "pipeline": [
                 { "$match": {
                  "$or": [
                    { "_cursor.to": null },
                    { "_cursor.to": { "$exists": false } }
                ],
                    "$expr": { "$eq": ["$owner", "$$rev_address"] }
                } }
            ],
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
            "address" : "$rev_address",
        }},
    ];
    let cursor = domains_collection
        .aggregate(legacy_pipeline, AggregateOptions::default())
        .await;

    if let Ok(mut cursor) = cursor {
        while let Some(doc) = cursor.next().await {
            if let Ok(doc) = doc {
                if let (Ok(domain), Ok(address)) = (doc.get_str("domain"), doc.get_str("address")) {
                    // Find the corresponding address in results and update its domain
                    if let Some(result) = results.iter_mut().find(|data| data.address == address) {
                        result.domain = Some(domain.to_string());
                    }
                }
            }
        }
    }

    // Fallback Query
    let fallback_addresses: Vec<String> = results
        .iter()
        .filter_map(|data| {
            if data.domain.is_none() {
                Some(data.address.clone())
            } else {
                None
            }
        })
        .collect();
    let fallback_pipeline = vec![
        doc! {
            "$match": {
                "_cursor.to": null,
                "owner": { "$in": fallback_addresses.clone() },
                "main": true
            }
        },
        doc! {
            "$lookup": {
                "from": "domains",
                "let": { "id": "$id" },
                "pipeline": [
                    doc! { "$match": {
                        "_cursor.to": { "$exists": false },
                        "$expr": { "$eq": ["$id", "$$id"] }
                    } }
                ],
                "as": "domain_data"
            }
        },
        doc! { "$unwind": "$domain_data" },
        doc! {
            "$project": {
                "_id": 0,
                "domain": "$domain_data.domain",
                "address": "$owner",
            }
        },
    ];
    let cursor = id_owners_collection
        .aggregate(fallback_pipeline, AggregateOptions::default())
        .await;
    if let Ok(mut cursor) = cursor {
        while let Some(doc) = cursor.next().await {
            if let Ok(doc) = doc {
                if let (Ok(domain), Ok(address)) = (doc.get_str("domain"), doc.get_str("address")) {
                    // Find the corresponding address in results and update its domain
                    if let Some(result) = results.iter_mut().find(|data| data.address == address) {
                        result.domain = Some(domain.to_string());
                    }
                }
            }
        }
    }

    (StatusCode::OK, Json(results)).into_response()
}
