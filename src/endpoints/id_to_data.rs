use crate::{
    models::{AppState, Data},
    utils::{get_error, to_hex},
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
use std::sync::Arc;

#[derive(Deserialize)]
pub struct IdQuery {
    id: FieldElement,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<IdQuery>,
) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert("Cache-Control", HeaderValue::from_static("max-age=30"));

    let domains = state.db.collection::<mongodb::bson::Document>("domains");
    let starknet_ids = state.db.collection::<mongodb::bson::Document>("id_owners");

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
                let addr = doc.get_str("legacy_address").ok().map(String::from);
                let expiry = doc.get_i64("expiry").ok();
                Some((domain, addr, expiry))
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

    if domain_data.is_none() || owner.is_none() {
        return get_error("no domain associated to this starknet id was found".to_string());
    }

    let (domain, addr, expiry) = domain_data.unwrap();
    let owner = owner.unwrap();
    println!("hex_id: {}", hex_id);
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
                        "field": "2507652182250236150756610039180649816461897572",
                        "verifier": to_hex(&state.conf.contracts.pop_verifier)
                    }
                ],
                "id": &hex_id,
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

    let starknet_ids_data = state.db.collection::<Document>("id_verifier_data");
    let results = starknet_ids_data.aggregate(pipeline, None).await;

    let mut github = None;
    let mut twitter = None;
    let mut discord = None;
    let mut proof_of_personhood = None;
    let mut old_github = None;
    let mut old_twitter = None;
    let mut old_discord = None;

    if let Ok(mut cursor) = results {
        while let Some(result) = cursor.next().await {
            if let Ok(doc) = result {
                println!("doc: {}", doc);

                let field = doc
                    .get_document("_id")
                    .unwrap()
                    .get_str("field")
                    .unwrap_or_default();
                let verifier = doc
                    .get_document("_id")
                    .unwrap()
                    .get_str("verifier")
                    .unwrap_or_default();
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

                    ("2507652182250236150756610039180649816461897572", _) => {
                        proof_of_personhood = doc.get_str("data").ok().map(String::from)
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
                "legacy_address": &owner,
                "rev_address": &owner,
                "_cursor.to": null,
            },
            None,
        )
        .await;
    let is_owner_main = is_owner_main_document.is_ok() && is_owner_main_document.unwrap().is_some();
    let data = Data {
        domain,
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
    };

    (StatusCode::OK, headers, Json(data)).into_response()
}
