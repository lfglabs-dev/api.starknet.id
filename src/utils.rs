use ark_ff::{biginteger::BigInteger256, BigInteger};
use axum::{
    body::Body,
    http::StatusCode,
    response::{IntoResponse, Response},
    Router,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use serde_json::Value;
use starknet::core::types::FieldElement;
use std::{fmt::Write, str, sync::Arc};

use crate::{config::Config, models::AppState};

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

// profile picture metadata utils
pub fn parse_base64_image(metadata: &str) -> String {
    let encoded_part = metadata
        .split(',')
        .nth(1)
        .unwrap_or("")
        .trim_end_matches('}');
    let decoded_bytes = STANDARD.decode(encoded_part).unwrap_or_else(|_| vec![]);
    let decoded_str = str::from_utf8(&decoded_bytes).unwrap_or("{}");
    let v: Value = serde_json::from_str(decoded_str).unwrap_or(serde_json::json!({}));
    v["image"].as_str().unwrap_or("").to_string()
}

fn parse_image_url(config: &Config, url: &str) -> String {
    if url.starts_with("ipfs://") {
        url.replace("ipfs://", config.variables.ipfs_gateway.as_str())
    } else {
        url.to_string()
    }
}

pub async fn fetch_image_url(config: &Config, url: &str) -> String {
    let parsed_url = parse_image_url(config, url);
    match reqwest::get(&parsed_url).await {
        Ok(resp) => match resp.json::<Value>().await {
            Ok(data) => parse_image_url(config, data["image"].as_str().unwrap_or("")),
            Err(_) => "Error fetching data".to_string(),
        },
        Err(_) => "Error fetching data".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod extract_prefix_and_root {
        use super::*;

        #[test]
        fn test_standard_domain() {
            let (prefix, root) = extract_prefix_and_root("sub.example.com".to_string());
            assert_eq!(prefix, "sub.");
            assert_eq!(root, "example.com");
        }

        #[test]
        fn test_multiple_subdomains() {
            let (prefix, root) = extract_prefix_and_root("deep.nested.sub.example.com".to_string());
            assert_eq!(prefix, "deep.nested.sub.");
            assert_eq!(root, "example.com");
        }

        #[test]
        fn test_no_subdomain() {
            let (prefix, root) = extract_prefix_and_root("example.com".to_string());
            assert_eq!(prefix, "");
            assert_eq!(root, "example.com");
        }

        #[test]
        fn test_single_part() {
            let (prefix, root) = extract_prefix_and_root("localhost".to_string());
            assert_eq!(prefix, "");
            assert_eq!(root, "localhost");
        }

        #[test]
        fn test_empty_string() {
            let (prefix, root) = extract_prefix_and_root("".to_string());
            assert_eq!(prefix, "");
            assert_eq!(root, "");
        }

        #[test]
        fn test_with_trailing_dot() {
            let (prefix, root) = extract_prefix_and_root("sub.example.com.".to_string());
            assert_eq!(prefix, "sub.example.");
            assert_eq!(root, "com.");
        }

        #[test]
        fn test_complex_tld() {
            let (prefix, root) = extract_prefix_and_root("service.example.co.uk".to_string());
            assert_eq!(prefix, "service.example.");
            assert_eq!(root, "co.uk");
        }

        #[test]
        fn test_dots_only() {
            let (prefix, root) = extract_prefix_and_root("...".to_string());
            assert_eq!(prefix, "..");
            assert_eq!(root, ".");
        }

        #[test]
        fn test_unicode_domain() {
            let (prefix, root) = extract_prefix_and_root("sub.例子.com".to_string());
            assert_eq!(prefix, "sub.");
            assert_eq!(root, "例子.com");
        }
    }
}
