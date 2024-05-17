use crate::{
    models::AppState,
    utils::{get_error, to_hex},
};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use axum_auto_routes::route;
use futures::StreamExt;
use mongodb::{bson::doc, options::AggregateOptions};
use regex::Regex;
use serde::{Deserialize, Serialize};
use starknet::core::types::FieldElement;
use std::{collections::HashMap, sync::Arc};

#[derive(Deserialize)]
pub struct StarknetIdQuery {
    addr: FieldElement,
}

lazy_static::lazy_static! {
    static ref DOMAIN_REGEX: Regex = Regex::new(r"^[^.]+\.stark$").unwrap();
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Subscription {
    pub enabled: bool,
    pub allowance: String,
    pub renewer_address: String,
    pub auto_renew_contract: Option<String>,
    pub token: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Subscriptions {
    pub eth_subscriptions: Option<Vec<Subscription>>,
    pub altcoin_subscriptions: Option<Vec<Subscription>>,
}

#[route(
    get,
    "/renewal/get_subscription_info",
    crate::endpoints::renewal::get_subscription_info
)]
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
            "$lookup": {
                "from": "auto_renew_flows_altcoins",
                "let": doc! { "domain_name": "$domainData.domain" },
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
                "as": "renew_flows_altcoins"
            }
        },
        doc! {
            "$unwind": {
                "path": "$renew_flows_altcoins",
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
                    },
                    { "renew_flows_altcoins": { "$eq": null } },
                    {
                        "renew_flows_altcoins.renewer_address": &addr,
                        "renew_flows_altcoins._cursor.to": null
                    }
                ]
            }
        },
        doc! {
            "$project": doc! {
                "_id": 0,
                "id": 1,
                "domain": "$domainData.domain",
                "eth_subscription": "$renew_flows",
                "enabled":  {
                    "$cond": {
                        "if": { "$eq": ["$renew_flows", null] },
                        "then": false,
                        "else": "$renew_flows.enabled"
                    }
                },
                "altcoin_subscription": "$renew_flows_altcoins",
                "altcoin_enabled":  {
                    "$cond": {
                        "if": { "$eq": ["$renew_flows_altcoins", null] },
                        "then": false,
                        "else": "$renew_flows_altcoins.enabled"
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
            let mut results: HashMap<String, Subscriptions> = HashMap::new();
            while let Some(doc) = cursor.next().await {
                if let Ok(doc) = doc {
                    if let Ok(domain) = doc.get_str("domain") {
                        // Initialize a Subscription entry for the domain if it doesn't exist
                        let entry =
                            results
                                .entry(domain.to_string())
                                .or_insert_with(|| Subscriptions {
                                    eth_subscriptions: None,
                                    altcoin_subscriptions: None,
                                });

                        let enabled = doc.get_bool("enabled").unwrap_or(false);
                        let altcoin_enabled = doc.get_bool("altcoin_enabled").unwrap_or(false);

                        if enabled {
                            let data = doc.get_document("eth_subscription").unwrap();
                            let eth_subscription = Subscription {
                                enabled,
                                allowance: data
                                    .get_str("allowance")
                                    .unwrap()
                                    .to_string(),
                                renewer_address: data
                                    .get_str("renewer_address")
                                    .unwrap()
                                    .to_string(),
                                auto_renew_contract:  data.get_str("auto_renew_contract").ok().map(|s| s.to_string()),
                                token: Some("0x049d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7".to_string())
                            };
                            entry
                                .eth_subscriptions
                                .get_or_insert_with(Vec::new)
                                .push(eth_subscription);
                        }
                        if altcoin_enabled {
                            let data = doc.get_document("altcoin_subscription").unwrap();
                            let auto_renew_contract = data
                                .get_str("auto_renew_contract")
                                .ok()
                                .map(|s| s.to_string());
                            let token = auto_renew_contract.as_ref().and_then(|contract| {
                                state
                                    .conf
                                    .subscription_to_altcoin
                                    .get(&FieldElement::from_hex_be(contract).unwrap())
                                    .cloned()
                            });

                            let altcoin_subscription = Subscription {
                                enabled: altcoin_enabled,
                                allowance: data.get_str("allowance").unwrap().to_string(),
                                renewer_address: data
                                    .get_str("renewer_address")
                                    .unwrap()
                                    .to_string(),
                                auto_renew_contract,
                                token,
                            };
                            entry
                                .altcoin_subscriptions
                                .get_or_insert_with(Vec::new)
                                .push(altcoin_subscription);
                        }
                    }
                } else {
                    continue;
                }
            }
            (StatusCode::OK, Json(results)).into_response()
        }
        Err(_) => get_error("Error while fetching from database".to_string()),
    }
}
