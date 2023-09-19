use crate::{
    models::{AppState, Data},
    resolving::get_custom_resolver,
    utils::{get_error, to_hex},
};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Json},
};
use futures::StreamExt;
use mongodb::bson::{doc, Document};
use serde::Deserialize;
use starknet::core::types::FieldElement;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct DomainQuery {
    domain: String,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DomainQuery>,
) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert("Cache-Control", HeaderValue::from_static("max-age=30"));

    let domains = state.starknetid_db.collection::<mongodb::bson::Document>("domains");
    match get_custom_resolver(&domains, &query.domain).await {
        None => {}
        Some(res) => {
            // todo: add support for argent and braavos here
            return get_error(format!("custom resolver {} is not supported yet", res));
        }
    }

    let starknet_ids = state.starknetid_db.collection::<mongodb::bson::Document>("id_owners");

    let domain_document = domains
        .find_one(
            doc! {
                "domain": &query.domain,
                "_cursor.to": null,
            },
            None,
        )
        .await;

    let (domain, addr, expiry, starknet_id) = match domain_document {
        Ok(Some(doc)) => {
            let domain = doc.get_str("domain").unwrap_or_default().to_owned();
            let addr = doc.get_str("legacy_address").ok().map(String::from);
            let expiry = doc.get_i64("expiry").ok();
            let id = doc.get_str("id").unwrap_or_default().to_owned();
            (domain, addr, expiry, id)
        }
        _ => return get_error("Error while fetching from database".to_string()),
    };

    let owner_document = starknet_ids
        .find_one(
            doc! {
                "id": &starknet_id,
                "_cursor.to": null,
            },
            None,
        )
        .await;
    let owner_addr = match owner_document {
        Ok(Some(doc)) => doc.get_str("owner").ok().map(String::from).unwrap(),
        _ => return get_error("Error while fetching starknet-id from database".to_string()),
    };
    let pipeline = vec![
        doc! {
            "$match": {
                "$or": [
                    {
                        "field": {
                            "$in": ["0x0000000000000000000000000000000000000000000000000000676974687562", "0x0000000000000000000000000000000000000000000000000074776974746572", "0x00000000000000000000000000000000000000000000000000646973636f7264"]
                        },
                        "verifier": { "$in": [ to_hex(&state.conf.contracts.verifier), to_hex(&state.conf.contracts.old_verifier)] } // modified this to accommodate both verifiers
                    },
                    {
                        "field": "0x0000000000000000000000000070726f6f665f6f665f706572736f6e686f6f64",
                        "verifier": to_hex(&state.conf.contracts.pop_verifier)
                    }
                ],
                "id": &starknet_id,
                "_cursor.to": null,
            }
        },
        doc! {
            "$group": {
                "_id": { "field": "$field", "verifier": "$verifier" }, // group by both field and verifier
                "data": { "$first": "$data" }
            }
        },
    ];

    let starknet_ids_data = state.starknetid_db.collection::<Document>("id_verifier_data");
    let results = starknet_ids_data.aggregate(pipeline, None).await;

    let mut github = None;
    let mut old_github = None; // added for old_verifier
    let mut twitter = None;
    let mut old_twitter = None; // added for old_verifier
    let mut discord = None;
    let mut old_discord = None; // added for old_verifier
    let mut proof_of_personhood = None;

    if let Ok(mut cursor) = results {
        while let Some(result) = cursor.next().await {
            if let Ok(doc) = result {
                let field = doc.get_document("_id").unwrap().get_str("field").unwrap();
                let verifier = doc
                    .get_document("_id")
                    .unwrap()
                    .get_str("verifier")
                    .unwrap();

                // it's a bit ugly but it will get better when we removed the old verifier
                match (field, verifier) {
                    (
                        "0x0000000000000000000000000000000000000000000000000000676974687562",
                        verifier,
                    ) if verifier == to_hex(&state.conf.contracts.verifier) => {
                        github = doc.get_str("data").ok().and_then(|data| {
                            FieldElement::from_hex_be(data)
                                .map(|fe| fe.to_string())
                                .ok()
                        })
                    }
                    (
                        "0x0000000000000000000000000000000000000000000000000000676974687562",
                        verifier,
                    ) if verifier == to_hex(&state.conf.contracts.old_verifier) => {
                        old_github = doc.get_str("data").ok().and_then(|data| {
                            FieldElement::from_hex_be(data)
                                .map(|fe| fe.to_string())
                                .ok()
                        })
                    }

                    (
                        "0x0000000000000000000000000000000000000000000000000074776974746572",
                        verifier,
                    ) if verifier == to_hex(&state.conf.contracts.verifier) => {
                        twitter = doc.get_str("data").ok().and_then(|data| {
                            FieldElement::from_hex_be(data)
                                .map(|fe| fe.to_string())
                                .ok()
                        })
                    }
                    (
                        "0x0000000000000000000000000000000000000000000000000074776974746572",
                        verifier,
                    ) if verifier == to_hex(&state.conf.contracts.old_verifier) => {
                        old_twitter = doc.get_str("data").ok().and_then(|data| {
                            FieldElement::from_hex_be(data)
                                .map(|fe| fe.to_string())
                                .ok()
                        })
                    }

                    (
                        "0x00000000000000000000000000000000000000000000000000646973636f7264",
                        verifier,
                    ) if verifier == to_hex(&state.conf.contracts.verifier) => {
                        discord = doc.get_str("data").ok().and_then(|data| {
                            FieldElement::from_hex_be(data)
                                .map(|fe| fe.to_string())
                                .ok()
                        })
                    }
                    (
                        "0x00000000000000000000000000000000000000000000000000646973636f7264",
                        verifier,
                    ) if verifier == to_hex(&state.conf.contracts.old_verifier) => {
                        old_discord = doc.get_str("data").ok().and_then(|data| {
                            FieldElement::from_hex_be(data)
                                .map(|fe| fe.to_string())
                                .ok()
                        })
                    }

                    ("0x0000000000000000000000000070726f6f665f6f665f706572736f6e686f6f64", _)
                        if verifier == to_hex(&state.conf.contracts.pop_verifier) =>
                    {
                        // ensure pop is valid
                        proof_of_personhood = doc.get_str("data").ok()
                        .and_then(| data |
                             Some(data == "0x0000000000000000000000000000000000000000000000000000000000000001"));
                    }

                    _ => {}
                }
            }
        }
    }

    let is_owner_main_document = domains
        .find_one(
            doc! {
                "domain": &domain,
                "legacy_address": &owner_addr,
                "rev_address": &owner_addr,
                "_cursor.to": null,
            },
            None,
        )
        .await;
    let is_owner_main = is_owner_main_document.is_ok() && is_owner_main_document.unwrap().is_some();

    let data = Data {
        domain: Some(domain),
        addr,
        domain_expiry: expiry,
        is_owner_main,
        owner_addr,
        github,
        old_github, // added this field
        twitter,
        old_twitter, // added this field
        discord,
        old_discord, // added this field
        proof_of_personhood,
        starknet_id: FieldElement::from_hex_be(&starknet_id).unwrap().to_string(),
    };

    (StatusCode::OK, headers, Json(data)).into_response()
}
