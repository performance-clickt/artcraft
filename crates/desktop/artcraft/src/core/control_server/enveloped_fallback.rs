use crate::core::control_server::auth::bearer_auth_layer::bearer_auth_layer;
use crate::core::control_server::envelope::control_response::{ControlErrorCode, ControlErrorResponse};
use crate::core::control_server::state::control_server_settings::ControlServerSettings;
use axum::{middleware, Router};

const UNMATCHED_PATH_MESSAGE: &str = "No such control endpoint.";
const METHOD_NOT_ALLOWED_MESSAGE: &str = "That control endpoint does not accept this HTTP method.";

/// Closes the control router: attaches the two enveloped catch-alls, then the bearer auth layer.
///
/// This exists as one function — rather than three chained calls at the call site — because the
/// ORDER of these three steps is a security boundary, and axum fails it silently in both
/// directions. Verified against axum 0.8.9's source, not its docs:
///
/// * `Router::layer` fans the layer into `path_router`, `fallback_router` AND `catch_all_fallback`
///   (routing/mod.rs), and `Endpoint::layer` keeps a `MethodRouter` a `MethodRouter` while
///   `MethodRouter::layer` maps its `fallback` field through the layer too. So both catch-alls
///   registered BEFORE `layer` end up behind auth — an unauthenticated caller gets 401, never a
///   404/405 that would confirm which paths and methods exist.
/// * `method_not_allowed_fallback` rewrites the fallback of every ALREADY-registered
///   `MethodRouter`. Called AFTER `layer` it would overwrite the layered 405 fallback with an
///   unlayered handler — an unauthenticated 405 escape hatch, and no compile error to warn you.
/// * `layer` (not `route_layer`) is likewise deliberate, though for a narrower reason than the
///   pre-HM-934 comment claimed: `route_layer` maps `path_router` through the same
///   `Endpoint::layer`, so it WOULD authenticate the 405 fallback. What it passes through
///   untouched are `fallback_router` and `catch_all_fallback` — so under `route_layer` the
///   unmatched-path 404 alone would answer without auth, letting a caller enumerate which paths
///   exist by which ones return 401 instead of 404.
///
/// Every future endpoint must therefore be routed into `router` BEFORE it reaches this function.
/// A route added afterwards is neither authenticated nor covered by the 405 fallback.
pub fn seal_control_router<S>(
  router: Router<S>,
  settings: &ControlServerSettings,
) -> Router<S>
where
  S: Clone + Send + Sync + 'static,
{
  router
    .fallback(unmatched_path_handler)
    .method_not_allowed_fallback(method_not_allowed_handler)
    .layer(middleware::from_fn_with_state(settings.clone(), bearer_auth_layer))
}

/// Replaces axum's bare-text 404 for paths that match no route at all.
async fn unmatched_path_handler() -> ControlErrorResponse {
  ControlErrorResponse::new(ControlErrorCode::NotFound, UNMATCHED_PATH_MESSAGE)
}

/// Replaces axum's bare-text 405 for a real route reached with the wrong method.
///
/// NB: Returning no `Allow` header of our own is deliberate — axum's `MethodRouter` appends the
/// accurate one to whatever this handler returns, and setting our own here would override it with
/// a list we would have to keep in sync by hand.
async fn method_not_allowed_handler() -> ControlErrorResponse {
  ControlErrorResponse::new(ControlErrorCode::MethodNotAllowed, METHOD_NOT_ALLOWED_MESSAGE)
}

#[cfg(test)]
mod tests {
  use super::*;
  use axum::body::{to_bytes, Body};
  use axum::http::header::{ALLOW, AUTHORIZATION};
  use axum::http::{HeaderMap, Request, StatusCode};
  use axum::routing::{get, post};
  use axum::Json;
  use serde_json::Value;
  use tower::ServiceExt;

