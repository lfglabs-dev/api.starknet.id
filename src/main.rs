mod config;
mod endpoints;
mod models;
mod utils;
use axum::{
    http::StatusCode,
    routing::{get, post},
    Router,
};
use mongodb::{bson::doc, options::ClientOptions, Client};
use std::net::SocketAddr;
use std::sync::Arc;

use tower_http::cors::{Any, CorsLayer};

#[tokio::main]
async fn main() {
    println!("quest_server: starting v{}", env!("CARGO_PKG_VERSION"));
    let conf = config::load();
    let client_options = ClientOptions::parse(&conf.database.connection_string)
        .await
        .unwrap();

    let shared_state = Arc::new(models::AppState {
        conf: conf.clone(),
        db: Client::with_options(client_options)
            .unwrap()
            .database(&conf.database.name),
    });
    if shared_state
        .db
        .run_command(doc! {"ping": 1}, None)
        .await
        .is_err()
    {
        println!("error: unable to connect to database");
        return;
    } else {
        println!("database: connected")
    }

    let cors = CorsLayer::new().allow_headers(Any).allow_origin(Any);
    let app = Router::new()
        .route("/", get(root))
        .route(
            "/addr_to_available_ids",
            get(endpoints::addr_to_available_ids::handler),
        )
        .route("/addr_to_domain", get(endpoints::addr_to_domain::handler))
        .route(
            "/addr_to_external_domains",
            get(endpoints::addr_to_external_domains::handler),
        )
        .route(
            "/addr_to_full_ids",
            get(endpoints::addr_to_full_ids::handler),
        )
        .route(
            "/addr_to_token_id",
            get(endpoints::addr_to_token_id::handler),
        )
        .route(
            "/addrs_to_domains",
            post(endpoints::addrs_to_domains::handler),
        )
        .route("/data_to_ids", get(endpoints::data_to_ids::handler))
        .route("/domain_to_addr", get(endpoints::domain_to_addr::handler))
        .route("/id_to_data", get(endpoints::id_to_data::handler))
        .with_state(shared_state)
        .layer(cors);

    let addr = SocketAddr::from(([0, 0, 0, 0], conf.server.port));
    println!("server: listening on http://0.0.0.0:{}", conf.server.port);
    axum::Server::bind(&addr)
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .unwrap();
}

async fn root() -> (StatusCode, String) {
    (
        StatusCode::ACCEPTED,
        format!("starknetid_server v{}", env!("CARGO_PKG_VERSION")),
    )
}
