use crate::{models::AppState, utils::get_error};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Json},
};
use futures::StreamExt;
use mongodb::bson::{doc, Document};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Serialize)]
pub struct Data {
    domain: String,
    addr: Option<String>,
    domain_expiry: Option<i32>,
    is_owner_main: bool,
    owner_addr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    github: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    twitter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    discord: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    proof_of_personhood: Option<String>,
    starknet_id: String, // Added the starknet_id field
}

// Struct for extracting the domain query parameter.
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
                "token_id": &starknet_id, // using starknet_id
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
                        "verifier": &state.conf.contracts.verifier.to_string()
                    },
                    {
                        "field": "2507652182250236150756610039180649816461897572",
                        "verifier": &state.conf.contracts.pop_verifier.to_string()
                    }
                ],
                "token_id": &starknet_id, // using starknet_id
                "_chain.valid_to": null,
            }
        },
        doc! {
            "$group": {
                "_id": "$field",
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

    if let Ok(mut cursor) = results {
        while let Some(result) = cursor.next().await {
            if let Ok(doc) = result {
                match doc.get_str("_id") {
                    Ok("113702622229858") => github = doc.get_str("data").ok().map(String::from),
                    Ok("32782392107492722") => twitter = doc.get_str("data").ok().map(String::from),
                    Ok("28263441981469284") => discord = doc.get_str("data").ok().map(String::from),
                    Ok("2507652182250236150756610039180649816461897572") => {
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
        twitter,
        discord,
        proof_of_personhood,
        starknet_id,
    };

    (StatusCode::OK, headers, Json(data)).into_response()
}
