use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Query, Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::cookie::{Cookie, PrivateCookieJar, SameSite};
use cookie::time::Duration;
use openidconnect::core::{
    CoreAuthDisplay, CoreAuthPrompt, CoreAuthenticationFlow, CoreErrorResponseType,
    CoreGenderClaim, CoreJsonWebKey, CoreJsonWebKeySet, CoreJweContentEncryptionAlgorithm,
    CoreJwsSigningAlgorithm, CoreProviderMetadata, CoreRevocableToken, CoreRevocationErrorResponse,
    CoreTokenIntrospectionResponse, CoreTokenType,
};
use openidconnect::{
    AdditionalClaims, AuthorizationCode, ClaimsVerificationError, Client, ClientId, ClientSecret,
    CsrfToken, EmptyExtraTokenFields, EndpointMaybeSet, EndpointNotSet, EndpointSet, IdTokenFields,
    IssuerUrl, Nonce, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope,
    SignatureVerificationError, StandardErrorResponse, StandardTokenResponse, TokenResponse,
};
use serde::{Deserialize, Serialize};

use crate::App;

const FLOW_COOKIE: &str = "oidc_flow";
const SESSION_COOKIE: &str = "session";
const SESSION_TTL_SECS: u64 = 60 * 60;
const FLOW_TTL_SECS: u64 = 10 * 60;

/// Captures every non-standard ID-token claim so the admin claim is
/// addressable by its configured name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllClaims {
    #[serde(flatten)]
    pub claims: serde_json::Map<String, serde_json::Value>,
}
impl AdditionalClaims for AllClaims {}

type AllIdTokenFields = IdTokenFields<
    AllClaims,
    EmptyExtraTokenFields,
    CoreGenderClaim,
    CoreJweContentEncryptionAlgorithm,
    CoreJwsSigningAlgorithm,
>;
type AllTokenResponse = StandardTokenResponse<AllIdTokenFields, CoreTokenType>;
type OidcClient = Client<
    AllClaims,
    CoreAuthDisplay,
    CoreGenderClaim,
    CoreJweContentEncryptionAlgorithm,
    CoreJsonWebKey,
    CoreAuthPrompt,
    StandardErrorResponse<CoreErrorResponseType>,
    AllTokenResponse,
    CoreTokenIntrospectionResponse,
    CoreRevocableToken,
    CoreRevocationErrorResponse,
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointMaybeSet,
    EndpointMaybeSet,
>;

const JWKS_REFRESH_MIN_INTERVAL_SECS: u64 = 60;

pub struct Oidc {
    client: RwLock<Arc<OidcClient>>,
    http: openidconnect::reqwest::Client,
    metadata: CoreProviderMetadata,
    client_id: ClientId,
    client_secret: ClientSecret,
    redirect: RedirectUrl,
    last_refresh: AtomicU64,
}

fn build_client(
    metadata: CoreProviderMetadata,
    client_id: &ClientId,
    client_secret: &ClientSecret,
    redirect: &RedirectUrl,
) -> OidcClient {
    Client::from_provider_metadata(metadata, client_id.clone(), Some(client_secret.clone()))
        .set_redirect_uri(redirect.clone())
}

impl Oidc {
    pub async fn discover(
        issuer: &str,
        client_id: &str,
        client_secret: &str,
        public_url: &str,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let issuer_url = IssuerUrl::new(issuer.to_string())?;
        // No redirects: token/discovery endpoints must answer directly.
        // TODO: no redirect policy needed?
        let http = openidconnect::reqwest::ClientBuilder::new()
            .redirect(openidconnect::reqwest::redirect::Policy::none())
            .build()?;
        let metadata = CoreProviderMetadata::discover_async(issuer_url, &http).await?;
        let client_id = ClientId::new(client_id.to_string());
        let client_secret = ClientSecret::new(client_secret.to_string());
        let redirect = RedirectUrl::new(format!(
            "{}/auth/callback",
            public_url.trim_end_matches('/')
        ))?;
        let client = build_client(metadata.clone(), &client_id, &client_secret, &redirect);
        Ok(Self {
            client: RwLock::new(Arc::new(client)),
            http,
            metadata,
            client_id,
            client_secret,
            redirect,
            last_refresh: AtomicU64::new(0),
        })
    }

    fn client(&self) -> Arc<OidcClient> {
        self.client.read().unwrap().clone()
    }

