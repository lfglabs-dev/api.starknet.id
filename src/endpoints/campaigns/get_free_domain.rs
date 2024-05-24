use crate::{ecdsa_sign::ecdsa_sign, models::AppState, utils::get_error};
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

#[derive(Deserialize)]
pub struct FreeDomainQuery {
    addr: FieldElement,
    code: String,
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
    // verify campaign is active
    let now = chrono::Utc::now().timestamp();
    if now < state.conf.free_domains.start_time || now > state.conf.free_domains.end_time {
        return get_error("Campaign not active".to_string());
    }

    let free_domains = state
        .starknetid_db
        .collection::<mongodb::bson::Document>("free_domain_ticket");
    match free_domains
        .find_one(
            doc! {
                "code" : &query.code,
                "enabled": true,
                "type": "5+letters",
            },
            None,
        )
        .await
    {
        Ok(Some(doc)) => {
            let used = doc.get_bool("used").unwrap();
            if used {
                return get_error("Coupon code already used".to_string());
            }

            // generate the signature
            let message_hash = pedersen_hash(&query.addr, &FREE_DOMAIN_STR);
            match ecdsa_sign(&state.conf.free_domains.priv_key.clone(), &message_hash) {
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
                                },
                            },
                            None,
                        )
                        .await
                    {
                        Ok(_) => (
                            // and return the signature
                            StatusCode::OK,
                            Json(json!({
                                "r": signature.r,
                                "s": signature.s,
                            })),
                        )
                            .into_response(),
                        Err(e) => get_error(format!("Error while updating coupon code: {}", e)),
                    }
                }
                Err(e) => get_error(format!("Error while generating signature: {}", e)),
            }
        }
        _ => get_error("Coupon code not found".to_string()),
    }
}
