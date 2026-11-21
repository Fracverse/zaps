use axum::{
    extract::{FromRef, State},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use super::feed::AuthUser;

const PAYSTACK_API: &str = "https://api.paystack.co";

#[derive(Clone)]
pub struct BankAccountState {
    pool: PgPool,
    paystack_secret_key: Option<String>,
    client: Client,
}

impl BankAccountState {
    pub fn from_env(pool: PgPool) -> Self {
        Self {
            pool,
            paystack_secret_key: std::env::var("PAYSTACK_SECRET_KEY")
                .ok()
                .filter(|key| !key.trim().is_empty()),
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        }
    }
}

impl FromRef<BankAccountState> for PgPool {
    fn from_ref(state: &BankAccountState) -> Self {
        state.pool.clone()
    }
}

#[derive(Serialize)]
struct Bank {
    name: String,
    code: String,
}

#[derive(Deserialize)]
struct PaystackResponse<T> {
    status: bool,
    message: Option<String>,
    data: Option<T>,
}

#[derive(Deserialize)]
struct PaystackBank {
    name: String,
    code: String,
    #[serde(default = "default_true")]
    active: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Deserialize)]
struct PaystackResolution {
    account_name: String,
    account_number: String,
}

#[derive(Deserialize)]
pub struct VerifyBankAccountRequest {
    bank_code: String,
    account_number: String,
}

#[derive(Serialize)]
struct VerifyBankAccountResponse {
    account_name: String,
}

#[derive(Serialize)]
struct BankAccountResponse {
    bank_account: Option<BankAccount>,
}

#[derive(Serialize)]
struct BankAccount {
    bank_name: String,
    bank_code: String,
    account_name: String,
    account_number_last4: String,
}

#[derive(Serialize)]
struct SavedBankAccountResponse {
    bank_account: BankAccount,
}

pub fn bank_account_routes(state: BankAccountState) -> Router {
    Router::new()
        .route("/", get(get_bank_account).put(save_bank_account))
        .route("/banks", get(get_supported_banks))
        .route("/verify", post(verify_bank_account))
        .with_state(state)
}

fn invalid_input(message: &'static str) -> axum::response::Response {
    (
        axum::http::StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": message })),
    )
        .into_response()
}

fn validate_request(input: &VerifyBankAccountRequest) -> Result<(), &'static str> {
    if input.bank_code.trim().is_empty() || input.bank_code.len() > 20 {
        return Err("Select a supported bank.");
    }
    if input.account_number.len() != 10
        || !input.account_number.chars().all(|digit| digit.is_ascii_digit())
    {
        return Err("Enter a valid 10-digit account number.");
    }
    Ok(())
}

fn configured_key(state: &BankAccountState) -> Result<&str, axum::response::Response> {
    state
        .paystack_secret_key
        .as_deref()
        .ok_or_else(|| {
            (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "Bank account verification is not configured."
                })),
            )
                .into_response()
        })
}

async fn get_supported_banks(
    State(state): State<BankAccountState>,
    _auth: AuthUser,
) -> axum::response::Response {
    let key = match configured_key(&state) {
        Ok(key) => key,
        Err(response) => return response,
    };
    let response = match state
        .client
        .get(format!("{PAYSTACK_API}/bank"))
        .query(&[("country", "nigeria"), ("perPage", "100")])
        .bearer_auth(key)
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            tracing::error!("Bank provider request failed: {error}");
            return provider_error();
        }
    };
    let result = match response.json::<PaystackResponse<Vec<PaystackBank>>>().await {
        Ok(result) if result.status => result,
        Ok(result) => {
            tracing::warn!("Bank provider rejected bank list request: {:?}", result.message);
            return provider_error();
        }
        Err(error) => {
            tracing::error!("Could not parse provider bank list response: {error}");
            return provider_error();
        }
    };
    let banks = result
        .data
        .unwrap_or_default()
        .into_iter()
        .filter(|bank| bank.active)
        .map(|bank| Bank {
            name: bank.name,
            code: bank.code,
        })
        .collect::<Vec<_>>();
    Json(serde_json::json!({ "banks": banks })).into_response()
}

async fn resolve_account(
    state: &BankAccountState,
    input: &VerifyBankAccountRequest,
) -> Result<PaystackResolution, axum::response::Response> {
    validate_request(input).map_err(invalid_input)?;
    let key = configured_key(state)?;
    let response = state
        .client
        .get(format!("{PAYSTACK_API}/bank/resolve"))
        .query(&[
            ("account_number", input.account_number.as_str()),
            ("bank_code", input.bank_code.as_str()),
        ])
        .bearer_auth(key)
        .send()
        .await
        .map_err(|error| {
            tracing::error!("Bank account verification request failed: {error}");
            provider_error()
        })?;
    let status = response.status();
    let result = response
        .json::<PaystackResponse<PaystackResolution>>()
        .await
        .map_err(|error| {
            tracing::error!("Could not parse bank account verification response: {error}");
            provider_error()
        })?;
    if !status.is_success() || !result.status {
        return Err((
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({
                "error": "We couldn't verify this bank account. Check the details and try again."
            })),
        )
            .into_response());
    }
    let account = result.data.ok_or_else(provider_error)?;
    if account.account_name.trim().is_empty() {
        return Err((
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({
                "error": "We couldn't verify this bank account. Check the details and try again."
            })),
        )
            .into_response());
    }
    Ok(account)
}

