use std::{
    fmt,
    sync::{Arc, RwLock},
};

use alloy::primitives::Address;
use anyhow::{Result, anyhow};
use axum::{
    Json, RequestPartsExt as _, extract::FromRequestParts, http::request::Parts,
    response::IntoResponse,
};
use axum_extra::{
    TypedHeader,
    headers::{Authorization, authorization::Bearer},
};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::state::AppState;

#[derive(thiserror::Error, Debug)]
pub enum PrivyError {
    #[error("missing environment variable: {0}")]
    MissingEnv(&'static str),

    #[error("invalid or missing token")]
    InvalidToken,

    #[error("failed to validate access token: {0}")]
    ValidateAccessTokenError(jsonwebtoken::errors::Error),

    #[error("failed to get user by id: {0}")]
    GetUserByIdRequestError(#[from] reqwest::Error),

    #[error("failed to get user by id: {0}")]
    GetUserByIdFailed(anyhow::Error),

    #[error("failed to parse user data: {0}")]
    ParseUserError(#[from] serde_json::Error),

    #[error("failed to find wallet: {0}")]
    FindWalletError(anyhow::Error),

    #[error("failed to read decoding key: {0}")]
    ReadDecodingKeyError(jsonwebtoken::errors::Error),
}

impl IntoResponse for PrivyError {
    fn into_response(self) -> axum::response::Response {
        let status = match self {
            PrivyError::MissingEnv(_)
            | PrivyError::GetUserByIdRequestError(_)
            | PrivyError::ParseUserError(_)
            | PrivyError::ReadDecodingKeyError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            PrivyError::InvalidToken
            | PrivyError::ValidateAccessTokenError(_)
            | PrivyError::GetUserByIdFailed(_)
            | PrivyError::FindWalletError(_) => StatusCode::BAD_REQUEST,
        };
        let body = Json(json!({
            "error": self.to_string(),
        }));
        (status, body).into_response()
    }
}

#[derive(Clone)]
pub struct PrivyConfig {
    pub app_id: String,
    pub app_secret: String,
    pub verification_key: String,
}

impl fmt::Debug for PrivyConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PrivyConfig")
            .field("app_id", &self.app_id)
            .finish_non_exhaustive()
    }
}

impl PrivyConfig {
    pub fn from_env() -> Result<Self, PrivyError> {
        let app_id =
            std::env::var("PRIVY_APP_ID").map_err(|_| PrivyError::MissingEnv("PRIVY_APP_ID"))?;

        let app_secret = std::env::var("PRIVY_APP_SECRET")
            .map_err(|_| PrivyError::MissingEnv("PRIVY_APP_SECRET"))?;

        let verification_key = std::env::var("PRIVY_VERIFICATION_KEY")
            .map_err(|_| PrivyError::MissingEnv("PRIVY_VERIFICATION_KEY"))?;

        Ok(Self {
            app_id,
            app_secret,
            verification_key,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Privy {
    pub config: PrivyConfig,
    pub client: reqwest::Client,
}

impl Privy {
    #[must_use]
    pub fn new(config: PrivyConfig) -> Self {
        Self {
            config,
            client: Client::new(),
        }
    }

    pub async fn authenticate_user(&self, access_token: &str) -> Result<UserSession, PrivyError> {
        let claims = self.validate_access_token(access_token)?;
        let user = self.get_user_by_id(&claims.user_id).await?;

        let evm_wallet = find_wallet(&user.linked_accounts, "ethereum", "privy")
            .map_err(PrivyError::FindWalletError)?;
        let wallet = Address::parse_checksummed(&evm_wallet.address, None)
            .map_err(|err| PrivyError::FindWalletError(err.into()))?;

        Ok(UserSession {
            user_id: user.id,
            session_id: claims.session_id,
            wallet,
        })
    }

    pub fn validate_access_token(&self, access_token: &str) -> Result<PrivyClaims, PrivyError> {
        let mut validation = Validation::new(Algorithm::ES256);
        validation.set_issuer(&["privy.io"]);
        validation.set_audience(&[&self.config.app_id]);

        let key = DecodingKey::from_ec_pem(self.config.verification_key.as_bytes())
            .map_err(PrivyError::ReadDecodingKeyError)?;

        let token_data = decode::<PrivyClaims>(access_token, &key, &validation)
            .map_err(PrivyError::ValidateAccessTokenError)?;

        Ok(token_data.claims)
    }

    pub async fn get_user_by_id(&self, user_id: &str) -> Result<User, PrivyError> {
        let url = format!("https://auth.privy.io/api/v1/users/{user_id}");

        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(PrivyError::GetUserByIdRequestError)?;

        if !response.status().is_success() {
            return Err(PrivyError::GetUserByIdFailed(anyhow!(
                "Failed to get user data: {}",
                response.status()
            )));
        }
        let text = response.text().await?;
        Ok(serde_json::from_str(&text)?)
    }
}

#[derive(Clone)]
pub struct UserSession {
    pub user_id: String,
    pub session_id: String,
    pub wallet: Address,
}

impl FromRequestParts<Arc<RwLock<AppState>>> for UserSession {
    type Rejection = PrivyError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<RwLock<AppState>>,
    ) -> Result<Self, Self::Rejection> {
        // Extract the token from the authorization header
        let TypedHeader(Authorization(bearer)) = parts
            .extract::<TypedHeader<Authorization<Bearer>>>()
            .await
            .map_err(|_| PrivyError::InvalidToken)?;
        // validate token
        let privy = {
            state
                .read()
                .expect("lock should not be poisoned")
                .privy
                .clone() // this is relatively cheap to clone
        };
        privy.authenticate_user(bearer.token()).await
    }
}

#[derive(Serialize)]
pub struct SignAndSendEvmTransactionRequest {
    pub address: String,
    pub chain_type: String, // Always "ethereum"
    pub method: String,     // Always "eth_sendTransaction"
    pub caip2: String,      // Format: "eip155:{chain_id}"
    pub params: SignAndSendEvmTransactionParams,
}

#[derive(Serialize)]
pub struct SignAndSendEvmTransactionParams {
    pub transaction: serde_json::Value,
}

// Request types for signing transactions
#[derive(Serialize)]
pub struct SignAndSendTransactionRequest {
    pub address: String,
    pub chain_type: String,
    pub method: String,
    pub caip2: String,
    pub params: SignAndSendTransactionParams,
}

#[derive(Serialize)]
pub struct SignAndSendTransactionParams {
    pub transaction: String,
    pub encoding: String,
}

#[derive(Deserialize)]
pub struct SignAndSendTransactionResponse {
    pub method: String,
    pub data: SignAndSendTransactionData,
}

#[derive(Deserialize)]
pub struct SignAndSendTransactionData {
    pub hash: String,
    pub caip2: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PrivyClaims {
    #[serde(rename = "aud")]
    pub(crate) app_id: String,
    #[serde(rename = "exp")]
    pub(crate) expiration: i64,
    #[serde(rename = "iss")]
    pub(crate) issuer: String,
    #[serde(rename = "sub")]
    pub(crate) user_id: String,
    #[serde(rename = "iat")]
    pub(crate) issued_at: i64,
    #[serde(rename = "sid")]
    pub(crate) session_id: String,
}

#[derive(Serialize, Deserialize)]
pub struct User {
    pub created_at: i64,
    pub has_accepted_terms: bool,
    pub id: String,
    pub is_guest: bool,
    pub linked_accounts: Vec<LinkedAccount>,
    pub mfa_methods: Vec<serde_json::Value>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum LinkedAccount {
    #[serde(rename = "email")]
    Email(EmailAccount),

    #[serde(rename = "wallet")]
    Wallet(Box<WalletAccount>),

    Unknown(serde_json::Map<String, serde_json::Value>),
}

#[derive(Serialize, Deserialize)]
pub struct EmailAccount {
    pub address: String,
    pub first_verified_at: u64,
    pub latest_verified_at: u64,
    pub verified_at: u64,
}

#[derive(Serialize, Deserialize)]
pub struct WalletAccount {
    pub address: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain_id: Option<String>, // Can be either "eip155:1" or "solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp" format
    pub chain_type: String, // Can be "ethereum" or "solana"
    pub connector_type: String,
    pub first_verified_at: u64,
    pub latest_verified_at: u64,
    pub verified_at: u64,
    pub wallet_client: String,
    pub wallet_client_type: String,
    // Optional fields
    #[serde(default)]
    pub delegated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imported: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery_method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wallet_index: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

fn find_wallet<'a>(
    linked_accounts: &'a [LinkedAccount],
    chain_type: &str,
    wallet_client: &str,
) -> Result<&'a WalletAccount> {
    linked_accounts
        .iter()
        .find_map(|account| match account {
            LinkedAccount::Wallet(wallet) => {
                if wallet.delegated
                    && wallet.chain_type == chain_type
                    && wallet.wallet_client == wallet_client
                {
                    Some(wallet.as_ref())
                } else {
                    None
                }
            }
            _ => None,
        })
        .ok_or_else(|| anyhow!("could not find a delegated {} wallet", chain_type))
}
