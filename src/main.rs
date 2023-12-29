#![recursion_limit = "256"]

mod config;
mod endpoints;
mod models;
mod resolving;
mod tax;
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
    println!("starknetid_server: starting v{}", env!("CARGO_PKG_VERSION"));
    let conf = config::load();

    let starknetid_client_options =
        ClientOptions::parse(&conf.databases.starknetid.connection_string)
            .await
            .unwrap();

    let sales_client_options = ClientOptions::parse(&conf.databases.sales.connection_string)
        .await
        .unwrap();

    let states = tax::sales_tax::load_sales_tax().await;
    if states.states.is_empty() {
        println!("error: unable to load sales tax");
        return;
    }

    let shared_state = Arc::new(models::AppState {
        conf: conf.clone(),
        starknetid_db: Client::with_options(starknetid_client_options)
            .unwrap()
            .database(&conf.databases.starknetid.name),
        sales_db: Client::with_options(sales_client_options)
            .unwrap()
            .database(&conf.databases.sales.name),
        states,
    });
    // we will know by looking at the log number which db has an issue
    for db in [&shared_state.starknetid_db, &shared_state.sales_db] {
        if db.run_command(doc! {"ping": 1}, None).await.is_err() {
            println!("error: unable to connect to a database");
            return;
        } else {
            println!("database: connected")
        }
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
        .route("/domain_to_data", get(endpoints::domain_to_data::handler))
        .route("/id_to_data", get(endpoints::id_to_data::handler))
        .route("/uri", get(endpoints::uri::handler))
        .route(
            "/referral/add_click",
            post(endpoints::referral::add_click::handler),
        )
        .route(
            "/referral/revenue",
            get(endpoints::referral::revenue::handler),
        )
        .route(
            "/referral/sales_count",
            get(endpoints::referral::sales_count::handler),
        )
        .route(
            "/referral/click_count",
            get(endpoints::referral::click_count::handler),
        )
        .route(
            "/stats/count_addrs",
            get(endpoints::stats::count_addrs::handler),
        )
        .route(
            "/stats/count_club_domains",
            get(endpoints::stats::count_club_domains::handler),
        )
        .route(
            "/stats/count_domains",
            get(endpoints::stats::count_domains::handler),
        )
        .route(
            "/stats/count_ids",
            get(endpoints::stats::count_ids::handler),
        )
        .route(
            "/stats/count_created",
            get(endpoints::stats::count_created::handler),
        )
        .route(
            "/stats/expired_club_domains",
            get(endpoints::stats::expired_club_domains::handler),
        )
        .route(
            "/stats/count_renewed",
            get(endpoints::stats::count_renewed::handler),
        )
        .route(
            "/starkscan/fetch_nfts",
            get(endpoints::starkscan::fetch_nfts::handler),
        )
        .route(
            "/renewal/get_renewal_data",
            get(endpoints::renewal::get_renewal_data::handler),
        )
        .route(
            "/renewal/get_metahash",
            get(endpoints::renewal::get_metahash::handler),
        )
        .route(
            "/renewal/get_non_subscribed_domains",
            get(endpoints::renewal::get_non_subscribed_domains::handler),
        )
        .route("/galxe/verify", post(endpoints::galxe::verify::handler))
        .route(
            "/crosschain/solana/claim",
            post(endpoints::crosschain::solana::claim::handler),
        )
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
