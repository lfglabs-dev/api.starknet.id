use crate::{models::AppState, utils::to_hex};
use anyhow::{Context, Result};
use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::IntoResponse,
};
use axum_auto_routes::route;
use futures::stream::StreamExt;
use mongodb::{
    bson::{doc, Document},
    options::AggregateOptions,
    Cursor,
};
use serde::{Deserialize, Serialize};
use starknet::core::types::FieldElement;
use std::sync::Arc;

#[derive(Serialize, Debug)]
struct AddrToDomainData {
    domain: Option<String>,
    address: String,
}

#[derive(Deserialize)]
pub struct AddrToDomainsQuery {
    addresses: Vec<FieldElement>,
}

async fn process_cursor(
    mut cursor: Cursor<Document>,
    results: &mut Vec<AddrToDomainData>,
) -> Result<()> {
    while let Some(result) = cursor.next().await {
        let doc = result.context("Failed to retrieve document from cursor")?;
        if let (Ok(domain), Ok(address)) = (doc.get_str("domain"), doc.get_str("address")) {
            if let Some(data) = results.iter_mut().find(|d| d.address == address) {
                if data.domain == None {
                    data.domain = Some(domain.to_string());
                }
            }
        }
    }
    Ok(())
}

async fn run_aggregation_pipeline(
    collection: mongodb::Collection<Document>,
    pipeline: Vec<Document>,
    results: &mut Vec<AddrToDomainData>,
) -> Result<()> {
    let cursor = collection
        .aggregate(pipeline, AggregateOptions::default())
        .await
        .context("Failed to execute aggregation pipeline")?;

    process_cursor(cursor, results).await
}

#[route(post, "/addrs_to_domains", crate::endpoints::addrs_to_domains)]
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

    let addresses: Vec<String> = query.addresses.iter().map(to_hex).collect();

    let mut results = addresses
        .iter()
        .map(|addr| AddrToDomainData {
            domain: None,
            address: addr.clone(),
        })
        .collect::<Vec<_>>();

    let legacy_pipeline = create_legacy_pipeline(&addresses);
    if let Err(e) =
        run_aggregation_pipeline(domains_collection.clone(), legacy_pipeline, &mut results).await
    {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(e.to_string())).into_response();
    }

    let normal_pipeline = create_normal_pipeline(&addresses);
    if let Err(e) =
        run_aggregation_pipeline(domains_collection.clone(), normal_pipeline, &mut results).await
    {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(e.to_string())).into_response();
    }

    let fallback_addresses = results
        .iter()
        .filter_map(|data| data.domain.is_none().then(|| data.address.clone()))
        .collect::<Vec<_>>();

    let fallback_pipeline = create_fallback_pipeline(&fallback_addresses);
    if let Err(e) =
        run_aggregation_pipeline(id_owners_collection, fallback_pipeline, &mut results).await
    {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(e.to_string())).into_response();
    }

    (StatusCode::OK, Json(results)).into_response()
}

fn create_legacy_pipeline(addresses: &[String]) -> Vec<Document> {
    vec![
        doc! {
            "$match": {
                "legacy_address": { "$in": addresses },
                "_cursor.to": null,
                "$expr": { "$eq": ["$legacy_address", "$rev_address"] },
            },
        },
        doc! {
            "$project": {
                "_id": 0,
                "domain": 1,
                "address": "$legacy_address",
            },
        },
    ]
}

