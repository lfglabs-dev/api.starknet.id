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
use mongodb::{bson::doc, options::AggregateOptions};
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
    let hex_addr = to_hex(&query.addr);

    let domains = state
        .starknetid_db
        .collection::<mongodb::bson::Document>("domains");
    let legacy_pipeline = vec![
        doc! { "$match": { "_cursor.to": null, "rev_address": &hex_addr } },
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
            "domain_expiry" : "$expiry"
        }},
    ];

    let id_owners = state
        .starknetid_db
        .collection::<mongodb::bson::Document>("id_owners");
    let main_id_pipeline = vec![
        doc! { "$match": { "_cursor.to": null, "owner": hex_addr, "main": true } },
        doc! { "$lookup": {
            "from": "domains",
            "let": { "id": "$id" },
            "pipeline": [
                doc! { "$match": {
                    "_cursor.to": { "$exists": false },
                    "$expr": { "$eq": ["$id", "$$id"] }
                } }
            ],
            "as": "domain_data"
        }},
        doc! { "$unwind": "$domain_data" },
        doc! { "$project": {
            "domain": "$domain_data.domain",
            "domain_expiry" : "$domain_data.expiry"
        }},
    ];

    let cursor: Result<mongodb::Cursor<mongodb::bson::Document>, &str> = domains
        .aggregate(legacy_pipeline, AggregateOptions::default())
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

            // now trying default pipeline
            let cursor: Result<mongodb::Cursor<mongodb::bson::Document>, &str> = id_owners
                .aggregate(main_id_pipeline, AggregateOptions::default())
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
                Err(e) => get_error(format!("Error calling the db: {}", e)),
            }
        }
        Err(e) => get_error(format!("Error accessing the database: {}", e)),
    }
}
