//! Assets API Builder - Configurable router builder for Assets API endpoints
//!
//! Two route families, a mutation switch and an admin switch:
//! `specs/design/axum-assets-endpoints/phase2-architecture.md` ("Routes"), originally
//! `specs/design/axum-assets-recipes-api/`.
//!
//! **Route-presence rule.** A primary mutation route (`POST key/data|entry`, `DELETE
//! key/data|entry`, `DELETE key/removedir`, `PUT key/makedir`, `POST key/description|expire|
//! override`) exists unless [`AssetsApiBuilder::read_only`] is set. An `admin/` route exists unless
//! `read_only()` is set or `with_admin(false)` is given. A GET alternative exists iff its primary
//! route exists and [`AssetsApiBuilder::with_destructive_gets`] is set. Cancel, `submit` (both
//! methods), the observe routes and the reads always exist. An omitted route answers axum's own
//! 405 (the path serves other methods) or 404.

use axum::{
    routing::{delete, get, post, put, MethodRouter},
    Extension, Router,
};
use liquers_core::context::{EnvRef, Environment};
use std::marker::PhantomData;

use super::key_handlers as kh;
use super::query_handlers as qh;
use super::websocket::{self as ws, WebSocketLimits};

/// Builder for Assets API endpoints (`{base}/q/…`, `{base}/key/…`, `{base}/admin/…`, and the
/// WebSocket at `{base}/ws/q` and `{base}/ws/key`).
pub struct AssetsApiBuilder<E: Environment> {
    base_path: String,
    websocket_path: Option<String>,
    read_only: bool,
    admin: bool,
    destructive_gets: bool,
    websocket_limits: WebSocketLimits,
    _phantom: PhantomData<E>,
}

impl<E: Environment> AssetsApiBuilder<E> {
    /// Create a new AssetsApiBuilder with the specified base path.
    ///
    /// The WebSocket endpoints default to `{base_path}/ws/q` and `{base_path}/ws/key`.
    ///
    /// # Example
    /// ```ignore
    /// let builder = AssetsApiBuilder::new("/liquer/api/assets");
    /// ```
    pub fn new(base_path: impl Into<String>) -> Self {
        let base_path = base_path.into();
        let websocket_path = Some(format!("{}/ws", base_path));
        Self {
            base_path,
            websocket_path,
            read_only: false,
            admin: true,
            destructive_gets: false,
            websocket_limits: WebSocketLimits::default(),
            _phantom: PhantomData,
        }
    }

    /// Set the WebSocket base path; the endpoints are `{ws_path}/q` and `{ws_path}/key`.
    ///
    /// # Example
    /// ```ignore
    /// let builder = AssetsApiBuilder::new("/api/assets")
    ///     .with_websocket_path("/api/notifications");
    /// ```
    pub fn with_websocket_path(mut self, ws_path: impl Into<String>) -> Self {
        self.websocket_path = Some(ws_path.into());
        self
    }

    /// Limits of the WebSocket endpoints: the largest client message and the number of
    /// subscriptions per connection (defaults: 64 KiB, 256).
    pub fn with_websocket_limits(mut self, limits: WebSocketLimits) -> Self {
        self.websocket_limits = limits;
        self
    }

    /// Disable the WebSocket endpoints.
    pub fn without_websocket(mut self) -> Self {
        self.websocket_path = None;
        self
    }

    /// Omit every state-changing key route and the admin routes. `q/cancel` and `key/cancel`
    /// stay: cancelling changes no data. A stop-gap until real access control exists.
    pub fn read_only(mut self) -> Self {
        self.read_only = true;
        self
    }

    /// Include (the default) or omit every `admin/` route: `audit`, `audit/{*key}`,
    /// `refresh_command_versions`.
    pub fn with_admin(mut self, enabled: bool) -> Self {
        self.admin = enabled;
        self
    }

    /// Also serve a GET alternative for every operation that needs no request body (`GET
    /// key/remove`, `GET key/expire`, …), as `StoreApiBuilder::with_destructive_gets` does. Off
    /// by default. `GET q/submit` and `GET key/submit` are always served.
    pub fn with_destructive_gets(mut self) -> Self {
        self.destructive_gets = true;
        self
    }

    /// A route whose primary method may be disabled, with its optional GET alternative.
    fn optional(
        &self,
        router: Router<EnvRef<E>>,
        path: &str,
        present: bool,
        primary: MethodRouter<EnvRef<E>>,
        get_alternative: MethodRouter<EnvRef<E>>,
    ) -> Router<EnvRef<E>> {
        if !present {
            return router;
        }
        let method_router = if self.destructive_gets {
            primary.merge(get_alternative)
        } else {
            primary
        };
        router.route(path, method_router)
    }