fn create_normal_pipeline(addresses: &[String]) -> Vec<Document> {
    vec![
        doc! { "$match": { "_cursor.to": null, "rev_address": { "$in": addresses } } },
        doc! { "$lookup": {
            "from": "id_owners",
            "let": { "rev_address": "$rev_address" },
            "pipeline": [
                doc! { "$match": doc! {
                        "id" : {
                            "$ne" : null
                        },
                    "$or": [
                        { "_cursor.to": null },
                        { "_cursor.to": { "$exists": false } }
                    ],
                        "$expr": { "$eq": ["$owner", "$$rev_address"] }
                    }
                }
            ],
            "as": "identity"
        }},
        doc! {
            "$unwind": doc! {
                "path": "$identity",
                "preserveNullAndEmptyArrays": true
            }
        },
        doc! {
            "$lookup": doc! {
                "from": "id_user_data",
                "let": doc! { "id": "$identity.id" },
                "pipeline": [
                    doc! { "$match": doc! {
                        "_cursor.to": doc! { "$exists": false },
                        "field": "0x000000000000000000000000000000000000000000000000737461726b6e6574",
                        "$expr": { "$eq": ["$id", "$$id"] }
                    } }
                ],
                "as": "starknet_data"
            }
        },
        doc! {
            "$unwind": doc! {
                "path": "$starknet_data",
                "preserveNullAndEmptyArrays": true
            }
        },
        doc! {
            "$lookup": doc! {
                "from": "domains",
                "let": doc! {
                    "root_domain": doc! {
                        "$reduce": doc! {
                            "input": doc! {
                                "$slice": [
                                    doc! {
                                        "$split": [
                                            "$domain",
                                            "."
                                        ]
                                    },
                                    1,
                                    10
                                ]
                            },
                            "initialValue": "",
                            "in": doc! {
                                "$cond": doc! {
                                    "if": doc! {
                                        "$eq": [
                                            "$$value",
                                            ""
                                        ]
                                    },
                                    "then": "$$this",
                                    "else": doc! {
                                        "$concat": [
                                            "$$value",
                                            ".",
                                            "$$this"
                                        ]
                                    }
                                }
                            }
                        }
                    }
                },
                "pipeline": [
                    doc! {
                        "$match": doc! {
                            "_cursor.to": null,
                            "$expr": doc! {
                                "$eq": [
                                    "$domain",
                                    "$$root_domain"
                                ]
                            }
                        }
                    },
                    doc! {
                        "$project": doc! {
                            "domain": 1,
                            "resolver": 1
                        }
                    }
                ],
                "as": "root_domain"
            }
        },
        doc! {
            "$unwind": doc! {
                "path": "$root_domain",
                "preserveNullAndEmptyArrays": true
            }
        },
        doc! {
            "$lookup": doc! {
                "from": "custom_resolutions",
                "let": doc! {
                    "rev_address": "$rev_address",
                    "resolver": "$root_domain.resolver"
                },
                "pipeline": [
                    doc! {
                        "$match": doc! {
                            "$expr": doc! {
                                "$and": [
                                    doc! {
                                        "$eq": [
                                            "$field",
                                            "0x000000000000000000000000000000000000000000000000737461726b6e6574"
                                        ]
                                    },
                                    doc! {
                                        "$eq": [
                                            "$value",
                                            "$$rev_address"
                                        ]
                                    },
                                    doc! {
                                        "$eq": [
                                            "$resolver",
                                            "$$resolver"
                                        ]
                                    }
                                ]
                            }
                        }
                    }
                ],
                "as": "custom_resolution"
            }
        },
        doc! {
            "$unwind": doc! {
                "path": "$custom_resolution",
                "preserveNullAndEmptyArrays": true
            }
        },
        doc! {
            "$match": doc! {
                "$or": [
                    doc! {
                        "$expr": doc! {
                            "$eq": [
                                "$rev_address",
                                "$starknet_data.data"
                            ]
                        }
                    },
                    doc! {
                        "$expr": doc! {
                            "$eq": [
                                "$rev_address",
                                "$custom_resolution.value"
                            ]
                        }
                    }
                ]
            }
        },
        doc! { "$project": {
            "domain": 1,
            "address" : "$rev_address",
        }},
    ]
}

fn create_fallback_pipeline(fallback_addresses: &[String]) -> Vec<Document> {
    vec![
        doc! {
            "$match": {
                "_cursor.to": null,
                "owner": { "$in": fallback_addresses },
                "main": true
            }
        },
        doc! {
            "$lookup": {
                "from": "domains",
                "let": { "id": "$id" },
                "pipeline": [
                    doc! { "$match": {
                        "_cursor.to": null,
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
    ]
}
