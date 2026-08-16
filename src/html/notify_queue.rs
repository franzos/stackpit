//! The delivery queue page: notifications that failed and what happened to them.

use askama::Template;
use axum::extract::{Path, State};

use crate::html::chrome::PageChrome;
use crate::html::render_template;
use crate::html::utils::Chrome;
use crate::orgs::extractor::{require_org_owner, ActiveOrg};
use crate::queries;
use crate::server::AppState;

#[allow(unused_imports)]
use crate::html::filters;

/// Render cap only; the retention sweep bounds the table itself.
const PAGE_LIMIT: i64 = 200;

pub struct QueueRow {
    pub id: i64,
    pub project_id: i64,
    pub project_label: String,
    pub integration_name: String,
    pub integration_kind: String,
    pub status: String,
    pub attempts: i64,
    pub last_error: Option<String>,
    pub next_attempt_at: i64,
    pub created_at: i64,
}

impl QueueRow {
    pub fn is_failed(&self) -> bool {
        self.status == queries::notify_queue::STATUS_FAILED
    }
}

#[derive(Template)]
#[template(path = "notify_queue.html")]
struct QueueTemplate {
    rows: Vec<QueueRow>,
    pending: i64,
    failed: i64,
    message: Option<String>,
    chrome: PageChrome,
}

pub async fn handler(
    State(state): State<AppState>,
    Chrome(chrome): Chrome,
    active: ActiveOrg,
) -> axum::response::Response {
    if let Err(r) = require_org_owner(&active) {
        return r;
    }
    render_page(&state, &chrome, active.session_org_id, None).await
}

/// Attempt one queued delivery now, synchronously: a failure is reported, never re-queued.
pub async fn replay(
    State(state): State<AppState>,
    Chrome(chrome): Chrome,
    active: ActiveOrg,
    Path(id): Path<i64>,
) -> axum::response::Response {
    if let Err(r) = require_org_owner(&active) {
        return r;
    }
    let org_id = active.session_org_id;

    let item = match queries::notify_queue::get(&state.pool, id, org_id).await {
        Ok(Some(i)) => i,
        Ok(None) => {
            return render_page(
                &state,
                &chrome,
                org_id,
                Some(chrome.t("flash-queue-item-not-found")),
            )
            .await
        }
        Err(e) => return render_page(&state, &chrome, org_id, Some(chrome.err(e))).await,
    };

    // Same `DrainCtx` as the background drain, so replay re-checks licence and SSRF by the same code.
    let ctx = state.drain_ctx();
    let msg = match crate::notify::queue::replay_once(&ctx, &item).await {
        Ok(()) => {
            if let Err(e) =
                queries::notify_queue::delete(&state.writer_pool, id, Some(org_id)).await
            {
                chrome.err(e)
            } else {
                chrome.t("flash-queue-replayed")
            }
        }
        Err(e) => chrome.tv1("flash-queue-replay-failed", "error", &e),
    };
    render_page(&state, &chrome, org_id, Some(msg)).await
}

/// Drop a queued item without attempting it.
pub async fn cancel(
    State(state): State<AppState>,
    Chrome(chrome): Chrome,
    active: ActiveOrg,
    Path(id): Path<i64>,
) -> axum::response::Response {
    if let Err(r) = require_org_owner(&active) {
        return r;
    }
    let org_id = active.session_org_id;
    let msg = match queries::notify_queue::delete(&state.writer_pool, id, Some(org_id)).await {
        Ok(0) => chrome.t("flash-queue-item-not-found"),
        Ok(_) => chrome.t("flash-queue-cancelled"),
        Err(e) => chrome.err(e),
    };
    render_page(&state, &chrome, org_id, Some(msg)).await
}

