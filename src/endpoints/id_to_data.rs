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
pub struct IdQuery {
    id: String,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<IdQuery>,
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
                "token_id": &query.id,
                "_chain.valid_to": null,
            },
            None,
        )
        .await;

    let domain_data = match domain_document {
        Ok(doc) => {
            if let Some(doc) = doc {
                let domain = doc.get_str("domain").unwrap_or_default().to_owned();
                let addr = doc.get_str("addr").ok().map(String::from);
                let expiry = doc.get_i32("expiry").ok();
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
                "token_id": &query.id,
                "_chain.valid_to": null,
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

    let pipeline = vec![
        doc! {
            "$match": {
                "$or": [
                    {
                        "field": {
                            // utf-8 encoded: github, twitter, discord
                            "$in": ["113702622229858", "32782392107492722", "28263441981469284"]
                        },
                        "verifier": { "$in": [ &state.conf.contracts.verifier.to_string(), &state.conf.contracts.old_verifier.to_string() ] }
                    },
                    {
                        // utf-8 encoded: proof_of_personhood
                        "field": "2507652182250236150756610039180649816461897572",
                        "verifier": &state.conf.contracts.pop_verifier.to_string()
                    }
                ],
                "token_id": &query.id,
                "_chain.valid_to": null,
            }
        },
        doc! {
            "$group": {
                "_id": { "field": "$field", "verifier": "$verifier" },
                "data": { "$first": "$data" }
            }
        },
    ];

    let starknet_ids_data = state.db.collection::<Document>("starknet_ids_data");
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
                    ("113702622229858", v) if v == state.conf.contracts.verifier.to_string() => {
                        github = doc.get_str("data").ok().map(String::from)
                    }
                    ("113702622229858", v)
                        if v == state.conf.contracts.old_verifier.to_string() =>
                    {
                        old_github = doc.get_str("data").ok().map(String::from)
                    }
                    ("32782392107492722", v) if v == state.conf.contracts.verifier.to_string() => {
                        twitter = doc.get_str("data").ok().map(String::from)
                    }
                    ("32782392107492722", v)
                        if v == state.conf.contracts.old_verifier.to_string() =>
                    {
                        old_twitter = doc.get_str("data").ok().map(String::from)
                    }
                    ("28263441981469284", v) if v == state.conf.contracts.verifier.to_string() => {
                        discord = doc.get_str("data").ok().map(String::from)
                    }
                    ("28263441981469284", v)
                        if v == state.conf.contracts.old_verifier.to_string() =>
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
                "addr": &owner,
                "rev_addr": &owner,
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
        owner_addr: owner,
        github,
        twitter,
        discord,
        proof_of_personhood,
        old_github,
        old_twitter,
        old_discord,
        starknet_id: query.id,
    };

    (StatusCode::OK, headers, Json(data)).into_response()
}
