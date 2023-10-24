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
    domain_expiry: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    nft_pp: Option<NFTPP>,
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

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<AddrQuery>,
) -> impl IntoResponse {
    let id_owners = state
        .starknetid_db
        .collection::<mongodb::bson::Document>("id_owners");

    let pipeline = [
        doc! {
            "$addFields": doc! {
                "id": to_hex(&query.addr),
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
                                    "0x0000000000000000000000000000000000000000000000000074776974746572"
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
                            "data": 1
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
            let mut full_ids = Vec::new();
            while let Some(doc) = cursor.next().await {
                if let Ok(doc) = doc {
                    let id = FieldElement::from_hex_be(
                        &doc.get_str("id").unwrap_or_default().to_owned(),
                    )
                    .unwrap()
                    .to_string();
                    let domain = doc.get_str("domain").ok().map(String::from);
                    let domain_expiry = doc.get_i32("domain_expiry").ok();
                    let pp_verifier_data = doc.get_array("pp_verifier_data").ok();
                    let mut nft_pp = None;
                    if let Some(data) = pp_verifier_data {
                        if data.len() >= 2 {
                            if let (Some(contract_data), Some(id_data)) = (data.get(0), data.get(1))
                            {
                                if let (Bson::Document(contract_doc), Bson::Document(id_doc)) =
                                    (contract_data, id_data)
                                {
                                    if let (Ok(contract_str), Ok(id_str)) =
                                        (contract_doc.get_str("data"), id_doc.get_str("data"))
                                    {
                                        nft_pp = Some(NFTPP {
                                            contract: contract_str.to_string(),
                                            id: id_str.to_string(),
                                        });
                                    }
                                }
                            }
                        }
                    }
                    full_ids.push(FullId {
                        id,
                        domain,
                        domain_expiry,
                        nft_pp,
                    });
                }
            }
            let response = FullIdResponse { full_ids };
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(_) => get_error("Error while fetching from database".to_string()),
    }
}
