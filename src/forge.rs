/// Detects which git forge a repo lives on and builds source links accordingly.

#[derive(Debug, Clone, PartialEq)]
pub enum ForgeType {
    GitHub,
    GitLab,
    Gitea,
    Bitbucket,
    Sourcehut,
    Gitee,
    Azure,
    Unknown,
}

impl ForgeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::GitHub => "github",
            Self::GitLab => "gitlab",
            Self::Gitea => "gitea",
            Self::Bitbucket => "bitbucket",
            Self::Sourcehut => "sourcehut",
            Self::Gitee => "gitee",
            Self::Azure => "azure",
            Self::Unknown => "unknown",
        }
    }

    pub fn from_tag(s: &str) -> Self {
        match s {
            "github" => Self::GitHub,
            "gitlab" => Self::GitLab,
            "gitea" => Self::Gitea,
            "bitbucket" => Self::Bitbucket,
            "sourcehut" => Self::Sourcehut,
            "gitee" => Self::Gitee,
            "azure" => Self::Azure,
            _ => Self::Unknown,
        }
    }
}

/// Guesses the forge from the hostname; a neutral host stays Unknown and needs an override.
pub fn detect_forge(repo_url: &str) -> (ForgeType, String) {
    let hostname = extract_hostname(repo_url);
    let forge = match hostname.as_str() {
        "github.com" => ForgeType::GitHub,
        "gitlab.com" => ForgeType::GitLab,
        "codeberg.org" => ForgeType::Gitea,
        "gitea.com" => ForgeType::Gitea,
        "bitbucket.org" => ForgeType::Bitbucket,
        "git.sr.ht" => ForgeType::Sourcehut,
        "gitee.com" => ForgeType::Gitee,
        "dev.azure.com" | "ssh.dev.azure.com" => ForgeType::Azure,
        // Substring fallbacks for self-hosted instances that name themselves.
        // GitLab is tried before GitHub deliberately: `gitlab.example.com`
        // contains neither the other's name, but the order has been relied on.
        //
        // There is no `git.<domain>` → Gitea rule and no network probe:
        // `git.gnome.org` is GitLab and `git.kernel.org` is cgit, so a guess
        // would silently produce broken source links where `unknown` at least
        // admits it does not know. The operator sets the override once.
        h if h.contains("gitlab") => ForgeType::GitLab,
        h if h.contains("github") => ForgeType::GitHub,
        h if h.contains("forgejo") || h.contains("gitea") => ForgeType::Gitea,
        h if h.contains("bitbucket") => ForgeType::Bitbucket,
        h if h.contains("sourcehut") || h.contains("sr.ht") => ForgeType::Sourcehut,
        h if h.contains("gitee") => ForgeType::Gitee,
        h if h.contains("azure") => ForgeType::Azure,
        _ => ForgeType::Unknown,
    };
    (forge, hostname)
}

/// Forge tag for a tracker integration kind; Forgejo maps onto `gitea`, notify kinds to `None`.
pub fn tracker_forge_tag(kind: crate::domain::IntegrationKind) -> Option<&'static str> {
    use crate::domain::IntegrationKind as K;
    match kind {
        K::GitHub => Some(ForgeType::GitHub.as_str()),
        K::GitLab => Some(ForgeType::GitLab.as_str()),
        K::Forgejo => Some(ForgeType::Gitea.as_str()),
        K::Webhook | K::Slack | K::Email => None,
    }
}

/// A repository's forge coordinates, derived from its URL.
#[derive(Debug, Clone, PartialEq)]
pub struct ForgeRef {
    pub host: String,
    /// GitHub and Forgejo: the first path segment. `None` for GitLab.
    pub owner: Option<String>,
    /// GitHub and Forgejo: the second path segment. `None` for GitLab.
    pub repo: Option<String>,
    /// GitLab: the whole namespace path, percent-encoded. `None` for the others.
    pub gitlab_path: Option<String>,
}

/// The path after the host, minus surrounding slashes and any `.git` suffix.
pub fn repo_path(repo_url: &str) -> String {
    let url = repo_url.trim().trim_end_matches('/');
    let url = url.strip_suffix(".git").unwrap_or(url);

    if let Some(rest) = url.strip_prefix("git@") {
        if let Some((_, path)) = rest.split_once(':') {
            return path.trim_matches('/').to_string();
        }
    }

    let after_scheme = url.find("://").map(|i| &url[i + 3..]).unwrap_or(url);

    // Userinfo lives inside the authority; splitting it off first would move this split point.
    match after_scheme.split_once('/') {
        Some((_authority, path)) => path.trim_matches('/').to_string(),
        None => String::new(),
    }
}

