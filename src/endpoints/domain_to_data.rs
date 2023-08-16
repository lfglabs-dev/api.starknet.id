use crate::{
    models::{AppState, Data},
    utils::get_error,
};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Json},
};
use futures::StreamExt;
use mongodb::bson::{doc, Document};
use serde::Deserialize;
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

    let domains = state.db.collection::<mongodb::bson::Document>("domains");
    let starknet_ids = state
        .db
        .collection::<mongodb::bson::Document>("starknet_ids");

    let domain_document = domains
        .find_one(
            doc! {
                "domain": &query.domain,
                "_chain.valid_to": null,
            },
            None,
        )
        .await;

    let (domain, addr, expiry, starknet_id) = match domain_document {
        Ok(Some(doc)) => {
            let domain = doc.get_str("domain").unwrap_or_default().to_owned();
            let addr = doc.get_str("addr").ok().map(String::from);
            let expiry = doc.get_i32("expiry").ok();
            let id = doc.get_str("token_id").unwrap_or_default().to_owned();
            (domain, addr, expiry, id)
        }
        _ => return get_error("Error while fetching from database".to_string()),
    };

    let owner_document = starknet_ids
        .find_one(
            doc! {
                "token_id": &starknet_id,
                "_chain.valid_to": null,
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
                            "$in": ["113702622229858", "32782392107492722", "28263441981469284"]
                        },
                        "verifier": { "$in": [ &state.conf.contracts.verifier.to_string(), &state.conf.contracts.old_verifier.to_string()] } // modified this to accommodate both verifiers
                    },
                    {
                        "field": "2507652182250236150756610039180649816461897572",
                        "verifier": &state.conf.contracts.pop_verifier.to_string()
                    }
                ],
                "token_id": &starknet_id,
                "_chain.valid_to": null,
            }
        },
        doc! {
            "$group": {
                "_id": { "field": "$field", "verifier": "$verifier" }, // group by both field and verifier
                "data": { "$first": "$data" }
            }
        },
    ];

    let starknet_ids_data = state.db.collection::<Document>("starknet_ids_data");
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
                    ("113702622229858", verifier)
                        if verifier == &state.conf.contracts.verifier.to_string() =>
                    {
                        github = doc.get_str("data").ok().map(String::from)
                    }
                    ("113702622229858", verifier)
                        if verifier == &state.conf.contracts.old_verifier.to_string() =>
                    {
                        old_github = doc.get_str("data").ok().map(String::from)
                    }

                    ("32782392107492722", verifier)
                        if verifier == &state.conf.contracts.verifier.to_string() =>
                    {
                        twitter = doc.get_str("data").ok().map(String::from)
                    }
                    ("32782392107492722", verifier)
                        if verifier == &state.conf.contracts.old_verifier.to_string() =>
                    {
                        old_twitter = doc.get_str("data").ok().map(String::from)
                    }

                    ("28263441981469284", verifier)
                        if verifier == &state.conf.contracts.verifier.to_string() =>
                    {
                        discord = doc.get_str("data").ok().map(String::from)
                    }
                    ("28263441981469284", verifier)
                        if verifier == &state.conf.contracts.old_verifier.to_string() =>
                    {
                        old_discord = doc.get_str("data").ok().map(String::from)
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
                "addr": &owner_addr,
                "rev_addr": &owner_addr,
                "_chain.valid_to": null,
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
        owner_addr,
        github,
        old_github, // added this field
        twitter,
        old_twitter, // added this field
        discord,
        old_discord, // added this field
        proof_of_personhood,
        starknet_id,
    };

    (StatusCode::OK, headers, Json(data)).into_response()
}
