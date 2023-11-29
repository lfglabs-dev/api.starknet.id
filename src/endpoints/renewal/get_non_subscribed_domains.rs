use crate::{
    models::AppState,
    utils::{get_error, to_hex},
};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use futures::StreamExt;
use mongodb::{bson::doc, options::AggregateOptions};
use serde::Deserialize;
use starknet::core::types::FieldElement;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct StarknetIdQuery {
    addr: FieldElement,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<StarknetIdQuery>,
) -> impl IntoResponse {
    let domains = state
        .starknetid_db
        .collection::<mongodb::bson::Document>("domains");
    let addr = to_hex(&query.addr);

    let pipeline = vec![
        doc! {
            "$match": {
                "legacy_address": &addr,
                "_cursor.to": null,
            },
        },
        doc! {
            "$lookup": {
                "from": "auto_renew_flows",
                "localField": "domain",
                "foreignField": "domain",
                "as": "renew_flows"
            }
        },
        doc! {
            "$unwind": {
                "path": "$renew_flows",
                "preserveNullAndEmptyArrays": true
            }
        },
        doc! {
            "$match": {
                "$or": [
                    { "renew_flows": { "$eq": null } },
                    {
                        "renew_flows.renewer_address": &addr,
                        "renew_flows._cursor.to": null
                    }
                ]
            }
        },
        doc! {
            "$project": {
                "domain": 1,
                "enabled": {
                    "$cond": {
                        "if": { "$eq": ["$renew_flows", null] },
                        "then": false,
                        "else": "$renew_flows.enabled"
                    }
                },
            }
        },
    ];

    let cursor = domains
        .aggregate(pipeline, AggregateOptions::default())
        .await;
    match cursor {
        Ok(mut cursor) => {
            let mut results: Vec<String> = Vec::new();
            while let Some(doc) = cursor.next().await {
                if let Ok(doc) = doc {
                    let enabled = doc.get_bool("enabled").unwrap_or_default();
                    if !enabled {
                        let domain = doc.get_str("domain").map(|s| s.to_string()).ok().unwrap();
                        results.push(domain);
                    }
                }
            }
            (StatusCode::OK, Json(results)).into_response()
        }
        Err(_) => get_error("Error while fetching from database".to_string()),
    }
}
