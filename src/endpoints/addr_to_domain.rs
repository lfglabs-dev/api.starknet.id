use crate::{
    models::AppState,
    utils::{get_error, to_hex},
};
use anyhow::{bail, Result};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use axum_auto_routes::route;
use futures::StreamExt;
use mongodb::{
    bson::{doc, Document},
    options::AggregateOptions,
    Cursor,
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

async fn read_cursor(mut cursor: Cursor<Document>) -> Result<AddrToDomainData> {
    if let Some(result) = cursor.next().await {
        let doc = result?;
        let domain = doc.get_str("domain").unwrap_or_default().to_owned();
        let domain_expiry = doc.get_i64("domain_expiry").ok();
        Ok(AddrToDomainData {
            domain,
            domain_expiry,
        })
    } else {
        bail!("No document found for the given address")
    }
}

async fn aggregate_data(
    collection: mongodb::Collection<Document>,
    pipeline: Vec<Document>,
) -> Result<AddrToDomainData> {
    let cursor = collection
        .aggregate(pipeline, AggregateOptions::default())
        .await?;
    read_cursor(cursor).await
}

#[route(get, "/addr_to_domain", crate::endpoints::addr_to_domain)]
pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<AddrToDomainQuery>,
) -> impl IntoResponse {
    let hex_addr = to_hex(&query.addr);
    let domains_collection = state.starknetid_db.collection::<Document>("domains");
    let id_owners_collection = state.starknetid_db.collection::<Document>("id_owners");

    let legacy_pipeline = create_legacy_pipeline(&hex_addr);
    let normal_pipeline = create_normal_pipeline(&hex_addr);
    let main_id_pipeline = create_main_id_pipeline(&hex_addr);

    let results = [
        aggregate_data(domains_collection.clone(), legacy_pipeline),
        aggregate_data(domains_collection.clone(), normal_pipeline),
        aggregate_data(id_owners_collection, main_id_pipeline),
    ];

    for result in results {
        match result.await {
            Ok(data) => return (StatusCode::OK, Json(data)).into_response(),
            Err(_) => continue,
        }
    }

    get_error("No data found for the given address".to_string())
}

fn create_legacy_pipeline(address: &String) -> Vec<Document> {
    vec![
        doc! { "$match": { "_cursor.to": null, "rev_address": address,     "$expr": {
          "$eq": ["$rev_address", "$legacy_address"]
        } } },
        doc! { "$project": {
            "domain": 1,
            "domain_expiry" : "$expiry"
        }},
    ]
}

fn create_normal_pipeline(address: &String) -> Vec<Document> {
    vec![
        doc! {
            "$match": doc! {
                "_cursor.to": null,
                "rev_address": address
            }
        },
        doc! {
            "$lookup": doc! {
                "from": "id_owners",
                "let": doc! {
                    "rev_address": "$rev_address"
                },
                "pipeline": [
                    doc! {
                        "$match": doc! {
                            "$or": [
                                doc! {
                                    "_cursor.to": null
                                },
                                doc! {
                                    "_cursor.to": doc! {
                                        "$exists": false
                                    }
                                }
                            ],
                            "$expr": doc! {
                                "$eq": [
                                    "$owner",
                                    "$$rev_address"
                                ]
                            }
                        }
                    }
                ],
                "as": "identity"
            }
        },
        doc! {
            "$unwind": doc! {
                "path": "$identity",
                "preserveNullAndEmptyArrays": true
            }
        },
        doc! {
            "$lookup": doc! {
                "from": "id_user_data",
                "let": doc! {
                    "id": "$identity.id"
                },
                "pipeline": [
                    doc! {
                        "$match": doc! {
                            "_cursor.to": doc! {
                                "$exists": false
                            },
                            "field": "0x000000000000000000000000000000000000000000000000737461726b6e6574",
                            "$expr": doc! {
                                "$eq": [
                                    "$id",
                                    "$$id"
                                ]
                            }
                        }
                    }
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
        doc! {
            "$project": doc! {
                "domain": 1,
                "domain_expiry": "$expiry"
            }
        },
    ]
}

fn create_main_id_pipeline(address: &String) -> Vec<Document> {
    vec![
        doc! { "$match": { "_cursor.to": null, "owner": address, "main": true } },
        doc! { "$lookup": {
            "from": "domains",
            "let": { "id": "$id" },
            "pipeline": [
                doc! { "$match": {
                    "_cursor.to": null,
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
    ]
}