  const GET_ONLY_PATH: &str = "/v1/get_only";
  const POST_ONLY_PATH: &str = "/v1/post_only";
  const UNMATCHED_PATH: &str = "/v1/no_such_endpoint";
  const WRONG_TOKEN: &str = "not-the-token";
  const BODY_LIMIT: usize = 64 * 1024;

  /// How a test request authenticates. The valid token is generated per settings instance, so it
  /// cannot be named as a literal — `Valid` tells the helper to read it back off the router.
  #[derive(Clone, Copy)]
  enum Auth {
    Missing,
    Wrong,
    Valid,
  }

  /// The security property this issue turns on: adding the catch-alls must not create a path that
  /// answers before auth does. An unauthenticated caller must not be able to tell an unmatched
  /// path from a real one.
  mod auth_ordering_tests {
    use super::*;

    #[tokio::test]
    async fn test_unauthenticated_unmatched_path_is_401_not_404() {
      let (status, _, body) = send(Request::get(UNMATCHED_PATH), Auth::Missing).await;

      assert_eq!(status, StatusCode::UNAUTHORIZED);
      assert_eq!(body["error"]["code"], "UNAUTHORIZED");
    }

    #[tokio::test]
    async fn test_unauthenticated_wrong_method_is_401_not_405() {
      let (status, _, body) = send(Request::get(POST_ONLY_PATH), Auth::Missing).await;

      assert_eq!(status, StatusCode::UNAUTHORIZED);
      assert_eq!(body["error"]["code"], "UNAUTHORIZED");
    }

    /// A wrong token must be indistinguishable from no token on both catch-alls.
    #[tokio::test]
    async fn test_wrong_token_is_401_on_both_catch_alls() {
      let (unmatched_status, _, _) = send(Request::get(UNMATCHED_PATH), Auth::Wrong).await;
      let (wrong_method_status, _, _) = send(Request::get(POST_ONLY_PATH), Auth::Wrong).await;

      assert_eq!(unmatched_status, StatusCode::UNAUTHORIZED);
      assert_eq!(wrong_method_status, StatusCode::UNAUTHORIZED);
    }

    /// The two catch-alls must produce the SAME 401 body, or the difference becomes the very
    /// route-probing oracle the 401s exist to close.
    #[tokio::test]
    async fn test_unauthenticated_catch_all_bodies_are_identical() {
      let (_, _, unmatched_body) = send(Request::get(UNMATCHED_PATH), Auth::Missing).await;
      let (_, _, wrong_method_body) = send(Request::get(POST_ONLY_PATH), Auth::Missing).await;

      assert_eq!(unmatched_body, wrong_method_body);
    }

    /// Negative control for `seal_control_router`'s whole reason to exist: registering the 405
    /// fallback AFTER the auth layer silently overwrites the layered fallback with an unlayered
    /// one, so an unauthenticated wrong-method request gets a 405 instead of a 401 — confirming
    /// the route exists. This asserts the hazard is real, so the correct-order tests above are
    /// meaningful rather than vacuous, and so nobody "simplifies" the ordering back out.
    #[tokio::test]
    async fn test_registering_405_fallback_after_auth_layer_would_leak() {
      let settings = ControlServerSettings::new_with_generated_token(0);

      let wrongly_ordered: Router = Router::new()
        .route(POST_ONLY_PATH, post(ok_handler))
        .fallback(unmatched_path_handler)
        .layer(middleware::from_fn_with_state(settings.clone(), bearer_auth_layer))
        .method_not_allowed_fallback(method_not_allowed_handler);

      let request = Request::get(POST_ONLY_PATH)
        .body(Body::empty())
        .expect("test request should build");
      let response = wrongly_ordered.oneshot(request).await.expect("router is infallible");

      assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    /// Characterization test pinning a PRE-EXISTING leak that HM-934 does not introduce and does
    /// not fix: axum's `MethodRouter` appends `Allow` to whatever its fallback branch returns,
    /// including the auth layer's 401. So an unauthenticated caller can still distinguish "real
    /// path, wrong method" (401 + `Allow`) from "no such path" (401, no `Allow`).
    ///
    /// Verified against main's pre-HM-934 shape (routes + auth layer, no fallbacks): that router
    /// already answered `GET /v1/post_only` with 401 and `Allow: POST`. Pinned here so the
    /// follow-up that closes it has a tripwire, and so a future reader does not mistake this for
    /// intended behavior.
    #[tokio::test]
    async fn test_allow_header_still_leaks_method_shape_on_401() {
      let (_, unmatched_headers, _) = send(Request::get(UNMATCHED_PATH), Auth::Missing).await;
      let (_, wrong_method_headers, _) = send(Request::get(POST_ONLY_PATH), Auth::Missing).await;

      assert!(!unmatched_headers.contains_key(ALLOW));
      assert_eq!(wrong_method_headers[ALLOW], "POST");
    }
  }