async fn verify_bank_account(
    State(state): State<BankAccountState>,
    _auth: AuthUser,
    Json(input): Json<VerifyBankAccountRequest>,
) -> axum::response::Response {
    match resolve_account(&state, &input).await {
        Ok(account) => Json(VerifyBankAccountResponse {
            account_name: account.account_name,
        })
        .into_response(),
        Err(response) => response,
    }
}

async fn get_bank_account(
    State(pool): State<PgPool>,
    auth: AuthUser,
) -> axum::response::Response {
    match sqlx::query(
        "SELECT bank_name, bank_code, account_name, RIGHT(account_number, 4) AS account_number_last4 FROM bank_accounts WHERE user_id = $1",
    )
    .bind(auth.id)
    .fetch_optional(&pool)
    .await
    {
        Ok(row) => {
            let bank_account = row.map(|row| BankAccount {
                bank_name: row.get("bank_name"),
                bank_code: row.get("bank_code"),
                account_name: row.get("account_name"),
                account_number_last4: row.get("account_number_last4"),
            });
            Json(BankAccountResponse { bank_account }).into_response()
        }
        Err(error) => {
            tracing::error!("Could not load merchant bank account: {error}");
            database_error()
        }
    }
}

async fn save_bank_account(
    State(state): State<BankAccountState>,
    auth: AuthUser,
    Json(input): Json<VerifyBankAccountRequest>,
) -> axum::response::Response {
    let resolved = match resolve_account(&state, &input).await {
        Ok(account) if account.account_number == input.account_number => account,
        Ok(_) => {
            return (
                axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({ "error": "The bank provider returned mismatched account details." })),
            )
                .into_response()
        }
        Err(response) => return response,
    };

    let bank_name = match bank_name(&state, &input.bank_code).await {
        Ok(name) => name,
        Err(response) => return response,
    };

    match sqlx::query(
        r#"
        INSERT INTO bank_accounts (user_id, bank_name, bank_code, account_name, account_number, verified_at)
        VALUES ($1, $2, $3, $4, $5, NOW())
        ON CONFLICT (user_id) DO UPDATE SET
            bank_name = EXCLUDED.bank_name,
            bank_code = EXCLUDED.bank_code,
            account_name = EXCLUDED.account_name,
            account_number = EXCLUDED.account_number,
            verified_at = NOW(),
            updated_at = NOW()
        RETURNING bank_name, bank_code, account_name, RIGHT(account_number, 4) AS account_number_last4
        "#,
    )
    .bind(auth.id)
    .bind(bank_name)
    .bind(&input.bank_code)
    .bind(resolved.account_name)
    .bind(resolved.account_number)
    .fetch_one(&state.pool)
    .await
    {
        Ok(row) => Json(SavedBankAccountResponse {
            bank_account: BankAccount {
                bank_name: row.get("bank_name"),
                bank_code: row.get("bank_code"),
                account_name: row.get("account_name"),
                account_number_last4: row.get("account_number_last4"),
            },
        })
        .into_response(),
        Err(error) => {
            tracing::error!("Could not save merchant bank account: {error}");
            database_error()
        }
    }
}

async fn bank_name(
    state: &BankAccountState,
    code: &str,
) -> Result<String, axum::response::Response> {
    let key = configured_key(state)?;
    let response = state
        .client
        .get(format!("{PAYSTACK_API}/bank"))
        .query(&[("country", "nigeria"), ("perPage", "100")])
        .bearer_auth(key)
        .send()
        .await
        .map_err(|error| {
            tracing::error!("Could not load bank name for verified account: {error}");
            provider_error()
        })?;
    let result = response
        .json::<PaystackResponse<Vec<PaystackBank>>>()
        .await
        .map_err(|error| {
            tracing::error!("Could not parse bank list for verified account: {error}");
            provider_error()
        })?;
    result
        .data
        .unwrap_or_default()
        .into_iter()
        .find(|bank| bank.code == code)
        .map(|bank| bank.name)
        .ok_or_else(|| {
            (
                axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({ "error": "Select a supported bank." })),
            )
                .into_response()
        })
}

fn provider_error() -> axum::response::Response {
    (
        axum::http::StatusCode::BAD_GATEWAY,
        Json(serde_json::json!({ "error": "Bank provider is temporarily unavailable." })),
    )
        .into_response()
}

fn database_error() -> axum::response::Response {
    (
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": "Unable to save or load bank account details." })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bank_account_number_must_be_ten_ascii_digits() {
        let valid = VerifyBankAccountRequest {
            bank_code: "058".into(),
            account_number: "0123456789".into(),
        };
        assert!(validate_request(&valid).is_ok());

        for account_number in ["123", "12345678901", "12345678a0", "１２３４５６７８９０"] {
            let input = VerifyBankAccountRequest {
                bank_code: "058".into(),
                account_number: account_number.into(),
            };
            assert!(validate_request(&input).is_err());
        }
    }
}