/// Derive tracker coordinates from a repository URL.
pub fn derive_forge_ref(forge: &ForgeType, repo_url: &str) -> Result<ForgeRef, String> {
    let host = extract_hostname(repo_url);
    if host.is_empty() {
        return Err(format!("cannot read a hostname from `{repo_url}`"));
    }
    let path = repo_path(repo_url);
    if path.is_empty() {
        return Err(format!("`{repo_url}` has no repository path"));
    }

    match forge {
        ForgeType::GitLab => Ok(ForgeRef {
            host,
            owner: None,
            repo: None,
            gitlab_path: Some(
                percent_encoding::utf8_percent_encode(&path, GITLAB_PATH_ENCODE).to_string(),
            ),
        }),
        ForgeType::GitHub | ForgeType::Gitea => {
            let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
            if segments.len() != 2 {
                return Err(format!(
                    "expected an owner/repo path, got `{path}` ({} segments)",
                    segments.len()
                ));
            }
            Ok(ForgeRef {
                host,
                owner: Some(segments[0].to_string()),
                repo: Some(segments[1].to_string()),
                gitlab_path: None,
            })
        }
        other => Err(format!("{} is not a tracker forge", other.as_str())),
    }
}

/// GitLab's project reference needs `/` encoded, which the default PATH set leaves alone.
pub const GITLAB_PATH_ENCODE: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.');

/// Human-friendly label for the forge -- falls back to the raw hostname.
pub fn label_from_hostname(hostname: &str) -> String {
    match hostname {
        "github.com" => "GitHub".to_string(),
        "gitlab.com" => "GitLab".to_string(),
        "codeberg.org" => "Codeberg".to_string(),
        "gitea.com" => "Gitea".to_string(),
        "bitbucket.org" => "Bitbucket".to_string(),
        "git.sr.ht" => "Sourcehut".to_string(),
        "gitee.com" => "Gitee".to_string(),
        "dev.azure.com" | "ssh.dev.azure.com" => "Azure DevOps".to_string(),
        h if h.contains("gitlab") => "GitLab".to_string(),
        other => other.to_string(),
    }
}

/// Build source link (user template or forge-specific URL; None if unknown).
pub fn source_url(
    forge_type: &ForgeType,
    repo_url: &str,
    url_template: Option<&str>,
    commit: &str,
    path: &str,
    line: u64,
) -> Option<String> {
    if let Some(tmpl) = url_template {
        if !tmpl.is_empty() {
            let base = normalize_repo_url(repo_url);
            return Some(
                tmpl.replace("{repo}", &base)
                    .replace("{commit}", commit)
                    .replace("{path}", path)
                    .replace("{line}", &line.to_string()),
            );
        }
    }

    let base = normalize_repo_url(repo_url);

    match forge_type {
        ForgeType::GitHub | ForgeType::Gitee => {
            Some(format!("{base}/blob/{commit}/{path}#L{line}"))
        }
        ForgeType::GitLab => Some(format!("{base}/-/blob/{commit}/{path}#L{line}")),
        ForgeType::Gitea => Some(format!("{base}/src/commit/{commit}/{path}#L{line}")),
        ForgeType::Bitbucket => Some(format!("{base}/src/{commit}/{path}#lines-{line}")),
        ForgeType::Sourcehut => Some(format!("{base}/tree/{commit}/item/{path}#L{line}")),
        ForgeType::Azure => Some(format!("{base}?path={path}&version=GC{commit}&line={line}")),
        ForgeType::Unknown => None,
    }
}

/// Normalizes any repo URL (SSH, git@, http) down to a clean https base.
fn normalize_repo_url(url: &str) -> String {
    let url = url.trim_end_matches('/');
    let url = url.strip_suffix(".git").unwrap_or(url);

    // git@host:org/repo -> https://host/org/repo
    if let Some(rest) = url.strip_prefix("git@") {
        if let Some((host, path)) = rest.split_once(':') {
            return format!("https://{host}/{path}");
        }
    }

    // ssh://[user@]host/path -> https://host/path
    if let Some(rest) = url.strip_prefix("ssh://") {
        let rest = rest.split_once('@').map(|(_, r)| r).unwrap_or(rest);
        return format!("https://{rest}");
    }

    url.to_string()
}

