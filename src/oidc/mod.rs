//! Browser-OIDC BFF: opaque `sp_grant` cookie indexes a server-side row
//! holding the IdP tokens encrypted at rest. Every authed request
//! introspects the access token via [`stackpit_auth::BearerGate`].

pub mod client;
pub mod cookies;
pub mod grants;
pub mod login_state;
pub mod logout;
pub mod refresh;
pub mod revocations;
