use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{models::AppState, utils::get_error};
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use futures::StreamExt;
use mongodb::{bson::doc, bson::Document};
use serde::{Deserialize, Serialize};
use serde_json::json;
use starknet::core::types::FieldElement;

#[derive(Deserialize, Debug)]
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

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Json(query): Json<SigQuery>,
) -> impl IntoResponse {
    // verify max_validity is not expired
    if !is_valid_timestamp(query.max_validity) {
        return get_error("Signature expired".to_string());
    }

    // get owner of SNS domain
    println!("query: {:?}", query);
    let client = RpcClient::new(state.conf.solana.rpc_url);
    let res = resolve_owner(&client, "riton").await.unwrap();

    return (StatusCode::OK, Json("test")).into_response();
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
