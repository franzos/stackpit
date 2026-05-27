//! Typed extractors over `AuthContext`. All read request extensions
//! populated by `resolve_auth_context`. Rejections are `StatusCode`;
//! callers wanting HTML redirects layer their own gate.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;

use crate::context::{AuthContext, PrincipalId};

impl<S> FromRequestParts<S> for AuthContext
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<AuthContext>()
            .cloned()
            .ok_or(StatusCode::UNAUTHORIZED)
    }
}

/// Caller must be authenticated as `Admin`.
#[derive(Debug, Clone)]
pub struct RequireAdmin;

impl<S> FromRequestParts<S> for RequireAdmin
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        match parts.extensions.get::<AuthContext>() {
            Some(AuthContext::Admin) => Ok(Self),
            Some(_) => Err(StatusCode::FORBIDDEN),
            None => Err(StatusCode::UNAUTHORIZED),
        }
    }
}

/// Caller must be authenticated as `User`. Carries the identity claims.
#[derive(Debug, Clone)]
pub struct RequireUser {
    pub iss: String,
    pub sub: String,
    pub principal_id: PrincipalId,
}

impl<S> FromRequestParts<S> for RequireUser
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        match parts.extensions.get::<AuthContext>() {
            Some(AuthContext::User {
                iss,
                sub,
                principal_id,
            }) => Ok(Self {
                iss: iss.clone(),
                sub: sub.clone(),
                principal_id: principal_id.clone(),
            }),
            // Admin has no (iss, sub) -- this extractor gates handlers that
            // need a user-bound principal.
            Some(AuthContext::Admin) => Err(StatusCode::FORBIDDEN),
            None => Err(StatusCode::UNAUTHORIZED),
        }
    }
}
