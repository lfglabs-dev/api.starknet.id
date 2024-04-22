use crate::{
    models::{AppState, OffchainResolverHint},
    resolving::get_offchain_resolver,
    utils::{extract_prefix_and_root, get_error, to_hex},
};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Json},
};
use axum_auto_routes::route;
use futures::StreamExt;
use mongodb::{bson::doc, options::AggregateOptions};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use starknet::{
    core::types::{BlockId, BlockTag, FieldElement, FunctionCall},
    macros::selector,
    providers::{jsonrpc::HttpTransport, JsonRpcClient, Provider},
};
use starknet_id::encode;
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
                        // means "starknet"
                        "field" : "0x000000000000000000000000000000000000000000000000737461726b6e6574",
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

        None => {
            match get_offchain_resolver(prefix, root_domain, &state) {
                // offchain resolver
                Some(offchain_resolver) => {
                    // query offchain_resolver uri
                    let url = format!("{}{}", offchain_resolver.uri[0], query.domain.clone());
                    let client = reqwest::Client::new();
                    match client
                        .get(&url)
                        .header("accept", "application/json")
                        .send()
                        .await
                    {
                        Ok(response) => match response.text().await {
                            Ok(text) => match serde_json::from_str::<OffchainResolverHint>(&text) {
                                Ok(hints) => {
                                    // Call the naming contract with the hints
                                    let provider = JsonRpcClient::new(HttpTransport::new(
                                        Url::parse(&state.conf.variables.rpc_url).unwrap(),
                                    ));
                                    //encode domain
                                    let trimmed_domain = query.domain.strip_suffix(".stark").unwrap_or(&query.domain);
                                    let splitted_domain = trimmed_domain.split('.').collect::<Vec<_>>();
                                    let encoded_domain : Vec<FieldElement> = splitted_domain.iter().map(|part| encode(part).unwrap()).collect();

                                    // build calldata
                                    let mut calldata : Vec<FieldElement> = vec![
                                        FieldElement::from(splitted_domain.len()),
                                    ];
                                    calldata.extend(encoded_domain);
                                    // add hint in calldata
                                    calldata.push(FieldElement::from(4_u64));
                                    calldata.push(hints.address);
                                    calldata.push(hints.r);
                                    calldata.push(hints.s);
                                    calldata.push(FieldElement::from(hints.max_validity));

                                    let call_result = provider
                                        .call(
                                            FunctionCall {
                                                contract_address: state.conf.contracts.naming,
                                                entry_point_selector: selector!("domain_to_address"),
                                                calldata,
                                            },
                                            BlockId::Tag(BlockTag::Latest),
                                        )
                                        .await;

                                    match call_result {
                                        Ok(result) => {
                                            // if call is successful we return the address
                                            (StatusCode::OK, Json(DomainToAddrData {
                                                addr: to_hex(&result[0]),
                                                domain_expiry: None
                                            })).into_response()
                                        }
                                        Err(e) => get_error(format!("{}", e)),
                                    }
                                },
                                Err(_) => get_error(text.to_string()),
                            },
                            Err(e) => get_error(format!(
                                "Failed to get JSON response while fetching offchain resolver api: {}",
                                e
                            )),
                        },
                        Err(e) => get_error(format!("Failed to fetch offchain resolver api: {}", e)),
                    }
                }
                None => {
                    // native resolver
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
                                        let addr =
                                            doc.get_str("addr").unwrap_or_default().to_owned();
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
                        Err(e) => get_error(format!("Error accessing the database: {}", e)),
                    }
                }
            }
        }
    }
}
