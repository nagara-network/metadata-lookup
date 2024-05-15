#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

pub mod error;
pub mod metadata;

pub use error::{Error, Result};
pub use nagara_logging::{debug, error, info, warn};

type ChainClient = subxt::OnlineClient<subxt::PolkadotConfig>;
type FileOnchainMetadata =
    crate::metadata::api::runtime_types::nagara_pda_files::pallet::FileInformation<
        subxt::utils::AccountId32,
        u128,
    >;

pub const ENV_MAINNET_URL: &str = "MAINNET_URL";
pub const ENV_STORE_KEY: &str = "STORE_KEY";
pub const ENV_STORE_URL: &str = "STORE_URL";
pub const ENV_TESTNET_URL: &str = "TESTNET_URL";
pub const INDEX_MAINNET: &str = "mainnet_files";
pub const INDEX_TESTNET: &str = "testnet_files";

async fn get_chain_client(mainnet: bool, store_vars: &StoreVariables) -> Result<ChainClient> {
    let url = if mainnet {
        store_vars.mainnet_url.clone()
    } else {
        store_vars.testnet_url.clone()
    };
    let chain_client = if url.starts_with("wss://") || url.starts_with("https://") {
        ChainClient::from_url(url).await?
    } else {
        ChainClient::from_insecure_url(url).await?
    };

    Ok(chain_client)
}

#[derive(serde::Deserialize)]
struct QueryParams {
    search: String,
    mainnet: bool,
}

#[derive(serde::Deserialize, serde::Serialize, Clone)]
struct FileOffchainMetadata {
    id: nagara_identities::public::PublicKey,
    filename: String,
    content_type: String,
    uploaded_at: chrono::DateTime<chrono::Utc>,
    download_counter: u64,
    descriptions: String,
}

impl FileOffchainMetadata {
    async fn try_get_full_metadata(&self, chain_client: &ChainClient) -> Result<FileMetadata> {
        let id = subxt::utils::AccountId32(self.id.to_bytes());
        let storage_call = metadata::api::storage().pda_files().files(id);
        let onchain_info = chain_client
            .storage()
            .at_latest()
            .await?
            .fetch(&storage_call)
            .await?
            .ok_or(Error::BadMetadataProcessing)?;
        let file_metadata = FileMetadata::from((self.clone(), onchain_info));

        Ok(file_metadata)
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
struct FileMetadata {
    id: nagara_identities::public::PublicKey,
    uploader: nagara_identities::public::PublicKey,
    big_brother: nagara_identities::public::PublicKey,
    servicer: nagara_identities::public::PublicKey,
    owner: nagara_identities::public::PublicKey,
    #[serde(with = "hex::serde")]
    transfer_fee: [u8; 16],
    #[serde(with = "hex::serde")]
    download_fee: [u8; 16],
    size: u64,
    #[serde(with = "hex::serde")]
    hash: [u8; 32],
    filename: String,
    content_type: String,
    uploaded_at: chrono::DateTime<chrono::Utc>,
    download_counter: u64,
    descriptions: String,
}

impl From<(FileOffchainMetadata, FileOnchainMetadata)> for FileMetadata {
    fn from(value: (FileOffchainMetadata, FileOnchainMetadata)) -> Self {
        let id = value.0.id;
        let uploader =
            nagara_identities::public::PublicKey::try_from(value.1.uploader.0.as_ref()).unwrap();
        let big_brother =
            nagara_identities::public::PublicKey::try_from(value.1.big_brother.0.as_ref()).unwrap();
        let servicer =
            nagara_identities::public::PublicKey::try_from(value.1.servicer.0.as_ref()).unwrap();
        let owner =
            nagara_identities::public::PublicKey::try_from(value.1.owner.0.as_ref()).unwrap();
        let size = value.1.size;
        let hash = value.1.hash;
        let filename = value.0.filename;
        let content_type = value.0.content_type;
        let uploaded_at = value.0.uploaded_at;
        let download_counter = value.0.download_counter;
        let descriptions = value.0.descriptions;
        let transfer_fee = value.1.transfer_fee.to_le_bytes();
        let download_fee = if let Some(download_fee) = value.1.download_fee {
            download_fee.to_le_bytes()
        } else {
            [0; 16]
        };

        Self {
            id,
            uploader,
            big_brother,
            servicer,
            owner,
            transfer_fee,
            download_fee,
            size,
            hash,
            filename,
            content_type,
            uploaded_at,
            download_counter,
            descriptions,
        }
    }
}

#[derive(Clone)]
struct StoreVariables {
    mainnet_url: String,
    store_key: String,
    store_url: String,
    testnet_url: String,
}

impl Default for StoreVariables {
    fn default() -> Self {
        let mainnet_url = std::env::var(ENV_MAINNET_URL).unwrap();
        let store_key = std::env::var(ENV_STORE_KEY).unwrap();
        let store_url = std::env::var(ENV_STORE_URL).unwrap();
        let testnet_url = std::env::var(ENV_TESTNET_URL).unwrap();

        Self {
            mainnet_url,
            store_key,
            store_url,
            testnet_url,
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
) -> actix_web::Result<actix_web::web::Json<Vec<FileMetadata>>> {
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
    let search_results = client
        .index(index)
        .search()
        .with_query(&query_param.search)
        .execute::<FileOffchainMetadata>()
        .await
        .map_err(|_| Error::StoreConnectionBroken)?;
    let offchain_results = search_results
        .hits
        .into_iter()
        .map(|x| x.result)
        .collect::<Vec<FileOffchainMetadata>>();
    let mut full_results = Vec::with_capacity(offchain_results.len());

    if !offchain_results.is_empty() {
        let chain_client = get_chain_client(query_param.mainnet, &store_vars).await?;

        for x in offchain_results {
            let full_metadata = x.try_get_full_metadata(&chain_client).await?;
            full_results.push(full_metadata);
        }
    }

    Ok(actix_web::web::Json(full_results))
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
