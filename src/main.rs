#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

pub mod error;

pub use error::{Error, Result};
pub use nagara_logging::{debug, error, info, warn};

pub const INDEX_TESTNET: &str = "testnet_files";
pub const INDEX_MAINNET: &str = "mainnet_files";
pub const ENV_STORE_KEY: &str = "STORE_KEY";
pub const ENV_STORE_URL: &str = "STORE_URL";

#[derive(serde::Deserialize)]
struct QueryParams {
    search: String,
    mainnet: bool,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct FileOffchainMetadata {
    id: nagara_identities::public::PublicKey,
    filename: String,
    content_type: String,
    length: u64,
    uploader: nagara_identities::public::PublicKey,
    uploaded_at: u32,
    download_counter: u64,
    #[serde(with = "hex::serde")]
    hash: [u8; 32],
    descriptions: String,
}

#[derive(Clone)]
struct StoreVariables {
    store_key: String,
    store_url: String,
}

impl Default for StoreVariables {
    fn default() -> Self {
        let store_key = std::env::var(ENV_STORE_KEY).unwrap();
        let store_url = std::env::var(ENV_STORE_URL).unwrap();

        Self {
            store_key,
            store_url,
        }
    }
}

async fn reject_unmapped_handler() -> impl actix_web::Responder {
    actix_web::HttpResponse::Forbidden().finish()
}

#[actix_web::get("/")]
async fn get_file_info(
    store_vars: actix_web::web::Data<StoreVariables>,
    query_param: actix_web::web::Query<QueryParams>,
) -> actix_web::Result<actix_web::web::Json<Vec<FileOffchainMetadata>>> {
    let client = meilisearch_sdk::client::Client::new(
        store_vars.store_url.clone(),
        Some(store_vars.store_key.clone()),
    )
    .map_err(|_| Error::StoreConnectionBroken)?;
    let index = if query_param.mainnet {
        INDEX_MAINNET
    } else {
        INDEX_TESTNET
    };
    let results = client
        .index(index)
        .search()
        .with_query(&query_param.search)
        .execute::<FileOffchainMetadata>()
        .await
        .map_err(|_| Error::StoreConnectionBroken)?;
    let results = results
        .hits
        .into_iter()
        .map(|x| x.result)
        .collect::<Vec<FileOffchainMetadata>>();

    Ok(actix_web::web::Json(results))
}

#[tokio::main]
async fn main() -> Result<()> {
    nagara_logging::init();

    actix_web::HttpServer::new(move || {
        let cors = actix_cors::Cors::default().allow_any_origin();
        let store_vars = actix_web::web::Data::new(StoreVariables::default());

        actix_web::App::new()
            .app_data(store_vars)
            .wrap(cors)
            .wrap(actix_web::middleware::Logger::default())
            .service(get_file_info)
            .default_service(actix_web::web::route().to(reject_unmapped_handler))
    })
    .bind("0.0.0.0:8686")?
    .run()
    .await?;

    Ok(())
}
