use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    models::AppState,
    utils::{get_error, to_hex},
};
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use chrono::{Duration, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use mongodb::bson::doc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use starknet::{
    core::{
        crypto::{ecdsa_sign, pedersen_hash},
        types::FieldElement,
    },
    id::encode,
};

#[derive(Deserialize, Debug, Clone)]
pub struct SigQuery {
    source_domain: String,
    target_address: FieldElement,
    source_signature: Vec<u8>,
    max_validity: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SNSResponse {
    owner_key: String,
}
#[derive(Serialize)]
struct JsonRequest {
    jsonrpc: String,
    method: String,
    params: JsonParams,
    id: i32,
}

#[derive(Serialize)]
struct JsonParams {
    domain: String,
}

#[derive(Deserialize, Debug)]
struct JsonResponse {
    jsonrpc: String,
    result: Option<String>,
    id: i32,
    error: Option<JsonError>,
}

#[derive(Deserialize, Debug)]
struct JsonError {
    code: i32,
    message: String,
}

lazy_static::lazy_static! {
    static ref SOL_SUBDOMAIN_STR: FieldElement = FieldElement::from_dec_str("9145722242464647959622012987758").unwrap();
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Json(query): Json<SigQuery>,
) -> impl IntoResponse {
    let source_domain = query.source_domain;
    let max_validity = query.max_validity;
    let target_address = query.target_address;
    let source_signature_array: [u8; 64] = match query.source_signature.clone().try_into() {
        Ok(arr) => arr,
        Err(_) => {
            return get_error("Invalid signature length".to_string());
        }
    };

    // verify max_validity is not expired
    if !is_valid_timestamp(max_validity) {
        return get_error("Signature expired".to_string());
    }

    // get owner of SNS domain
    let request_data = JsonRequest {
        jsonrpc: "2.0".to_string(),
        method: "sns_resolveDomain".to_string(),
        params: JsonParams {
            domain: source_domain.clone(),
        },
        id: 5678,
    };
    let client = reqwest::Client::new();
    match client
        .post(state.conf.solana.rpc_url.clone())
        .json(&request_data)
        .send()
        .await
    {
        Ok(response) => {
            match response.json::<JsonResponse>().await {
                Ok(parsed) => {
                    let owner_pubkey = parsed.result.unwrap();
                    println!("owner_pubKey: {:?}", owner_pubkey);

                    // recreate the message hash
                    let message = format!(
                        "{} allow claiming {} on starknet on {} at max validity timestamp {}",
                        owner_pubkey,
                        source_domain,
                        to_hex(&target_address),
                        max_validity
                    );
                    println!("message: {:?}", message);

                    // verify Solana signature
                    match verify_signature(&owner_pubkey, &message, &source_signature_array) {
                        Ok(()) => {
                            // Generate starknet signature
                            let stark_max_validity = Utc::now() + Duration::hours(1);
                            let stark_max_validity_sec = stark_max_validity.timestamp();

                            let domain_splitted: Vec<&str> = source_domain.split('.').collect();
                            let name_encoded = encode(domain_splitted[0]).unwrap();

                            let hash = pedersen_hash(
                                &pedersen_hash(
                                    &pedersen_hash(
                                        &SOL_SUBDOMAIN_STR,
                                        &FieldElement::from_dec_str(
                                            stark_max_validity_sec.to_string().as_str(),
                                        )
                                        .unwrap(),
                                    ),
                                    &name_encoded,
                                ),
                                &target_address,
                            );

                            match ecdsa_sign(&state.conf.solana.private_key.clone(), &hash) {
                                Ok(signature) => (
                                    StatusCode::OK,
                                    Json(json!({
                                        "r": signature.r,
                                        "s": signature.s,
                                        "max_validity": stark_max_validity_sec
                                    })),
                                )
                                    .into_response(),
                                Err(e) => get_error(format!(
                                    "Error while generating Starknet signature: {}",
                                    e
                                )),
                            }
                        }
                        Err(e) => get_error(format!("Signature verification failed: {}", e)),
                    }
                }
                Err(e) => get_error(format!("Error parsing response from SNS RPC: {}", e)),
            }
        }
        Err(e) => get_error(format!("Error sending request: SNS RPC: {}", e)),
    }
}

fn is_valid_timestamp(max_validity: u64) -> bool {
    let now = SystemTime::now();

    if let Ok(duration_since_epoch) = now.duration_since(UNIX_EPOCH) {
        let current_timestamp = duration_since_epoch.as_secs();
        current_timestamp < max_validity
    } else {
        false
    }
}

fn verify_signature(
    public_key_base58: &str,
    message: &str,
    signature_bytes: &[u8; 64],
) -> Result<(), ed25519_dalek::SignatureError> {
    // Convert the public key bytes to a VerifyingKey instance
    let public_key_bytes = bs58::decode(public_key_base58).into_vec().unwrap();
    let v_key =
        VerifyingKey::from_bytes(unsafe { &*(public_key_bytes.as_ptr() as *const [u8; 32]) })?;

    // Convert the signature bytes to a Signature instance
    let signature = Signature::from_bytes(signature_bytes);

    // Convert message to byte array
    let message_bytes = message.as_bytes();

    // Verify the signature
    v_key.verify(message_bytes, &signature)
}