pub fn extract_hostname(url: &str) -> String {
    let url = url.trim();

    // git@ style -- colon separates host from path
    if let Some(rest) = url.strip_prefix("git@") {
        if let Some((host, _)) = rest.split_once(':') {
            return host.to_string();
        }
    }

    // Strip scheme and optional userinfo to get at the hostname
    let after_scheme = url.find("://").map(|i| &url[i + 3..]).unwrap_or(url);

    // Isolate the authority first; a path segment with an `@` could otherwise fake the host.
    let authority = after_scheme.split('/').next().unwrap_or(after_scheme);

    let after_userinfo = authority
        .rsplit_once('@')
        .map(|(_, rest)| rest)
        .unwrap_or(authority);

    after_userinfo
        .split(':')
        .next()
        .unwrap_or(after_userinfo)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_github() {
        let (forge, host) = detect_forge("https://github.com/org/repo");
        assert_eq!(forge, ForgeType::GitHub);
        assert_eq!(host, "github.com");
    }

    #[test]
    fn detect_gitlab() {
        let (forge, _) = detect_forge("https://gitlab.com/org/repo.git");
        assert_eq!(forge, ForgeType::GitLab);
    }

    #[test]
    fn detect_self_hosted_gitlab() {
        let (forge, _) = detect_forge("https://gitlab.example.com/org/repo");
        assert_eq!(forge, ForgeType::GitLab);
    }

    #[test]
    fn detect_codeberg() {
        let (forge, host) = detect_forge("https://codeberg.org/user/repo");
        assert_eq!(forge, ForgeType::Gitea);
        assert_eq!(host, "codeberg.org");
    }

    #[test]
    fn detect_bitbucket() {
        let (forge, _) = detect_forge("git@bitbucket.org:org/repo.git");
        assert_eq!(forge, ForgeType::Bitbucket);
    }

    #[test]
    fn detect_sourcehut() {
        let (forge, _) = detect_forge("https://git.sr.ht/~user/repo");
        assert_eq!(forge, ForgeType::Sourcehut);
    }

    #[test]
    fn detect_unknown() {
        let (forge, host) = detect_forge("https://mygit.local/org/repo");
        assert_eq!(forge, ForgeType::Unknown);
        assert_eq!(host, "mygit.local");
    }

    #[test]
    fn source_url_github() {
        let url = source_url(
            &ForgeType::GitHub,
            "https://github.com/org/repo",
            None,
            "abc123",
            "src/main.rs",
            42,
        );
        assert_eq!(
            url.unwrap(),
            "https://github.com/org/repo/blob/abc123/src/main.rs#L42"
        );
    }

    #[test]
    fn source_url_gitlab() {
        let url = source_url(
            &ForgeType::GitLab,
            "https://gitlab.com/org/repo",
            None,
            "def456",
            "lib/foo.py",
            10,
        );
        assert_eq!(
            url.unwrap(),
            "https://gitlab.com/org/repo/-/blob/def456/lib/foo.py#L10"
        );
    }

    #[test]
    fn source_url_gitea() {
        let url = source_url(
            &ForgeType::Gitea,
            "https://codeberg.org/user/proj",
            None,
            "aaa",
            "main.go",
            1,
        );
        assert_eq!(
            url.unwrap(),
            "https://codeberg.org/user/proj/src/commit/aaa/main.go#L1"
        );
    }

    #[test]
    fn source_url_bitbucket() {
        let url = source_url(
            &ForgeType::Bitbucket,
            "https://bitbucket.org/org/repo",
            None,
            "bbb",
            "app.js",
            99,
        );
        assert_eq!(
            url.unwrap(),
            "https://bitbucket.org/org/repo/src/bbb/app.js#lines-99"
        );
    }

    #[test]
    fn source_url_sourcehut() {
        let url = source_url(
            &ForgeType::Sourcehut,
            "https://git.sr.ht/~user/repo",
            None,
            "ccc",
            "src/lib.rs",
            5,
        );
        assert_eq!(
            url.unwrap(),
            "https://git.sr.ht/~user/repo/tree/ccc/item/src/lib.rs#L5"
        );
    }

    #[test]
    fn source_url_azure() {
        let url = source_url(
            &ForgeType::Azure,
            "https://dev.azure.com/org/proj/_git/repo",
            None,
            "ddd",
            "Program.cs",
            12,
        );
        assert_eq!(
            url.unwrap(),
            "https://dev.azure.com/org/proj/_git/repo?path=Program.cs&version=GCddd&line=12"
        );
    }

    #[test]
    fn source_url_unknown_returns_none() {
        let url = source_url(
            &ForgeType::Unknown,
            "https://mygit.local/org/repo",
            None,
            "eee",
            "file.rs",
            1,
        );
        assert!(url.is_none());
    }

    #[test]
    fn source_url_template_override() {
        let url = source_url(
            &ForgeType::Unknown,
            "https://mygit.local/org/repo",
            Some("{repo}/view/{commit}/{path}?line={line}"),
            "fff",
            "app.rs",
            7,
        );
        assert_eq!(
            url.unwrap(),
            "https://mygit.local/org/repo/view/fff/app.rs?line=7"
        );
    }

    #[test]
    fn normalize_ssh_url() {
        let url = source_url(
            &ForgeType::GitHub,
            "git@github.com:org/repo.git",
            None,
            "abc",
            "f.rs",
            1,
        );
        assert_eq!(url.unwrap(), "https://github.com/org/repo/blob/abc/f.rs#L1");
    }

    #[test]
    fn label_known_hosts() {
        assert_eq!(label_from_hostname("github.com"), "GitHub");
        assert_eq!(label_from_hostname("codeberg.org"), "Codeberg");
        assert_eq!(label_from_hostname("gitlab.myorg.com"), "GitLab");
    }

    #[test]
    fn label_unknown_host() {
        assert_eq!(label_from_hostname("mygit.local"), "mygit.local");
    }

    #[test]
    fn detect_self_hosted_github_and_forgejo() {
        assert_eq!(
            detect_forge("https://github.acme.internal/org/repo").0,
            ForgeType::GitHub
        );
        assert_eq!(
            detect_forge("https://forgejo.acme.internal/org/repo").0,
            ForgeType::Gitea
        );
        assert_eq!(
            detect_forge("https://gitea.acme.internal/org/repo").0,
            ForgeType::Gitea
        );
    }

    /// The kinds that previously matched only an exact host now match by
    /// substring too, like GitHub/GitLab/Gitea already did.
    #[test]
    fn detect_self_hosted_instances_that_name_themselves() {
        for (url, want) in [
            ("https://bitbucket.acme.internal/o/r", ForgeType::Bitbucket),
            ("https://sourcehut.acme.internal/~o/r", ForgeType::Sourcehut),
            ("https://sr.ht.acme.internal/~o/r", ForgeType::Sourcehut),
            ("https://gitee.acme.internal/o/r", ForgeType::Gitee),
            ("https://azure.acme.internal/o/r", ForgeType::Azure),
        ] {
            assert_eq!(detect_forge(url).0, want, "{url}");
        }
        // The exact hosts keep working.
        assert_eq!(
            detect_forge("https://bitbucket.org/o/r").0,
            ForgeType::Bitbucket
        );
        assert_eq!(
            detect_forge("https://git.sr.ht/~o/r").0,
            ForgeType::Sourcehut
        );
        assert_eq!(detect_forge("https://gitee.com/o/r").0, ForgeType::Gitee);
        assert_eq!(
            detect_forge("https://dev.azure.com/o/r").0,
            ForgeType::Azure
        );
    }

    /// No `git.<domain>` heuristic: `git.gnome.org` is GitLab and
    /// `git.kernel.org` is cgit, so a guess would be silently wrong where
    /// `unknown` is at least honest.
    #[test]
    fn detect_leaves_a_neutral_host_unknown() {
        for url in [
            "https://git.gofranz.com/franz/stackpit",
            "https://git.gnome.org/o/r",
            "https://git.kernel.org/o/r",
            "https://code.example.com/o/r",
        ] {
            assert_eq!(detect_forge(url).0, ForgeType::Unknown, "{url}");
        }
    }

    #[test]
    fn repo_path_strips_scheme_host_git_suffix_and_slashes() {
        assert_eq!(repo_path("https://github.com/acme/backend"), "acme/backend");
        assert_eq!(
            repo_path("https://github.com/acme/backend.git"),
            "acme/backend"
        );
        assert_eq!(
            repo_path("https://github.com/acme/backend/"),
            "acme/backend"
        );
        assert_eq!(repo_path("git@github.com:acme/backend.git"), "acme/backend");
        assert_eq!(
            repo_path("ssh://git@gitlab.example/group/sub/proj.git"),
            "group/sub/proj"
        );
        assert_eq!(repo_path("https://github.com"), "");
    }

    #[test]
    fn derive_github_ref_needs_exactly_two_segments() {
        let r = derive_forge_ref(&ForgeType::GitHub, "https://github.com/acme/backend").unwrap();
        assert_eq!(r.host, "github.com");
        assert_eq!(r.owner.as_deref(), Some("acme"));
        assert_eq!(r.repo.as_deref(), Some("backend"));
        assert!(r.gitlab_path.is_none());

        assert!(derive_forge_ref(&ForgeType::GitHub, "https://github.com/acme").is_err());
        assert!(derive_forge_ref(&ForgeType::GitHub, "https://github.com/a/b/c").is_err());
        assert!(derive_forge_ref(&ForgeType::GitHub, "https://github.com").is_err());
    }

    #[test]
    fn derive_forgejo_ref_uses_the_same_two_segment_rule() {
        let r =
            derive_forge_ref(&ForgeType::Gitea, "git@git.gofranz.com:franz/stackpit.git").unwrap();
        assert_eq!(r.host, "git.gofranz.com");
        assert_eq!(r.owner.as_deref(), Some("franz"));
        assert_eq!(r.repo.as_deref(), Some("stackpit"));
    }

    #[test]
    fn derive_gitlab_ref_encodes_the_whole_subgroup_path() {
        let r = derive_forge_ref(
            &ForgeType::GitLab,
            "https://gitlab.com/group/subgroup/project.git",
        )
        .unwrap();
        assert_eq!(r.host, "gitlab.com");
        assert_eq!(r.gitlab_path.as_deref(), Some("group%2Fsubgroup%2Fproject"));
        assert!(r.owner.is_none());
        assert!(r.repo.is_none());

        let flat = derive_forge_ref(&ForgeType::GitLab, "git@gitlab.com:acme/backend.git").unwrap();
        assert_eq!(flat.gitlab_path.as_deref(), Some("acme%2Fbackend"));
    }

    #[test]
    fn gitlab_path_leaves_unreserved_characters_alone() {
        let r =
            derive_forge_ref(&ForgeType::GitLab, "https://gitlab.com/my-org/my.repo_v2").unwrap();
        assert_eq!(r.gitlab_path.as_deref(), Some("my-org%2Fmy.repo_v2"));
    }

    #[test]
    fn derive_rejects_non_tracker_forges() {
        assert!(derive_forge_ref(&ForgeType::Bitbucket, "https://bitbucket.org/a/b").is_err());
        assert!(derive_forge_ref(&ForgeType::Unknown, "https://git.local/a/b").is_err());
    }

    #[test]
    fn tracker_tag_maps_forgejo_onto_gitea_and_ignores_channels() {
        use crate::domain::IntegrationKind as K;
        assert_eq!(tracker_forge_tag(K::GitHub), Some("github"));
        assert_eq!(tracker_forge_tag(K::GitLab), Some("gitlab"));
        assert_eq!(tracker_forge_tag(K::Forgejo), Some("gitea"));
        assert_eq!(tracker_forge_tag(K::Slack), None);
        assert_eq!(tracker_forge_tag(K::Webhook), None);
        assert_eq!(tracker_forge_tag(K::Email), None);
    }

    #[test]
    fn extract_hostname_handles_every_accepted_url_form() {
        assert_eq!(extract_hostname("https://github.com/a/b"), "github.com");
        assert_eq!(extract_hostname("git@github.com:a/b.git"), "github.com");
        assert_eq!(
            extract_hostname("ssh://git@git.local:2222/a/b"),
            "git.local"
        );
        assert_eq!(extract_hostname("https://user@git.local/a/b"), "git.local");
    }

    /// The tracker's integration-host == repo-host guard is only as good as this.
    #[test]
    fn a_path_embedded_at_sign_cannot_advertise_a_host() {
        assert_eq!(
            extract_hostname("https://attacker.example/owner@github.com/repo"),
            "attacker.example"
        );
        assert_eq!(
            extract_hostname("https://attacker.example:8443/a@github.com/b"),
            "attacker.example"
        );
        assert_eq!(
            extract_hostname("https://user@git.local/owner@github.com/repo"),
            "git.local"
        );
        assert_eq!(
            repo_path("https://attacker.example/owner@github.com/repo"),
            "owner@github.com/repo",
            "the whole path is the path; none of it is authority"
        );
    }

    #[test]
    fn forge_type_roundtrip() {
        for ft in [
            ForgeType::GitHub,
            ForgeType::GitLab,
            ForgeType::Gitea,
            ForgeType::Bitbucket,
            ForgeType::Sourcehut,
            ForgeType::Gitee,
            ForgeType::Azure,
            ForgeType::Unknown,
        ] {
            assert_eq!(ForgeType::from_tag(ft.as_str()), ft);
        }
    }
}
