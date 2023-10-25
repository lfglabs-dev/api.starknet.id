use crate::{
    models::AppState,
    utils::{fetch_img_url, to_hex, to_u256},
};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Json},
};
use chrono::NaiveDateTime;
use futures::StreamExt;
use mongodb::bson::doc;
use serde::{Deserialize, Serialize};
use starknet::core::types::FieldElement;
use std::{collections::HashMap, sync::Arc};

#[derive(Serialize)]
pub struct TokenURI {
    name: String,
    description: String,
    image: String,
    expiry: Option<i64>,
    attributes: Option<Vec<Attribute>>,
}

#[derive(Serialize)]
pub struct Attribute {
    trait_type: String,
    value: Vec<String>,
}

#[derive(Deserialize)]
pub struct TokenIdQuery {
    id: FieldElement,
}

#[derive(Serialize, Debug)]
pub struct VerifierData {
    verifier: String,
    field: String,
    data: Option<String>,
    extended_data: Option<Vec<String>>,
}

const NFT_PP_CONTRACT: &'static str =
    "0x00000000000000000000000000000000006e66745f70705f636f6e7472616374";
const NFT_PP_ID: &'static str =
    "0x00000000000000000000000000000000000000000000006e66745f70705f6964";

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TokenIdQuery>,
) -> impl IntoResponse {
    let domains = state
        .starknetid_db
        .collection::<mongodb::bson::Document>("domains");
    let id_verifier_data = state
        .starknetid_db
        .collection::<mongodb::bson::Document>("id_verifier_data");

    // Query the domains collection
    let domain_filter = doc! {
        "id": to_hex(&query.id),
        "_cursor.to": null
    };
    let domain_data = domains.find_one(domain_filter, None).await.unwrap();

    // Query the id_verifier_data collection
    let verifier_filter = doc! {
        "id": to_hex(&query.id),
        "$or": [
            { "_cursor.to": null },
            { "_cursor.to": { "$exists": false } }
        ],
        "verifier" : to_hex(&state.conf.contracts.pp_verifier),
        "field": {
            "$in": [
                NFT_PP_CONTRACT,
                NFT_PP_ID
            ]
        }
    };
    let mut verifier_data_by_field: HashMap<String, VerifierData> = HashMap::new();
    if let Ok(mut cursor) = id_verifier_data.find(verifier_filter, None).await {
        while let Some(result) = cursor.next().await {
            if let Ok(doc) = result {
                if let (Ok(verifier), Ok(field)) = (doc.get_str("verifier"), doc.get_str("field")) {
                    let data = doc.get_str("data").ok().map(String::from);

                    let extended_data = doc
                        .get_array("extended_data")
                        .ok()
                        .map(|bson_array| {
                            bson_array
                                .iter()
                                .filter_map(|bson| bson.as_str().map(String::from))
                                .collect()
                        })
                        .filter(|v: &Vec<String>| !v.is_empty());

                    verifier_data_by_field.insert(
                        field.to_string(),
                        VerifierData {
                            verifier: verifier.to_string(),
                            field: field.to_string(),
                            data,
                            extended_data,
                        },
                    );
                }
            }
        }
    }

    let img_url = match (
        verifier_data_by_field.get(NFT_PP_CONTRACT),
        verifier_data_by_field.get(NFT_PP_ID),
    ) {
        (Option::Some(data_contract), Option::Some(data_id)) => {
            let id_felts = data_id.to_owned().extended_data.as_ref().unwrap();
            let id = to_u256(id_felts.get(0).unwrap(), id_felts.get(1).unwrap());
            fetch_img_url(
                &state.conf.starkscan.api_url,
                &state.conf.starkscan.api_key,
                data_contract.data.to_owned().unwrap(),
                id.to_string(),
            )
            .await
        }
        _ => None,
    };

    let mut headers = HeaderMap::new();
    headers.insert("Cache-Control", HeaderValue::from_static("max-age=30"));

    match domain_data {
        Some(doc) => {
            let domain = doc.get_str("domain").unwrap_or_default().to_owned();
            let expiry = doc.get_i64("expiry").unwrap_or_default();

            let token_uri = TokenURI {
                name: domain.clone(),
                description: "This token represents an identity on StarkNet.".to_string(),
                image: match img_url {
                    Some(url) => url,
                    None => format!("https://starknet.id/api/identicons/{}", &query.id),
                },
                expiry: Some(expiry),
                attributes: Some(vec![
                    Attribute {
                        trait_type: "Subdomain".to_string(),
                        value: vec![if domain.contains(".") { "yes" } else { "no" }.to_string()],
                    },
                    Attribute {
                        trait_type: "Domain expiry".to_string(),
                        value: vec![NaiveDateTime::from_timestamp_opt(expiry.into(), 0)
                            .map(|dt| dt.format("%b %d, %Y").to_string())
                            .unwrap_or_else(|| "Invalid date".into())],
                    },
                    Attribute {
                        trait_type: "Domain expiry timestamp".to_string(),
                        value: vec![expiry.to_string()],
                    },
                ]),
            };
            (StatusCode::OK, headers, Json(token_uri)).into_response()
        }
        None => {
            let token_uri = TokenURI {
                name: format!("Starknet ID: {}", &query.id),
                description: "This token represents an identity on StarkNet.".to_string(),
                image: format!("https://starknet.id/api/identicons/{}", &query.id),
                expiry: None,
                attributes: None,
            };
            (StatusCode::OK, headers, Json(token_uri)).into_response()
        }
    }
}
