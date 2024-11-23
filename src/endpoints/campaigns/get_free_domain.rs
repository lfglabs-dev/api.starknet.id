use crate::{ecdsa_sign::non_determinist_ecdsa_sign, models::AppState, utils::get_error};
use axum::{
    extract::{Query, State},
    response::{IntoResponse, Json},
};
use axum_auto_routes::route;
use mongodb::bson::doc;
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::json;
use starknet::core::types::FieldElement;
use starknet_crypto::pedersen_hash;
use std::sync::Arc;

use crate::utils::to_hex;

#[derive(Deserialize)]
pub struct FreeDomainQuery {
    addr: FieldElement,
    code: String,
    domain: String,
}

lazy_static::lazy_static! {
    // free domain registration
    static ref FREE_DOMAIN_STR: FieldElement = FieldElement::from_dec_str("2511989689804727759073888271181282305524144280507626647406").unwrap();
}

#[route(
    get,
    "/campaigns/get_free_domain",
    crate::endpoints::campaigns::get_free_domain
)]
pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<FreeDomainQuery>,
) -> impl IntoResponse {
    // assert domain is a root domain & get domain length
    let domain_parts = query.domain.split('.').collect::<Vec<&str>>();
    if domain_parts.len() != 2 {
        return get_error("Domain must be a root domain".to_string());
    }
    let domain_len = domain_parts[0].len();

    let free_domains = state
        .free_domains_db
        .collection::<mongodb::bson::Document>("free_domain_ticket");
    match free_domains
        .find_one(
            doc! {
                "code" : &query.code,
                "enabled": true,
            },
            None,
        )
        .await
    {
        Ok(Some(doc)) => {
            if let Ok(spent) = doc.get_bool("spent") {
                if spent {
                    if let Ok(spent_by) = doc.get_str("spent_by") {
                        if spent_by == to_hex(&query.addr) {
                            let r = doc.get_str("r").unwrap();
                            let s = doc.get_str("s").unwrap();
                            return (
                                StatusCode::OK,
                                Json(json!({
                                    "r": r,
                                    "s": s,
                                })),
                            )
                                .into_response();
                        } else {
                            return get_error(format!("Coupon code already used by {}\nIf you own this account, this means you have already used this coupon code with the other account. Please switch to it.", spent_by));
                        }
                    } else {
                        return get_error("Coupon code already used by someone else".to_string());
                    }
                }
            } else {
                println!("Error while verifying coupon code spent status and user address");
                return get_error("Error while verifying coupon code availability".to_string());
            }

            // Check domain length matches the coupon type
            if let Ok(coupon_type) = doc.get_str("type") {
                if let Some(pos) = coupon_type.find('+') {
                    if let Ok(domain_min_size) = coupon_type[..pos].parse::<usize>() {
                        if domain_len < domain_min_size {
                            return get_error(format!(
                                "Domain length is less than {}",
                                domain_min_size
                            ));
                        }
                    } else {
                        return get_error(
                            "Failed to parse the numeric part of the coupon type".to_string(),
                        );
                    }
                } else {
                    return get_error("Invalid coupon type format".to_string());
                }
            } else {
                return get_error("Error while verifying coupon code type".to_string());
            }

            // generate the signature
            let message_hash = pedersen_hash(&query.addr, &FREE_DOMAIN_STR);
            match non_determinist_ecdsa_sign(
                &state.conf.free_domains.priv_key.clone(),
                &message_hash,
            ) {
                Ok(signature) => {
                    // we blacklist the coupon code
                    match free_domains
                        .update_one(
                            doc! {
                                "code" : &query.code,
                                "type": "5+letters",
                            },
                            doc! {
                                "$set" : {
                                    "spent" : true,
                                    "spent_by" : to_hex(&query.addr),
                                    "r" : signature.r.to_string(),
                                    "s" : signature.s.to_string(),
                                },
                            },
                            None,
                        )
                        .await
                    {
                        Ok(_) => {
                            // Request paymaster API to add reward to the user
                            let api_url = format!(
                                "{}/accounts/{}/rewards",
                                state.conf.paymaster.api_url,
                                to_hex(&query.addr)
                            );
                            let api_key = &state.conf.paymaster.api_key;
                            let starknet_id_contract = &state.conf.contracts.starknetid;
                            let free_domain_contract = &state.conf.contracts.free_domains;
                            // Here you can add the actual request to the paymaster API
                            let client = reqwest::Client::new();
                            let res = client
                                .post(&api_url)
                                .header("api-key", api_key)
                                .json(&json!({
                                    "address": to_hex(&query.addr),
                                    "campaign": "Free Domain",
                                    "protocol": "STARKNETID",
                                    "freeTx": 1,
                                    "whitelistedCalls": [
                                        {
                                            "contractAddress": to_hex(starknet_id_contract),
                                            "entrypoint": "*"
                                        },
                                        {
                                            "contractAddress": to_hex(free_domain_contract),
                                            "entrypoint": "*"
                                        }
                                    ]
                                }))
                                .send()
                                .await;

                            match res {
                                Ok(response) if response.status().is_success() => (
                                    StatusCode::OK,
                                    Json(json!({
                                        "r": signature.r,
                                        "s": signature.s,
                                    })),
                                )
                                    .into_response(),
                                Ok(response) => get_error(format!(
                                    "Paymaster API request failed with status: {}",
                                    response.status()
                                )),
                                Err(e) => get_error(format!(
                                    "Error while requesting Paymaster API: {}",
                                    e
                                )),
                            }
                        }
                        Err(e) => get_error(format!("Error while updating coupon code: {}", e)),
                    }
                }
                Err(e) => get_error(format!("Error while generating signature: {}", e)),
            }
        }
        _ => get_error("Coupon code not found".to_string()),
    }
}