    /// Build the Axum router with all Assets API endpoints.
    ///
    /// Returns a Router that can be merged into your application.
    pub fn build(self) -> Router<EnvRef<E>> {
        let b = self.base_path.clone();
        let mutations = !self.read_only;
        let admin = !self.read_only && self.admin;
        let mut router = Router::new();

        // ---- query family: parse_query
        router = router
            .route(&format!("{b}/q/data/{{*query}}"), get(qh::q_get_data_handler::<E>))
            .route(&format!("{b}/q/entry/{{*query}}"), get(qh::q_get_entry_handler::<E>))
            .route(
                &format!("{b}/q/submit/{{*query}}"),
                get(qh::q_submit_handler::<E>).post(qh::q_submit_handler::<E>),
            )
            .route(&format!("{b}/q/info/{{*query}}"), get(qh::q_info_handler::<E>))
            .route(&format!("{b}/q/metadata/{{*query}}"), get(qh::q_get_metadata_handler::<E>))
            .route(&format!("{b}/q/version/{{*query}}"), get(qh::q_version_handler::<E>));
        router = self.optional(
            router,
            &format!("{b}/q/cancel/{{*query}}"),
            true,
            post(qh::q_cancel_handler::<E>),
            get(qh::q_cancel_handler::<E>),
        );

        // ---- key family: parse_key
        let mut data = get(kh::key_get_data_handler::<E>);
        let mut entry = get(kh::key_get_entry_handler::<E>);
        if mutations {
            data = data
                .post(kh::key_post_data_handler::<E>)
                .delete(kh::key_remove_handler::<E>);
            entry = entry
                .post(kh::key_post_entry_handler::<E>)
                .delete(kh::key_remove_handler::<E>);
        }
        router = router
            .route(&format!("{b}/key/data/{{*key}}"), data)
            .route(&format!("{b}/key/entry/{{*key}}"), entry)
            .route(
                &format!("{b}/key/submit/{{*key}}"),
                get(kh::key_submit_handler::<E>).post(kh::key_submit_handler::<E>),
            )
            .route(&format!("{b}/key/info/{{*key}}"), get(kh::key_info_handler::<E>))
            .route(
                &format!("{b}/key/metadata/{{*key}}"),
                get(kh::key_get_metadata_handler::<E>).post(kh::key_post_metadata_handler::<E>),
            )
            .route(&format!("{b}/key/version/{{*key}}"), get(kh::key_version_handler::<E>))
            .route(&format!("{b}/key/contains/{{*key}}"), get(kh::key_contains_handler::<E>))
            .route(&format!("{b}/key/can_make/{{*key}}"), get(kh::key_can_make_handler::<E>))
            .route(&format!("{b}/key/recover/{{*key}}"), get(kh::key_recover_handler::<E>))
            .route(&format!("{b}/key/listdir"), get(kh::key_listdir_root_handler::<E>))
            .route(&format!("{b}/key/listdir/{{*key}}"), get(kh::key_listdir_handler::<E>));
        if mutations && self.destructive_gets {
            router = router.route(
                &format!("{b}/key/remove/{{*key}}"),
                get(kh::key_remove_handler::<E>),
            );
        }
        router = self.optional(
            router,
            &format!("{b}/key/removedir/{{*key}}"),
            mutations,
            delete(kh::key_removedir_handler::<E>),
            get(kh::key_removedir_handler::<E>),
        );
        router = self.optional(
            router,
            &format!("{b}/key/makedir/{{*key}}"),
            mutations,
            put(kh::key_makedir_handler::<E>),
            get(kh::key_makedir_handler::<E>),
        );
        router = self.optional(
            router,
            &format!("{b}/key/description/{{*key}}"),
            mutations,
            post(kh::key_description_handler::<E>),
            get(kh::key_description_handler::<E>),
        );
        router = self.optional(
            router,
            &format!("{b}/key/expire/{{*key}}"),
            mutations,
            post(kh::key_expire_handler::<E>),
            get(kh::key_expire_handler::<E>),
        );
        router = self.optional(
            router,
            &format!("{b}/key/override/{{*key}}"),
            mutations,
            post(kh::key_override_handler::<E>),
            get(kh::key_override_handler::<E>),
        );
        router = self.optional(
            router,
            &format!("{b}/key/cancel/{{*key}}"),
            true,
            post(kh::key_cancel_handler::<E>),
            get(kh::key_cancel_handler::<E>),
        );

        // ---- admin
        router = self.optional(
            router,
            &format!("{b}/admin/audit/{{*key}}"),
            admin,
            post(kh::key_audit_handler::<E>),
            get(kh::key_audit_handler::<E>),
        );
        router = self.optional(
            router,
            &format!("{b}/admin/audit"),
            admin,
            post(kh::audit_all_handler::<E>),
            get(kh::audit_all_handler::<E>),
        );
        router = self.optional(
            router,
            &format!("{b}/admin/refresh_command_versions"),
            admin,
            post(kh::refresh_command_versions_handler::<E>),
            get(kh::refresh_command_versions_handler::<E>),
        );

        // ---- WebSocket: {ws}/q[/{*query}] and {ws}/key[/{*key}]; unaffected by read_only()
        if let Some(ws_path) = &self.websocket_path {
            let limits = Extension(self.websocket_limits);
            router = router
                .route(
                    &format!("{ws_path}/q"),
                    get(ws::ws_query_handler::<E>).layer(limits),
                )
                .route(
                    &format!("{ws_path}/q/{{*query}}"),
                    get(ws::ws_query_path_handler::<E>).layer(limits),
                )
                .route(
                    &format!("{ws_path}/key"),
                    get(ws::ws_key_handler::<E>).layer(limits),
                )
                .route(
                    &format!("{ws_path}/key/{{*key}}"),
                    get(ws::ws_key_path_handler::<E>).layer(limits),
                );
        }

        router
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use liquers_core::context::SimpleEnvironment;
    use liquers_core::value::Value;

    #[test]
    fn test_assets_api_builder_new_creates_correct_structure() {
        let builder: AssetsApiBuilder<SimpleEnvironment<Value>> =
            AssetsApiBuilder::new("/liquer/api/assets");
        assert_eq!(builder.base_path, "/liquer/api/assets");
        assert_eq!(
            builder.websocket_path,
            Some("/liquer/api/assets/ws".to_string())
        );
    }

    #[test]
    fn test_assets_api_builder_with_websocket_path_sets_custom_path() {
        let builder: AssetsApiBuilder<SimpleEnvironment<Value>> =
            AssetsApiBuilder::new("/api/assets").with_websocket_path("/api/ws/assets");
        assert_eq!(builder.websocket_path, Some("/api/ws/assets".to_string()));
    }

    #[test]
    fn test_assets_api_builder_without_websocket_disables_ws() {
        let builder: AssetsApiBuilder<SimpleEnvironment<Value>> =
            AssetsApiBuilder::new("/api/assets").without_websocket();
        assert_eq!(builder.websocket_path, None);
    }

    /// Whether `method uri` reaches a handler. axum's own 404/405 for a missing route or method
    /// have an empty body; a handler's error (a 404 for an absent key, say) is an `ApiResponse`.
    async fn routed(
        builder: AssetsApiBuilder<SimpleEnvironment<Value>>,
        method: &str,
        uri: &str,
    ) -> (bool, axum::http::StatusCode) {
        use tower::ServiceExt;
        let envref = SimpleEnvironment::<Value>::new().to_ref();
        let app = builder.build().with_state(envref);
        let request = axum::http::Request::builder()
            .method(method)
            .uri(uri)
            .body(axum::body::Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let missing = (status == axum::http::StatusCode::NOT_FOUND
            || status == axum::http::StatusCode::METHOD_NOT_ALLOWED)
            && body.is_empty();
        (!missing, status)
    }

    /// The route-presence rule, for all eight combinations of the three switches.
    #[tokio::test]
    async fn route_presence_for_every_switch_combination() {
        for read_only in [false, true] {
            for admin in [false, true] {
                for gets in [false, true] {
                    let make = || {
                        let mut builder = AssetsApiBuilder::<SimpleEnvironment<Value>>::new("/a")
                            .with_admin(admin);
                        if read_only {
                            builder = builder.read_only();
                        }
                        if gets {
                            builder = builder.with_destructive_gets();
                        }
                        builder
                    };
                    let case = format!("read_only={read_only} admin={admin} gets={gets}");
                    let mutations = !read_only;
                    let admin_on = !read_only && admin;
                    let expectations: [(&str, &str, bool); 16] = [
                        ("POST", "/a/key/data/x.txt", mutations),
                        ("DELETE", "/a/key/entry/x.txt", mutations),
                        ("DELETE", "/a/key/removedir/d", mutations),
                        ("PUT", "/a/key/makedir/d", mutations),
                        ("POST", "/a/key/expire/x.txt", mutations),
                        ("GET", "/a/key/remove/x.txt", mutations && gets),
                        ("GET", "/a/key/expire/x.txt", mutations && gets),
                        ("POST", "/a/admin/audit", admin_on),
                        ("GET", "/a/admin/refresh_command_versions", admin_on && gets),
                        ("POST", "/a/q/cancel/make_text", true),
                        ("GET", "/a/key/cancel/x.txt", gets),
                        ("GET", "/a/q/submit/make_text", true),
                        ("POST", "/a/key/submit/x.txt", true),
                        ("POST", "/a/key/metadata/x.txt", true),
                        ("GET", "/a/key/info/x.txt", true),
                        ("GET", "/a/key/listdir", true),
                    ];
                    for (method, uri, expected) in expectations {
                        let (routed, status) = routed(make(), method, uri).await;
                        assert_eq!(
                            routed,
                            expected,
                            "{case}: {method} {uri} answered {status}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn test_assets_api_builder_method_chaining() {
        let builder: AssetsApiBuilder<SimpleEnvironment<Value>> =
            AssetsApiBuilder::new("/api/assets")
                .with_websocket_path("/ws")
                .without_websocket()
                .with_websocket_path("/api/notifications");
        assert_eq!(builder.base_path, "/api/assets");
        assert_eq!(
            builder.websocket_path,
            Some("/api/notifications".to_string())
        );
    }
}
