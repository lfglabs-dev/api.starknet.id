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
use regex::Regex;
use serde::Deserialize;
use starknet::core::types::FieldElement;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct StarknetIdQuery {
    addr: FieldElement,
}

lazy_static::lazy_static! {
    static ref DOMAIN_REGEX: Regex = Regex::new(r"^[^.]+\.stark$").unwrap();
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<StarknetIdQuery>,
) -> impl IntoResponse {
    let id_owners = state
        .starknetid_db
        .collection::<mongodb::bson::Document>("id_owners");
    let addr = to_hex(&query.addr);

    let pipeline = vec![
        doc! {
            "$match": doc! {
                "owner": to_hex(&query.addr),
                "_cursor.to": null
            }
        },
        doc! {
            "$lookup": doc! {
                "from": "domains",
                "let": doc! {
                    "local_id": "$id"
                },
                "pipeline": [
                    doc! {
                        "$match": doc! {
                            "$expr": doc! {
                                "$eq": [
                                    "$id",
                                    "$$local_id"
                                ]
                            },
                            "root": true,
                            "_cursor.to": null,
                        }
                    }
                ],
                "as": "domainData"
            }
        },
        doc! {
            "$unwind": doc! {
                "path": "$domainData",
                "preserveNullAndEmptyArrays": true
            }
        },
        doc! {
            "$lookup": {
                "from": "auto_renew_flows",
                "let": doc! {
                    "domain_name": "$domainData.domain"
                },
                "pipeline": [
                    doc! {
                        "$match": doc! {
                            "$expr": doc! {
                                "$eq": ["$domain", "$$domain_name"]
                            },
                            "_cursor.to": null
                        }
                    }
                ],
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
            "$project": doc! {
                "_id": 0,
                "id": 1,
                "domain": "$domainData.domain",
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

    let cursor = id_owners
        .aggregate(pipeline, AggregateOptions::default())
        .await;
    match cursor {
        Ok(mut cursor) => {
            let mut results: Vec<String> = Vec::new();
            while let Some(doc) = cursor.next().await {
                println!("doc: {:?}", doc);
                if let Ok(doc) = doc {
                    let enabled = doc.get_bool("enabled").unwrap_or(false);
                    if !enabled {
                        if let Ok(domain) = doc.get_str("domain") {
                            if DOMAIN_REGEX.is_match(domain) {
                                results.push(domain.to_string());
                            }
                        }
                    }
                }
            }
            (StatusCode::OK, Json(results)).into_response()
        }
        Err(_) => get_error("Error while fetching from database".to_string()),
    }
}
