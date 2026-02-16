use anyhow::{Context, Result};
use serde::Deserialize;
use tracing::info;

#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub refresh_token: Option<String>,
    pub scope: String,
}

/// Exchange an authorization code for an access token.
///
/// This calls Discord's OAuth2 token endpoint.
/// You need to provide your app's client_id, client_secret,
/// and the code returned from the AUTHORIZE IPC command.
pub async fn exchange_code(
    client_id: &str,
    client_secret: &str,
    code: &str,
) -> Result<TokenResponse> {
    let client = reqwest::Client::new();

    let resp = client
        .post("https://discord.com/api/oauth2/token")
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("client_id", client_id),
            ("client_secret", client_secret),
            // For IPC apps, redirect_uri doesn't really matter but Discord requires it
            ("redirect_uri", "https://localhost"),
        ])
        .send()
        .await
        .context("Failed to reach Discord token endpoint")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Token exchange failed ({}): {}", status, body);
    }

    let token: TokenResponse = resp
        .json()
        .await
        .context("Failed to parse token response")?;

    info!("Got access token (expires in {}s)", token.expires_in);
    Ok(token)
}

/// Refresh an expired access token using a refresh token.
pub async fn refresh_token(
    client_id: &str,
    client_secret: &str,
    refresh_token: &str,
) -> Result<TokenResponse> {
    let client = reqwest::Client::new();

    let resp = client
        .post("https://discord.com/api/oauth2/token")
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", client_id),
            ("client_secret", client_secret),
        ])
        .send()
        .await
        .context("Failed to reach Discord token endpoint")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Token refresh failed ({}): {}", status, body);
    }

    let token: TokenResponse = resp
        .json()
        .await
        .context("Failed to parse token refresh response")?;

    info!("Refreshed access token (expires in {}s)", token.expires_in);
    Ok(token)
}
