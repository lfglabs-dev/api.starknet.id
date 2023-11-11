use crate::{
    models::AppState,
    utils::{extract_prefix_and_root, get_error},
};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Json},
};
use mongodb::bson::doc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Serialize)]
pub struct DomainToAddrData {
    addr: String,
    domain_expiry: Option<i64>,
}

#[derive(Deserialize)]
pub struct DomainQuery {
    domain: String,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DomainQuery>,
) -> impl IntoResponse {
    let mut headers: HeaderMap = HeaderMap::new();
    headers.insert("Cache-Control", HeaderValue::from_static("max-age=60"));
    let (prefix, root_domain) = extract_prefix_and_root(query.domain.clone());

    match (&state.conf).reversed_resolvers.get(&root_domain) {
        // custom resolver
        Some(resolver) => {
            let custom_resolutions = state
                .starknetid_db
                .collection::<mongodb::bson::Document>("custom_resolutions");
            match custom_resolutions
                .find_one(
                    doc! {
                        "domain_slice" : prefix,
                        "resolver" : resolver,
                        "field" : "starknet",
                        "_cursor.to": null,
                    },
                    None,
                )
                .await
            {
                Ok(Some(doc)) => {
                    let data = DomainToAddrData {
                        addr: doc.get_str("value").unwrap().to_string(),
                        domain_expiry: None,
                    };
                    (StatusCode::OK, headers, Json(data)).into_response()
                }
                _ => get_error("no target found".to_string()),
            }
        }

        // native resolver
        None => {
            let domains = state
                .starknetid_db
                .collection::<mongodb::bson::Document>("domains");
            let document = domains
                .find_one(
                    doc! {
                        "domain": &query.domain,
                        "_cursor.to": null,
                    },
                    None,
                )
                .await;

            match document {
                Ok(doc) => {
                    if let Some(doc) = doc {
                        let addr = doc.get_str("legacy_address").unwrap_or_default().to_owned();
                        let domain_expiry = doc.get_i64("expiry").ok();
                        let data = DomainToAddrData {
                            addr,
                            domain_expiry,
                        };
                        (StatusCode::OK, headers, Json(data)).into_response()
                    } else {
                        get_error("no address found".to_string())
                    }
                }
                Err(_) => get_error("Error while fetching from database".to_string()),
            }
        }
    }
}
