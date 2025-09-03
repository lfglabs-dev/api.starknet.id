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
use axum_auto_routes::route;
use futures::future::join_all;
use futures::stream::StreamExt;
use mongodb::{
    bson::{doc, Bson},
    options::AggregateOptions,
};
use serde::{Deserialize, Serialize};
use starknet::core::types::FieldElement;
use std::sync::Arc;

#[derive(Serialize, Deserialize)]
pub struct FullId {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    domain_expiry: Option<i64>,
}

pub struct TempsFullId {
    id: String,
    domain: Option<String>,
    domain_expiry: Option<i64>,
}

#[derive(Serialize, Deserialize)]
struct NFTPP {
    contract: String,
    id: String,
}

#[derive(Deserialize)]
pub struct AddrQuery {
    addr: FieldElement,
}

#[derive(Serialize)]
pub struct FullIdResponse {
    full_ids: Vec<FullId>,
}

#[route(get, "/addr_to_full_ids", crate::endpoints::addr_to_full_ids)]
pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<AddrQuery>,
) -> impl IntoResponse {
    let id_owners = state
        .starknetid_db
        .collection::<mongodb::bson::Document>("id_owners");

    let pipeline = [
        doc! {
            "$match": doc! {
                "owner": to_hex(&query.addr),
                "id" : {
                    "$ne" : null
                  },
                "_cursor.to": Bson::Null
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
                            "_cursor.to": Bson::Null
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
            "$lookup": doc! {
                "from": "id_verifier_data",
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
                            "field": doc! {
                                "$in": [
                                    // nft_pp_contract
                                    "0x00000000000000000000000000000000006e66745f70705f636f6e7472616374",
                                    // nft_pp_id
                                    "0x00000000000000000000000000000000000000000000006e66745f70705f6964"
                                ]
                            },
                            "verifier": to_hex(&state.conf.contracts.pp_verifier),
                            "_cursor.to": Bson::Null
                        }
                    },
                    doc! {
                        "$project": doc! {
                            "_id": 0,
                            "field": 1,
                            "data": 1,
                            "extended_data": 1
                        }
                    }
                ],
                "as": "verifierData"
            }
        },
        doc! {
            "$project": doc! {
                "_id": 0,
                "id": 1,
                "domain": "$domainData.domain",
                "domain_expiry": "$domainData.expiry",
                "pp_verifier_data": "$verifierData"
            }
        },
    ];

    let aggregate_options = AggregateOptions::default();
    let cursor = id_owners.aggregate(pipeline, aggregate_options).await;

    match cursor {
        Ok(mut cursor) => {
            let mut temp_full_ids = Vec::new();
            while let Some(doc) = cursor.next().await {
                if let Ok(doc) = doc {
                    let id = FieldElement::from_hex_be(
                        &doc.get_str("id").unwrap_or_default().to_owned(),
                    )
                    .unwrap()
                    .to_string();
                    let domain = doc.get_str("domain").ok().map(String::from);
                    let domain_expiry = doc.get_i64("domain_expiry").ok();
                    temp_full_ids.push(TempsFullId {
                        id,
                        domain,
                        domain_expiry,
                    });
                }
            }
            let full_ids_futures: Vec<_> = temp_full_ids
                .iter()
                .map(|id| async move {
                    FullId {
                        id: id.id.clone(),
                        domain: id.domain.clone(),
                        domain_expiry: id.domain_expiry,
                    }
                })
                .collect();

            let full_ids: Vec<_> = join_all(full_ids_futures).await;

            let response = FullIdResponse { full_ids: full_ids };
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(_) => get_error("Error while fetching from database".to_string()),
    }
}