    /// Refetches the provider's JWKS after a signing-key rotation, rebuilding the
    /// client with the fresh keys. Rate-limited so unverifiable tokens can't drive
    /// fetches against the IdP.
    async fn refresh_keys(&self) {
        let now = now_secs();
        let last = self.last_refresh.load(Ordering::Relaxed);
        if now.saturating_sub(last) < JWKS_REFRESH_MIN_INTERVAL_SECS
            || self
                .last_refresh
                .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
                .is_err()
        {
            return;
        }

        match CoreJsonWebKeySet::fetch_async(self.metadata.jwks_uri(), &self.http).await {
            Ok(jwks) => {
                let metadata = self.metadata.clone().set_jwks(jwks);
                let client = build_client(
                    metadata,
                    &self.client_id,
                    &self.client_secret,
                    &self.redirect,
                );
                *self.client.write().unwrap() = Arc::new(client);
                tracing::info!("refreshed OIDC signing keys");
            }
            Err(err) => tracing::warn!(error = %err, "JWKS refresh failed"),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct FlowState {
    csrf: String,
    nonce: String,
    verifier: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub sub: String,
    pub email: Option<String>,
    exp: u64,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn cookie(name: &'static str, value: String, secure: bool, ttl_secs: u64) -> Cookie<'static> {
    Cookie::build((name, value))
        .http_only(true)
        .secure(secure)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(Duration::seconds(ttl_secs as i64))
        .build()
}

type AuthError = (StatusCode, &'static str);

fn claim_contains(
    map: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    required: &str,
) -> bool {
    match map.get(key) {
        Some(serde_json::Value::Array(values)) => {
            values.iter().any(|v| v.as_str() == Some(required))
        }
        Some(serde_json::Value::String(value)) => value == required,
        _ => false,
    }
}

pub async fn login(State(state): State<App>, jar: PrivateCookieJar) -> Response {
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
    let (auth_url, csrf, nonce) = state
        .oidc
        .client()
        .authorize_url(
            CoreAuthenticationFlow::AuthorizationCode,
            CsrfToken::new_random,
            Nonce::new_random,
        )
        .add_scope(Scope::new("profile".to_string()))
        .add_scope(Scope::new("email".to_string()))
        .add_scope(Scope::new("groups".to_string()))
        .set_pkce_challenge(pkce_challenge)
        .url();

    let flow = FlowState {
        csrf: csrf.secret().clone(),
        nonce: nonce.secret().clone(),
        verifier: pkce_verifier.secret().clone(),
    };
    let value = serde_json::to_string(&flow).unwrap();
    let jar = jar.add(cookie(
        FLOW_COOKIE,
        value,
        state.cookie_secure,
        FLOW_TTL_SECS,
    ));
    (jar, Redirect::to(auth_url.as_str())).into_response()
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    code: String,
    state: String,
}

pub async fn callback(
    State(state): State<App>,
    jar: PrivateCookieJar,
    Query(query): Query<CallbackQuery>,
) -> Response {
    let Some(flow) = jar
        .get(FLOW_COOKIE)
        .and_then(|c| serde_json::from_str::<FlowState>(c.value()).ok())
    else {
        return (StatusCode::BAD_REQUEST, "missing or invalid flow cookie").into_response();
    };
    let jar = jar.remove(FLOW_COOKIE);
    match authenticate(&state, flow, query).await {
        Ok(session) => {
            let value = serde_json::to_string(&session).unwrap();
            let jar = jar.add(cookie(
                SESSION_COOKIE,
                value,
                state.cookie_secure,
                SESSION_TTL_SECS,
            ));
            (jar, Redirect::to("/")).into_response()
        }
        Err(err) => (jar, err).into_response(),
    }
}

async fn authenticate(
    state: &App,
    flow: FlowState,
    query: CallbackQuery,
) -> Result<Session, AuthError> {
    if flow.csrf != query.state {
        return Err((StatusCode::BAD_REQUEST, "state mismatch"));
    }

    let client = state.oidc.client();
    let token = client
        .exchange_code(AuthorizationCode::new(query.code))
        .map_err(|e| {
            tracing::error!(error = %e, "token endpoint not configured");
            (StatusCode::INTERNAL_SERVER_ERROR, "internal error")
        })?
        .set_pkce_verifier(PkceCodeVerifier::new(flow.verifier))
        .request_async(&state.oidc.http)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "code exchange failed");
            (StatusCode::UNAUTHORIZED, "code exchange failed")
        })?;

    let id_token = token
        .id_token()
        .ok_or((StatusCode::UNAUTHORIZED, "no ID token in response"))?;
    let nonce = Nonce::new(flow.nonce);
    let claims = match id_token.claims(&client.id_token_verifier(), &nonce) {
        Ok(claims) => claims,
        // An unknown signing key means the IdP most likely rotated keys since
        // discovery; refetch the JWKS and verify once more.
        Err(ClaimsVerificationError::SignatureVerification(
            SignatureVerificationError::NoMatchingKey,
        )) => {
            state.oidc.refresh_keys().await;
            let client = state.oidc.client();
            id_token
                .claims(&client.id_token_verifier(), &nonce)
                .map_err(|e| {
                    tracing::warn!(error = %e, "ID token verification failed after key refresh");
                    (StatusCode::UNAUTHORIZED, "invalid ID token")
                })?
        }
        Err(e) => {
            tracing::warn!(error = %e, "ID token verification failed");
            return Err((StatusCode::UNAUTHORIZED, "invalid ID token"));
        }
    };

    let authorized = state.admin_group.as_ref().is_none_or(|required| {
        claim_contains(
            &claims.additional_claims().claims,
            &state.admin_claim,
            required,
        )
    });
    if !authorized {
        tracing::warn!(sub = %claims.subject().as_str(), claim = %state.admin_claim, "admin group missing");
        return Err((StatusCode::FORBIDDEN, "not an admin"));
    }

    Ok(Session {
        sub: claims.subject().to_string(),
        email: claims.email().map(|e| e.to_string()),
        exp: now_secs() + SESSION_TTL_SECS,
    })
}

pub async fn logout(jar: PrivateCookieJar) -> Response {
    let removal = Cookie::build((SESSION_COOKIE, "")).path("/").build();
    (jar.remove(removal), StatusCode::NO_CONTENT).into_response()
}

pub async fn require_session(jar: PrivateCookieJar, mut request: Request, next: Next) -> Response {
    let session = jar
        .get(SESSION_COOKIE)
        .and_then(|c| serde_json::from_str::<Session>(c.value()).ok())
        .filter(|s| s.exp > now_secs());
    match session {
        Some(session) => {
            request.extensions_mut().insert(session);
            next.run(request).await
        }
        None => (
            StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!({ "error": "unauthenticated" })),
        )
            .into_response(),
    }
}
