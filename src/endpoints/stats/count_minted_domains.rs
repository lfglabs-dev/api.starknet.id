use crate::{models::AppState, utils::get_error};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    Json,
};
use axum_auto_routes::route;
use mongodb::bson::{doc, Bson};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Serialize)]
pub struct CountMintedDomainsData {
    count: u64,
}

#[derive(Deserialize)]
pub struct CountDomainsQuery {
    since: i64,
}

#[route(get, "/stats/count_minted_domains", crate::endpoints::stats::count_minted_domains)]
pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<CountDomainsQuery>,
) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert("Cache-Control", HeaderValue::from_static("max-age=60"));

    let domain_collection = state
        .starknetid_db
        .collection::<mongodb::bson::Document>("domains");
    let filter = doc! {
        "creation_date": { "$gte": query.since },
        "$or": [
            { "_cursor.to": { "$exists": false } },
            { "_cursor.to": Bson::Null },
        ],
    };

    let total = domain_collection.count_documents(filter, None).await;

    match total {
        Ok(count) => {
            let response_data = CountMintedDomainsData { count };
            (StatusCode::OK, headers, Json(response_data)).into_response()
        }
        Err(e) => get_error(format!("Error while fetching from database: {:?}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{models::AppState, config::Config, states::States, offchain::OffchainResolver, logger::Logger};
    use axum::{body::Body, http::StatusCode};
    use axum_test::TestClient;
    use mongodb::{bson::doc, options::ClientOptions, Client, Database};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    async fn setup_test_database() -> Database {
        let client_uri = "mongodb://localhost:27017";
        let options = ClientOptions::parse(client_uri).await.unwrap();
        let client = Client::with_options(options).unwrap();
        let db = client.database("test_db");

        db.collection("domains").drop(None).await.ok();

        db
    }

    async fn setup_app_state() -> Arc<AppState> {
        let starknetid_db = setup_test_database().await;
        let sales_db = setup_test_database().await;
        let free_domains_db = setup_test_database().await;

        Arc::new(AppState {
            conf: Config::default(),
            starknetid_db,
            sales_db,
            free_domains_db,
            states: States::default(),
            dynamic_offchain_resolvers: Arc::new(Mutex::new(HashMap::new())),
            logger: Logger::new("test"),
        })
    }

    #[tokio::test]
    async fn test_count_minted_domains() {
        let app_state = setup_app_state().await;

        let domains_collection = app_state.starknetid_db.collection("domains");

        let test_domains = vec![
            doc! { "creation_date": chrono::Utc::now().timestamp() - 1000, "expiry": chrono::Utc::now().timestamp() - 500 }, // Expired domain
            doc! { "creation_date": chrono::Utc::now().timestamp() - 2000, "expiry": chrono::Utc::now().timestamp() + 500 }, // Active domain
            doc! { "creation_date": chrono::Utc::now().timestamp() - 3000, "expiry": chrono::Utc::now().timestamp() - 100 }, // Expired domain
        ];

        domains_collection.insert_many(test_domains, None).await.unwrap();

        let app = crate::start_app(app_state.clone()).await;
        let client = TestClient::new(app);

        let response = client
            .get("/stats/count_minted_domains?since=0")
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["count"].as_u64().unwrap(), 3, "All minted domains should be counted, including expired ones.");
    }
}