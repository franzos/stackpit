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

    pub fn from_str(s: &str) -> Self {
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

/// Guesses the forge from the hostname. Self-hosted GitLab instances get
/// caught by the `contains("gitlab")` fallback -- good enough for most cases.
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
        h if h.contains("gitlab") => ForgeType::GitLab,
        _ => ForgeType::Unknown,
    };
    (forge, hostname)
}

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

/// Builds a link to a specific file+line in a commit. A user-provided template
/// takes precedence -- otherwise we construct forge-specific URLs.
/// Returns `None` for unknown forges without a template.
pub fn source_url(
    forge_type: &ForgeType,
    repo_url: &str,
    url_template: Option<&str>,
    commit: &str,
    path: &str,
    line: u64,
) -> Option<String> {
    // Custom template takes priority
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

    // Already an http(s) URL, just pass it through
    url.to_string()
}

fn extract_hostname(url: &str) -> String {
    let url = url.trim();

    // git@ style -- colon separates host from path
    if let Some(rest) = url.strip_prefix("git@") {
        if let Some((host, _)) = rest.split_once(':') {
            return host.to_string();
        }
    }

    // Strip scheme and optional userinfo to get at the hostname
    let after_scheme = url.find("://").map(|i| &url[i + 3..]).unwrap_or(url);

    let after_userinfo = after_scheme
        .split_once('@')
        .map(|(_, rest)| rest)
        .unwrap_or(after_scheme);

    // Everything before the first / or : is the host
    after_userinfo
        .split(['/', ':'])
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
            assert_eq!(ForgeType::from_str(ft.as_str()), ft);
        }
    }
}