  mod authenticated_envelope_tests {
    use super::*;

    #[tokio::test]
    async fn test_authenticated_unmatched_path_is_enveloped_404() {
      let (status, _, body) = send(Request::get(UNMATCHED_PATH), Auth::Valid).await;

      assert_eq!(status, StatusCode::NOT_FOUND);
      assert_eq!(body["success"], Value::Bool(false));
      assert_eq!(body["error"]["code"], "NOT_FOUND");
      assert!(body["error"]["message"].is_string());
    }

    #[tokio::test]
    async fn test_authenticated_wrong_method_is_enveloped_405_keeping_allow_header() {
      let (status, headers, body) =
        send(Request::get(POST_ONLY_PATH), Auth::Valid).await;

      assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
      assert_eq!(body["success"], Value::Bool(false));
      assert_eq!(body["error"]["code"], "METHOD_NOT_ALLOWED");
      // axum appends the accurate `Allow` to our handler's response; RFC 9110 requires it on a 405.
      assert_eq!(headers[ALLOW], "POST");
    }

    /// Regression guard: the catch-alls must not shadow the routes they sit beside.
    #[tokio::test]
    async fn test_authenticated_matching_route_still_reaches_its_handler() {
      let (status, _, body) = send(Request::get(GET_ONLY_PATH), Auth::Valid).await;

      assert_eq!(status, StatusCode::OK);
      assert_eq!(body["success"], Value::Bool(true));
    }
  }

  /// Drives a request through a router built by the REAL `seal_control_router`, so these tests
  /// exercise the production ordering rather than a re-creation of it.
  async fn send(
    builder: axum::http::request::Builder,
    auth: Auth,
  ) -> (StatusCode, HeaderMap, Value) {
    let settings = ControlServerSettings::new_with_generated_token(0);

    // NB: `POST_ONLY_PATH` arrives via `merge`, mirroring how the production router pulls in
    // `read_endpoints_router` and friends. `method_not_allowed_fallback` only rewrites the
    // MethodRouters already registered when it runs, so a merged-in route being covered by the
    // 405 fallback is a property worth exercising rather than assuming.
    let routes = Router::new()
      .route(GET_ONLY_PATH, get(ok_handler))
      .merge(Router::new().route(POST_ONLY_PATH, post(ok_handler)));

    let router: Router = seal_control_router(routes, &settings);

    let builder = match auth {
      Auth::Missing => builder,
      Auth::Wrong => builder.header(AUTHORIZATION, format!("Bearer {}", WRONG_TOKEN)),
      Auth::Valid => builder.header(AUTHORIZATION, format!("Bearer {}", settings.token())),
    };

    let request = builder.body(Body::empty()).expect("test request should build");
    let response = router.oneshot(request).await.expect("router is infallible");

    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), BODY_LIMIT)
      .await
      .expect("response body should be readable");
    let body: Value = serde_json::from_slice(&bytes).expect("every response must be JSON enveloped");

    (status, headers, body)
  }

  async fn ok_handler() -> Json<Value> {
    Json(serde_json::json!({ "success": true }))
  }
}

