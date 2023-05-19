use crate::{models::AppState, utils::get_error};
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
    domain_expiry: Option<i32>,
}

#[derive(Deserialize)]
pub struct DomainQuery {
    domain: String,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DomainQuery>,
) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert("Cache-Control", HeaderValue::from_static("max-age=60"));

    if query.domain.ends_with(".braavos.stark") || query.domain.ends_with(".xplorer.stark") {
        let subdomains = state.db.collection::<mongodb::bson::Document>("subdomains");
        let document = subdomains
            .find_one(
                doc! {
                    "domain": &query.domain,
                    "_chain.valid_to": null,
                },
                None,
            )
            .await;

        match document {
            Ok(doc) => {
                if let Some(doc) = doc {
                    let addr = doc.get_str("addr").unwrap_or_default().to_owned();
                    let data = DomainToAddrData {
                        addr,
                        domain_expiry: None,
                    };
                    (StatusCode::OK, headers, Json(data)).into_response()
                } else {
                    get_error("no address found".to_string())
                }
            }
            Err(_) => get_error("Error while fetching from database".to_string()),
        }
    } else {
        let domains = state.db.collection::<mongodb::bson::Document>("domains");
        let document = domains
            .find_one(
                doc! {
                    "domain": &query.domain,
                    "_chain.valid_to": null,
                },
                None,
            )
            .await;

        match document {
            Ok(doc) => {
                if let Some(doc) = doc {
                    let addr = doc.get_str("addr").unwrap_or_default().to_owned();
                    let domain_expiry = doc.get_i32("expiry").ok();
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
