use anyhow::Result;

/// Extract `scheme://host[:port]` from an absolute http(s) URL.
pub(super) fn url_origin(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let (scheme, rest) = if let Some(r) = trimmed.strip_prefix("https://") {
        ("https", r)
    } else if let Some(r) = trimmed.strip_prefix("http://") {
        ("http", r)
    } else {
        return None;
    };
    let host = rest.split(['/', '?', '#']).next()?;
    if host.is_empty() {
        return None;
    }
    Some(format!("{scheme}://{host}"))
}

/// Reject non-absolute-http(s) URLs in operator config so `javascript:` /
/// `//host` / bare-path values can't reach the RP-initiated logout bounce.
pub(super) fn validate_absolute_http_url(field: &str, value: Option<&str>) -> Result<()> {
    let Some(url) = value else {
        return Ok(());
    };
    let trimmed = url.trim();
    if trimmed.is_empty() {
        anyhow::bail!("{field} is set to an empty string -- remove the key or set a real URL");
    }
    let host_start = if let Some(rest) = trimmed.strip_prefix("https://") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("http://") {
        rest
    } else {
        anyhow::bail!("{field} must start with http:// or https://, got '{trimmed}'");
    };
    let host = host_start.split(['/', '?', '#']).next().unwrap_or("");
    if host.is_empty() {
        anyhow::bail!("{field} '{trimmed}' is missing a host component");
    }
    Ok(())
}

/// Force HTTPS for `issuer_url` (except loopback). A plain-HTTP Hydra
/// behind a TLS terminator leaks http:// endpoints into RP-initiated logout
/// and bearer validation.
pub(super) fn validate_issuer_url_scheme(issuer: &str) -> Result<()> {
    let trimmed = issuer.trim();
    if trimmed.is_empty() {
        anyhow::bail!(
            "auth.oauth.issuer_url is set to an empty string -- remove the key or set a real URL"
        );
    }
    let Some(rest) = trimmed.strip_prefix("http://") else {
        if trimmed.starts_with("https://") {
            return Ok(());
        }
        anyhow::bail!("auth.oauth.issuer_url must start with http:// or https://, got '{trimmed}'");
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host_only = host.rsplit_once(':').map(|(h, _)| h).unwrap_or(host);
    match host_only {
        // `host.containers.internal` is a loopback alias on Podman/Docker Desktop.
        "localhost" | "127.0.0.1" | "[::1]" | "::1" | "host.containers.internal" => Ok(()),
        _ => anyhow::bail!(
            "auth.oauth.issuer_url '{trimmed}' uses http:// with a non-loopback host -- HTTPS is \
             required. If Hydra is behind a TLS terminator, point issuer_url at the public HTTPS \
             URL instead of the internal http:// listener."
        ),
    }
}
