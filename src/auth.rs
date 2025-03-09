use std::{sync::LazyLock, time::Duration};

use alloy::{
    dyn_abi::TypedData,
    primitives::{Address, keccak256},
    signers::Signature,
    sol,
    sol_types::eip712_domain,
};
use anyhow::anyhow;
use axum::{
    Json, RequestPartsExt as _, debug_handler,
    extract::FromRequestParts,
    http::{StatusCode, request::Parts},
    response::IntoResponse,
};
use axum_extra::{
    TypedHeader,
    headers::{Authorization, authorization::Bearer},
};
use chrono::Utc;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::AppError;

pub static KEYS: LazyLock<Keys> = LazyLock::new(|| {
    let secret = std::env::var("JWT_SECRET").expect("JWT_SECRET must be set");
    Keys::new(secret.as_bytes())
});

#[debug_handler]
pub async fn authorize(Json(payload): Json<AuthPayload>) -> Result<Json<AuthBody>, AppError> {
    let domain = eip712_domain! {
        name: "Pokerd",
        version: "1",
        salt: keccak256("monad-hackathon")
    };
    let typed_data = TypedData::from_struct(&payload.login_data, Some(domain));
    let hash = typed_data
        .eip712_signing_hash()
        .expect("login data should be hashable");
    let Ok(address) = payload.signature.recover_address_from_prehash(&hash) else {
        return Err(anyhow!("could not verify signature").into());
    };
    let now = Utc::now();
    if payload.login_data.timestamp < (now - Duration::from_secs(30)).timestamp() {
        return Err(anyhow!("signature is too old").into());
    }
    if payload.login_data.timestamp > now.timestamp() {
        return Err(anyhow!("timestamp is in the future").into());
    }
    let claims = Claims {
        address,
        exp: (now + Duration::from_secs(86400)).timestamp(),
    };
    let token = encode(&Header::default(), &claims, &KEYS.encoding)
        .map_err(|_| AuthError::TokenCreation)?;
    Ok(Json(AuthBody::new(token)))
}

pub struct Keys {
    encoding: EncodingKey,
    decoding: DecodingKey,
}

impl Keys {
    #[must_use]
    pub fn new(secret: &[u8]) -> Self {
        Self {
            encoding: EncodingKey::from_secret(secret),
            decoding: DecodingKey::from_secret(secret),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AuthBody {
    access_token: String,
    token_type: String,
}

impl AuthBody {
    #[must_use]
    pub fn new(access_token: String) -> Self {
        Self {
            access_token,
            token_type: "Bearer".to_string(),
        }
    }
}

sol! {
    #[derive(Debug, Serialize, Deserialize)]
    struct LoginData {
        int64 timestamp;
    }
}

#[derive(Debug, Deserialize)]
pub struct AuthPayload {
    login_data: LoginData,
    signature: Signature,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub address: Address,
    pub exp: i64,
}

impl<S> FromRequestParts<S> for Claims
where
    S: Send + Sync,
{
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Extract the token from the authorization header
        let TypedHeader(Authorization(bearer)) = parts
            .extract::<TypedHeader<Authorization<Bearer>>>()
            .await
            .map_err(|_| AuthError::InvalidToken)?;
        // Decode the user data
        let token_data = decode::<Claims>(bearer.token(), &KEYS.decoding, &Validation::default())
            .map_err(|_| AuthError::InvalidToken)?;

        Ok(token_data.claims)
    }
}

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum AuthError {
    #[error("invalid token")]
    InvalidToken,

    #[error("token creation error")]
    TokenCreation,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> axum::response::Response {
        let status = match self {
            AuthError::InvalidToken => StatusCode::BAD_REQUEST,
            AuthError::TokenCreation => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let body = Json(json!({
            "error": self.to_string(),
        }));
        (status, body).into_response()
    }
}