async fn render_page(
    state: &AppState,
    chrome: &PageChrome,
    org_id: i64,
    message: Option<String>,
) -> axum::response::Response {
    let rows = queries::notify_queue::list_for_org_detailed(&state.pool, org_id, PAGE_LIMIT)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|v| QueueRow {
            id: v.item.id,
            project_id: v.item.project_id,
            project_label: v
                .project_name
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| v.item.project_id.to_string()),
            integration_name: v.integration_name,
            integration_kind: v.integration_kind,
            status: v.item.status,
            attempts: v.item.attempts,
            last_error: v.item.last_error,
            next_attempt_at: v.item.next_attempt_at,
            created_at: v.item.created_at,
        })
        .collect();

    let (pending, failed) = queries::notify_queue::counts_for_org(&state.pool, org_id)
        .await
        .unwrap_or((0, 0));

    render_template(&QueueTemplate {
        rows,
        pending,
        failed,
        message,
        chrome: chrome.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use askama::Template;
    use unic_langid::langid;

    fn chrome_for(locale: unic_langid::LanguageIdentifier) -> PageChrome {
        PageChrome::new("csrf".into(), locale, "/web/projects/".into())
    }

    fn row(id: i64, status: &str) -> QueueRow {
        QueueRow {
            id,
            project_id: 42,
            project_label: "web".into(),
            integration_name: "ops".into(),
            integration_kind: "slack".into(),
            status: status.into(),
            attempts: 3,
            last_error: Some("connection refused".into()),
            next_attempt_at: 1_700_000_000,
            created_at: 1_700_000_000,
        }
    }

    #[test]
    fn the_queue_page_renders_both_states_and_the_empty_case() {
        for locale in [langid!("en"), langid!("de")] {
            let chrome = chrome_for(locale.clone());
            let html = QueueTemplate {
                rows: vec![
                    row(1, queries::notify_queue::STATUS_PENDING),
                    row(2, queries::notify_queue::STATUS_FAILED),
                ],
                pending: 1,
                failed: 1,
                message: Some("hi".into()),
                chrome: chrome.clone(),
            }
            .render()
            .expect("queue page renders");
            assert!(
                !html.contains(crate::i18n::MISSING_PREFIX),
                "queue page ({locale}) leaked a missing localization key: {html}"
            );
            assert!(html.contains("/web/settings/queue/1/replay"));
            assert!(html.contains("/web/settings/queue/2/cancel"));

            let empty = QueueTemplate {
                rows: Vec::new(),
                pending: 0,
                failed: 0,
                message: None,
                chrome,
            }
            .render()
            .expect("empty queue page renders");
            assert!(!empty.contains(crate::i18n::MISSING_PREFIX));
        }
    }

    async fn seed(pool: &crate::db::DbPool) -> (i64, ActiveOrg) {
        sqlx::query(crate::db::sql!(
            "INSERT INTO organizations (org_id, slug, name) VALUES (5, 'acme', 'Acme')
             ON CONFLICT (org_id) DO NOTHING"
        ))
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(crate::db::sql!(
            "INSERT INTO projects (project_id, name, org_id) VALUES (42, 'web', 5)"
        ))
        .execute(pool)
        .await
        .unwrap();
        let id = queries::integrations::create_integration(
            pool,
            5,
            "ops",
            "webhook",
            Some("https://hooks.test/x"),
            None,
            None,
            false,
            false,
        )
        .await
        .unwrap();
        (
            id,
            ActiveOrg {
                session_org_id: 5,
                role: Some(crate::orgs::Role::Owner),
                org_name: None,
                memberships: vec![(5, crate::orgs::Role::Owner)],
            },
        )
    }

    async fn queued_id(pool: &crate::db::DbPool, integration_id: i64) -> i64 {
        queries::notify_queue::enqueue(pool, 5, 42, integration_id, "{}", "boom", 1000, 1000)
            .await
            .unwrap();
        queries::notify_queue::list_for_org(pool, 5, 10)
            .await
            .unwrap()[0]
            .id
    }

    #[tokio::test]
    async fn cancel_drops_the_item_and_refuses_another_org() {
        let pool = crate::db::open_test_pool().await;
        let (integration_id, active) = seed(&pool).await;
        let item_id = queued_id(&pool, integration_id).await;
        let (state, _chans) = crate::server::AppState::for_test(pool.clone());

        let mut foreign = active.clone();
        foreign.session_org_id = 6;
        foreign.memberships = vec![(6, crate::orgs::Role::Owner)];
        cancel(
            State(state.clone()),
            Chrome(chrome_for(langid!("en"))),
            foreign,
            Path(item_id),
        )
        .await;
        assert_eq!(
            queries::notify_queue::list_for_org(&pool, 5, 10)
                .await
                .unwrap()
                .len(),
            1,
            "another org must not be able to cancel this item"
        );

        cancel(
            State(state),
            Chrome(chrome_for(langid!("en"))),
            active,
            Path(item_id),
        )
        .await;
        assert!(queries::notify_queue::list_for_org(&pool, 5, 10)
            .await
            .unwrap()
            .is_empty());
    }

    /// Without the licence check, a button click would deliver Slack on an unlicensed install.
    #[tokio::test]
    async fn replay_is_refused_on_an_unlicensed_install() {
        let pool = crate::db::open_test_pool().await;
        let (integration_id, active) = seed(&pool).await;
        let item_id = queued_id(&pool, integration_id).await;
        let (mut state, _chans) = crate::server::AppState::for_test(pool.clone());
        state.license = crate::commercial::LicenseHandle::new(
            crate::commercial::license::LicenseStatus::Unlicensed,
            0,
        );

        let response = replay(
            State(state),
            Chrome(chrome_for(langid!("en"))),
            active,
            Path(item_id),
        )
        .await;
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let remaining = queries::notify_queue::list_for_org(&pool, 5, 10)
            .await
            .unwrap();
        assert_eq!(remaining.len(), 1, "the item is kept, not delivered");
    }

    #[tokio::test]
    async fn a_failed_replay_keeps_the_item() {
        let pool = crate::db::open_test_pool().await;
        let (integration_id, active) = seed(&pool).await;
        let item_id = queued_id(&pool, integration_id).await;
        let (state, _chans) = crate::server::AppState::for_test(pool.clone());

        // hooks.test does not resolve, so the attempt fails at the SSRF gate.
        replay(
            State(state),
            Chrome(chrome_for(langid!("en"))),
            active,
            Path(item_id),
        )
        .await;

        let remaining = queries::notify_queue::list_for_org(&pool, 5, 10)
            .await
            .unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(
            remaining[0].attempts, 1,
            "a manual replay does not consume an automatic retry"
        );
    }
}
