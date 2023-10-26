use crate::{
    models::{AppState, Data},
    resolving::get_custom_resolver,
    utils::{fetch_img_url, get_error, to_hex, to_u256},
};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Json},
};
use futures::StreamExt;
use mongodb::bson::{doc, Bson, Document};
use serde::Deserialize;
use starknet::core::types::FieldElement;
use std::{collections::HashMap, sync::Arc};

#[derive(Deserialize)]
pub struct IdQuery {
    id: FieldElement,
}

#[derive(Debug)]
pub struct VerifierData {
    data: Option<String>,
    extended_data: Option<Vec<String>>,
}

const NFT_PP_CONTRACT: &'static str =
    "0x00000000000000000000000000000000006e66745f70705f636f6e7472616374";
const NFT_PP_ID: &'static str =
    "0x00000000000000000000000000000000000000000000006e66745f70705f6964";

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<IdQuery>,
) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert("Cache-Control", HeaderValue::from_static("max-age=30"));

    let domains = state
        .starknetid_db
        .collection::<mongodb::bson::Document>("domains");
    let starknet_ids = state
        .starknetid_db
        .collection::<mongodb::bson::Document>("id_owners");

    let hex_id = to_hex(&query.id);

    let domain_document = domains
        .find_one(
            doc! {
                "id": &hex_id,
                "_cursor.to": Bson::Null,
            },
            None,
        )
        .await;

    let domain_data = match domain_document {
        Ok(doc) => {
            if let Some(doc) = doc {
                let domain = doc.get_str("domain").unwrap_or_default().to_owned();
                if get_custom_resolver(&domains, &domain).await.is_none() {
                    let addr = doc.get_str("legacy_address").ok().map(String::from);
                    let expiry = doc.get_i64("expiry").ok();
                    Some((domain, addr, expiry))
                } else {
                    // we don't handle subdomains, todo: add support for braavos and argent
                    None
                }
            } else {
                None
            }
        }
        Err(_) => return get_error("Error while fetching from database".to_string()),
    };

    let owner_document = starknet_ids
        .find_one(
            doc! {
                "id": &hex_id,
                "_cursor.to": null,
            },
            None,
        )
        .await;

    let owner = match owner_document {
        Ok(doc) => doc.and_then(|doc| doc.get_str("owner").ok().map(String::from)),
        Err(_) => return get_error("Error while fetching from database".to_string()),
    };

    if owner.is_none() {
        return get_error("starknet id not found".to_string());
    }

    let current_social_verifiers = state
        .conf
        .contracts
        .verifiers
        .clone()
        .into_iter()
        .map(|x| to_hex(&x))
        .collect::<Vec<_>>();
    let mut all_social_verifiers = current_social_verifiers.clone();
    all_social_verifiers.extend(vec![to_hex(&state.conf.contracts.old_verifier)]);
    let owner = owner.unwrap();
    let pipeline = vec![
        doc! {
            "$match": {
                "$or": [
                    {
                        "field": {
                            "$in": ["0x0000000000000000000000000000000000000000000000000000676974687562", "0x0000000000000000000000000000000000000000000000000074776974746572", "0x00000000000000000000000000000000000000000000000000646973636f7264"]
                        },
                        "verifier": { "$in": all_social_verifiers } // modified this to accommodate both verifiers
                    },
                    {
                        "field": "0x0000000000000000000000000070726f6f665f6f665f706572736f6e686f6f64",
                        "verifier": to_hex(&state.conf.contracts.pop_verifier)
                    },
                    {
                        "field": {
                            // nft_pp_contract, nft_pp_id
                            "$in": ["0x00000000000000000000000000000000006e66745f70705f636f6e7472616374", "0x00000000000000000000000000000000000000000000006e66745f70705f6964", "0x00000000000000000000000000000000000000000000000000646973636f7264"]
                        },
                        "verifier": to_hex(&state.conf.contracts.pp_verifier)
                    },
                ],
                "id": &hex_id,
                "_cursor.to": null,
            }
        },
        doc! {
            "$sort": doc! {
                "_cursor.from": 1
            }
        },
        doc! {
            "$group": {
                "_id": { "field": "$field", "verifier": "$verifier" }, // group by both field and verifier
                "data": { "$first": "$data" },
                "extended_data": { "$first": "$extended_data" }
            }
        },
    ];

    let starknet_ids_data = state
        .starknetid_db
        .collection::<Document>("id_verifier_data");
    let results = starknet_ids_data.aggregate(pipeline, None).await;

    let mut verifier_data_by_field: HashMap<(String, String), VerifierData> = HashMap::new();
    if let Ok(mut cursor) = results {
        while let Some(result) = cursor.next().await {
            if let Ok(doc) = result {
                match doc.get_document("_id") {
                    Ok(inner_doc) => {
                        if let (Ok(verifier), Ok(field)) =
                            (inner_doc.get_str("verifier"), inner_doc.get_str("field"))
                        {
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
                                (verifier.to_string(), field.to_string()),
                                VerifierData {
                                    data,
                                    extended_data,
                                },
                            );
                        }
                    }
                    Err(_) => {}
                }
            }
        }
    }

    let mut github = None;
    for verifier in current_social_verifiers.to_owned() {
        match verifier_data_by_field.get(&(
            verifier,
            "0x0000000000000000000000000000000000000000000000000000676974687562".to_string(),
        )) {
            Some(verifier_data) => {
                github = verifier_data.data.to_owned().and_then(|data| {
                    FieldElement::from_hex_be(&data)
                        .map(|fe| fe.to_string())
                        .ok()
                });
            }
            None => {}
        }
    }

    let old_github = match verifier_data_by_field.get(&(
        to_hex(&state.conf.contracts.old_verifier),
        "0x0000000000000000000000000000000000000000000000000000676974687562".to_string(),
    )) {
        Some(verifier_data) => verifier_data.data.to_owned().and_then(|data| {
            FieldElement::from_hex_be(&data)
                .map(|fe| fe.to_string())
                .ok()
        }),
        None => None,
    };

    let mut twitter = None;
    for verifier in current_social_verifiers.to_owned() {
        match verifier_data_by_field.get(&(
            verifier,
            "0x0000000000000000000000000000000000000000000000000074776974746572".to_string(),
        )) {
            Some(verifier_data) => {
                twitter = verifier_data.data.to_owned().and_then(|data| {
                    FieldElement::from_hex_be(&data)
                        .map(|fe| fe.to_string())
                        .ok()
                })
            }
            None => {}
        }
    }

    let old_twitter = match verifier_data_by_field.get(&(
        to_hex(&state.conf.contracts.old_verifier),
        "0x0000000000000000000000000000000000000000000000000074776974746572".to_string(),
    )) {
        Some(verifier_data) => verifier_data.data.to_owned().and_then(|data| {
            FieldElement::from_hex_be(&data)
                .map(|fe| fe.to_string())
                .ok()
        }),
        None => None,
    };

    let mut discord: Option<String> = None;
    for verifier in current_social_verifiers.to_owned() {
        match verifier_data_by_field.get(&(
            verifier,
            "0x00000000000000000000000000000000000000000000000000646973636f7264".to_string(),
        )) {
            Some(verifier_data) => {
                discord = verifier_data.data.to_owned().and_then(|data| {
                    FieldElement::from_hex_be(&data)
                        .map(|fe| fe.to_string())
                        .ok()
                })
            }
            None => {}
        }
    }

    let old_discord = match verifier_data_by_field.get(&(
        to_hex(&state.conf.contracts.old_verifier),
        "0x00000000000000000000000000000000000000000000000000646973636f7264".to_string(),
    )) {
        Some(verifier_data) => verifier_data.data.to_owned().and_then(|data| {
            FieldElement::from_hex_be(&data)
                .map(|fe| fe.to_string())
                .ok()
        }),
        None => None,
    };

    let proof_of_personhood = match verifier_data_by_field.get(&(
        to_hex(&state.conf.contracts.pop_verifier),
        "0x0000000000000000000000000070726f6f665f6f665f706572736f6e686f6f64".to_string(),
    )) {
        Some(verifier_data) => verifier_data.data.to_owned().and_then(|data| {
            Some(data == "0x0000000000000000000000000000000000000000000000000000000000000001")
        }),
        None => None,
    };

    let img_url = match (
        verifier_data_by_field.get(&(
            to_hex(&state.conf.contracts.pp_verifier),
            NFT_PP_CONTRACT.to_string(),
        )),
        verifier_data_by_field.get(&(
            to_hex(&state.conf.contracts.pp_verifier),
            NFT_PP_ID.to_string(),
        )),
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

    let data = match domain_data {
        None => Data {
            domain: None,
            addr: None,
            domain_expiry: None,
            is_owner_main: false,
            owner_addr: owner,
            github,
            twitter,
            discord,
            proof_of_personhood,
            old_github,
            old_twitter,
            old_discord,
            starknet_id: query.id.to_string(),
            img_url,
        },
        Some((domain, addr, expiry)) => {
            let is_owner_main_document = domains
                .find_one(
                    doc! {
                        "domain": &domain,
                        "legacy_address": &owner,
                        "rev_address": &owner,
                        "_cursor.to": null,
                    },
                    None,
                )
                .await;
            let is_owner_main =
                is_owner_main_document.is_ok() && is_owner_main_document.unwrap().is_some();
            Data {
                domain: Some(domain),
                addr,
                domain_expiry: expiry,
                is_owner_main,
                owner_addr: owner,
                github,
                twitter,
                discord,
                proof_of_personhood,
                old_github,
                old_twitter,
                old_discord,
                starknet_id: query.id.to_string(),
                img_url,
            }
        }
    };

    (StatusCode::OK, headers, Json(data)).into_response()
}
