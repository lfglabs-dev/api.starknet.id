use crate::{
    models::AppState,
    utils::{extract_prefix_and_root, get_error},
};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Json},
};
use axum_auto_routes::route;
use futures::StreamExt;
use mongodb::{bson::doc, options::AggregateOptions};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Serialize)]
pub struct DomainToAddrData {
    addr: String,
    domain_expiry: Option<i64>,
}

#[derive(Deserialize)]
pub struct DomainQuery {
    domain: String,
}

#[route(get, "/domain_to_addr", crate::endpoints::domain_to_addr)]
pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DomainQuery>,
) -> impl IntoResponse {
    let mut headers: HeaderMap = HeaderMap::new();
    headers.insert("Cache-Control", HeaderValue::from_static("max-age=60"));
    let (prefix, root_domain) = extract_prefix_and_root(query.domain.clone());

    match (&state.conf).reversed_resolvers.get(&root_domain) {
        // custom resolver
        Some(resolver) => {
            let custom_resolutions = state
                .starknetid_db
                .collection::<mongodb::bson::Document>("custom_resolutions");
            match custom_resolutions
                .find_one(
                    doc! {
                        "domain_slice" : prefix,
                        "resolver" : resolver,
                        "field" : "starknet",
                        "_cursor.to": null,
                    },
                    None,
                )
                .await
            {
                Ok(Some(doc)) => {
                    let data = DomainToAddrData {
                        addr: doc.get_str("value").unwrap().to_string(),
                        domain_expiry: None,
                    };
                    (StatusCode::OK, headers, Json(data)).into_response()
                }
                _ => get_error("no target found".to_string()),
            }
        }

        // native resolver
        None => {
            let domains = state
                .starknetid_db
                .collection::<mongodb::bson::Document>("domains");

            let pipeline = [
                doc! {
                    "$match": doc! {
                        "_cursor.to": null,
                        "domain": query.domain.clone()
                    }
                },
                doc! {
                    "$lookup": doc! {
                        "from": "id_user_data",
                        "let": doc! {
                            "userId": "$id"
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
                                            "$$userId"
                                        ]
                                    }
                                }
                            }
                        ],
                        "as": "userData"
                    }
                },
                doc! {
                    "$unwind": doc! {
                        "path": "$userData",
                        "preserveNullAndEmptyArrays": true
                    }
                },
                doc! {
                    "$lookup": doc! {
                        "from": "id_owners",
                        "let": doc! {
                            "userId": "$id"
                        },
                        "pipeline": [
                            doc! {
                                "$match": doc! {
                                    "$or": [
                                        doc! {
                                            "_cursor.to": doc! {
                                                "$exists": false
                                            }
                                        },
                                        doc! {
                                            "_cursor.to": null
                                        }
                                    ],
                                    "$expr": doc! {
                                        "$eq": [
                                            "$id",
                                            "$$userId"
                                        ]
                                    }
                                }
                            }
                        ],
                        "as": "ownerData"
                    }
                },
                doc! {
                    "$unwind": doc! {
                        "path": "$ownerData",
                        "preserveNullAndEmptyArrays": true
                    }
                },
                doc! {
                    "$project": doc! {
                        "addr": doc! {
                            "$cond": doc! {
                                "if": doc! {
                                    "$and": [
                                        doc! {
                                            "$ifNull": [
                                                "$legacy_address",
                                                false
                                            ]
                                        },
                                        doc! {
                                            "$ne": [
                                                "$legacy_address",
                                                "0x0000000000000000000000000000000000000000000000000000000000000000"
                                            ]
                                        }
                                    ]
                                },
                                "then": "$legacy_address",
                                "else": doc! {
                                    "$cond": doc! {
                                        "if": doc! {
                                            "$ifNull": [
                                                "$userData.data",
                                                false
                                            ]
                                        },
                                        "then": "$userData.data",
                                        "else": "$ownerData.owner"
                                    }
                                }
                            }
                        },
                        "domain_expiry": "$expiry"
                    }
                },
            ];

            // Execute the aggregation pipeline
            let cursor: Result<mongodb::Cursor<mongodb::bson::Document>, &str> = domains
                .aggregate(pipeline, AggregateOptions::default())
                .await
                .map_err(|_| "Error while executing aggregation pipeline");

            match cursor {
                Ok(mut cursor) => {
                    while let Some(result) = cursor.next().await {
                        return match result {
                            Ok(doc) => {
                                let addr = doc.get_str("addr").unwrap_or_default().to_owned();
                                let domain_expiry = doc.get_i64("domain_expiry").ok();
                                let data = DomainToAddrData {
                                    addr,
                                    domain_expiry,
                                };
                                (StatusCode::OK, Json(data)).into_response()
                            }
                            Err(e) => get_error(format!("Error calling the db: {}", e)),
                        };
                    }
                    return get_error("No document found for the given domain".to_string());
                }
                Err(e) => return get_error(format!("Error accessing the database: {}", e)),
            }
        }
    }
}
