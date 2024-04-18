use ark_ff::{biginteger::BigInteger256, BigInteger};
use axum::{
    body::Body,
    http::StatusCode,
    response::{IntoResponse, Response},
    Router,
};
use serde::Serialize;
use serde_json::Value;
use starknet::core::types::FieldElement;
use std::{fmt::Write, sync::Arc};

use crate::models::AppState;

#[derive(Serialize)]
pub struct ErrorMessage {
    error: String,
}

pub fn get_error(error: String) -> Response {
    (StatusCode::BAD_REQUEST, error).into_response()
}

pub fn extract_prefix_and_root(domain: String) -> (String, String) {
    let parts: Vec<&str> = domain.split('.').rev().collect();

    let root = parts
        .iter()
        .take(2)
        .rev()
        .cloned()
        .collect::<Vec<&str>>()
        .join(".");
    let prefix = if parts.len() > 2 {
        format!(
            "{}.",
            parts
                .iter()
                .skip(2)
                .rev()
                .cloned()
                .collect::<Vec<&str>>()
                .join("."),
        )
    } else {
        String::new()
    };

    (prefix, root)
}

pub fn to_hex(felt: &FieldElement) -> String {
    let bytes = felt.to_bytes_be();
    let mut result = String::with_capacity(bytes.len() * 2 + 2);
    result.push_str("0x");
    for byte in bytes {
        write!(&mut result, "{:02x}", byte).unwrap();
    }
    result
}

pub fn to_u256(low: &str, high: &str) -> BigInteger256 {
    fn from_byte_slice(bytes: &[u8]) -> Option<BigInteger256> {
        if bytes.len() > 32 {
            return None; // Ensure the byte slice isn't larger than expected
        }

        let mut bits = [false; 256];
        for (ind_byte, byte) in bytes.iter().enumerate() {
            for ind_bit in 0..8 {
                bits[ind_byte * 8 + ind_bit] = (byte >> (7 - ind_bit)) & 1 == 1;
            }
        }

        Some(BigInteger256::from_bits_be(&bits))
    }

    let mut output = from_byte_slice(&hex::decode(&(low)[2..]).unwrap()).unwrap();
    let mut _high = from_byte_slice(&hex::decode(&(high)[2..]).unwrap()).unwrap();

    _high.muln(128);
    let _ = output.add_with_carry(&_high);
    output
}

pub async fn fetch_img_url(
    api_url: &str,
    api_key: &str,
    contract: String,
    id: String,
) -> Option<String> {
    let url = format!("{}/nft/{}/{}", api_url, contract, id);

    let response_text = reqwest::Client::new()
        .get(&url)
        .header("accept", "application/json")
        .header("x-api-key", api_key)
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;

    let json: Value = serde_json::from_str(&response_text).ok()?;
    json.get("image_url")
        .and_then(|v| v.as_str().map(ToString::to_string))
}

pub fn clean_string(input: &str) -> String {
    input.chars().filter(|&c| c != '\0').collect()
}

// required for axum_auto_routes
pub trait WithState: Send {
    fn to_router(self: Box<Self>, shared_state: Arc<AppState>) -> Router;

    fn box_clone(&self) -> Box<dyn WithState>;
}

impl WithState for Router<Arc<AppState>, Body> {
    fn to_router(self: Box<Self>, shared_state: Arc<AppState>) -> Router {
        self.with_state(shared_state)
    }

    fn box_clone(&self) -> Box<dyn WithState> {
        Box::new((*self).clone())
    }
}

impl Clone for Box<dyn WithState> {
    fn clone(&self) -> Box<dyn WithState> {
        self.box_clone()
    }
}
