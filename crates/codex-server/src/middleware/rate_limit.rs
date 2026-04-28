use crate::middleware::auth::UserId;
use actix_web::body::{EitherBody, MessageBody};
use actix_web::dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::{Error, HttpMessage, HttpResponse};
use futures::future::{ready, LocalBoxFuture, Ready};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Mutex;
use std::time::Instant;

/// Per-IP and per-user token-bucket rate limiter.
///
/// Authenticated requests are bucketed by user ID; unauthenticated requests
/// fall back to IP address. This middleware must run *after* `AuthMiddleware`
/// in the actix-web stack so that `UserId` is already in request extensions.
///
/// Configured via environment variables or defaults:
/// - `RATE_LIMIT_REQUESTS`: max requests per window for **IP** buckets (default 120)
/// - `RATE_LIMIT_USER_REQUESTS`: max requests per window for **user** buckets (default 300)
/// - `RATE_LIMIT_UPLOAD_REQUESTS`: max upload requests per window (default 2000)
/// - `RATE_LIMIT_WINDOW_SECS`: shared window size in seconds (default 60)
pub struct RateLimitMiddleware;

impl<S, B> Transform<S, ServiceRequest> for RateLimitMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Transform = RateLimitService<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        let max_requests = std::env::var("RATE_LIMIT_REQUESTS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(120u64);
        let max_user_requests = std::env::var("RATE_LIMIT_USER_REQUESTS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(300u64);
        let max_upload_requests = std::env::var("RATE_LIMIT_UPLOAD_REQUESTS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2000u64);
        let window_secs = std::env::var("RATE_LIMIT_WINDOW_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(60u64);

        ready(Ok(RateLimitService {
            service: Rc::new(service),
            buckets: Rc::new(Mutex::new(HashMap::new())),
            max_requests,
            max_user_requests,
            max_upload_requests,
            window_secs,
        }))
    }
}

struct BucketEntry {
    count: u64,
    window_start: Instant,
}

pub struct RateLimitService<S> {
    service: Rc<S>,
    buckets: Rc<Mutex<HashMap<String, BucketEntry>>>,
    max_requests: u64,
    max_user_requests: u64,
    max_upload_requests: u64,
    window_secs: u64,
}

impl<S, B> Service<ServiceRequest> for RateLimitService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        // Skip rate limiting for health checks and static assets.
        let path = req.path().to_string();
        if path == "/" || path.starts_with("/assets") {
            let fut = self.service.call(req);
            return Box::pin(async move { Ok(fut.await?.map_into_left_body()) });
        }

        // If AuthMiddleware has already run, prefer per-user bucketing.
        // Prefix keys so IP and user buckets never collide.
        //
        // IMPORTANT: extract the UserId into an owned value BEFORE the if-let
        // so the Ref<Extensions> borrow is dropped at the semicolon.  If we
        // wrote `if let Some(u) = req.extensions().get::<UserId>().cloned()`
        // the Ref would live until the end of the whole if-else, causing a
        // RefCell double-borrow panic when the else branch calls
        // `req.connection_info()` (which calls extensions_mut internally).
        let maybe_user_id = req.extensions().get::<UserId>().cloned();
        let is_upload_request = is_upload_path(&path);
        let (key, max) = if let Some(user_id) = maybe_user_id {
            if is_upload_request {
                (
                    format!("upload:user:{}", user_id.0),
                    self.max_upload_requests,
                )
            } else {
                (format!("user:{}", user_id.0), self.max_user_requests)
            }
        } else {
            let ip = req
                .connection_info()
                .peer_addr()
                .unwrap_or("unknown")
                .to_string();
            if is_upload_request {
                (format!("upload:ip:{}", ip), self.max_upload_requests)
            } else {
                (format!("ip:{}", ip), self.max_requests)
            }
        };

        let now = Instant::now();
        let window = std::time::Duration::from_secs(self.window_secs);

        let allowed = {
            let mut buckets = self.buckets.lock().unwrap();
            let entry = buckets.entry(key).or_insert(BucketEntry {
                count: 0,
                window_start: now,
            });

            // Reset window if expired.
            if now.duration_since(entry.window_start) >= window {
                entry.count = 0;
                entry.window_start = now;
            }

            entry.count += 1;
            entry.count <= max
        };

        if !allowed {
            let window_secs = self.window_secs;
            let response = HttpResponse::TooManyRequests()
                .insert_header(("Retry-After", window_secs.to_string()))
                .json(serde_json::json!({
                    "error": "RATE_LIMITED",
                    "message": format!("Rate limit exceeded. Max {} requests per {} seconds.", max, window_secs),
                }));
            return Box::pin(async move { Ok(req.into_response(response).map_into_right_body()) });
        }

        let fut = self.service.call(req);
        Box::pin(async move { Ok(fut.await?.map_into_left_body()) })
    }
}

fn is_upload_path(path: &str) -> bool {
    path.starts_with("/api/vaults/")
        && (path.ends_with("/upload") || path.contains("/upload-sessions"))
}

#[cfg(test)]
mod tests {
    use super::RateLimitMiddleware;
    use actix_web::{http::StatusCode, test, web, App, HttpResponse};
    use std::sync::{Mutex, OnceLock};

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    struct RateLimitEnvGuard;

    impl RateLimitEnvGuard {
        fn set(requests: u64, user_requests: u64, upload_requests: u64, window_secs: u64) -> Self {
            std::env::set_var("RATE_LIMIT_REQUESTS", requests.to_string());
            std::env::set_var("RATE_LIMIT_USER_REQUESTS", user_requests.to_string());
            std::env::set_var("RATE_LIMIT_UPLOAD_REQUESTS", upload_requests.to_string());
            std::env::set_var("RATE_LIMIT_WINDOW_SECS", window_secs.to_string());
            Self
        }
    }

    impl Drop for RateLimitEnvGuard {
        fn drop(&mut self) {
            std::env::remove_var("RATE_LIMIT_REQUESTS");
            std::env::remove_var("RATE_LIMIT_USER_REQUESTS");
            std::env::remove_var("RATE_LIMIT_UPLOAD_REQUESTS");
            std::env::remove_var("RATE_LIMIT_WINDOW_SECS");
        }
    }

    #[actix_web::test]
    async fn upload_requests_use_separate_higher_bucket() {
        let _lock = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = RateLimitEnvGuard::set(2, 2, 4, 60);

        let app = test::init_service(
            App::new()
                .wrap(RateLimitMiddleware)
                .default_service(web::to(|| async { HttpResponse::Ok().finish() })),
        )
        .await;

        for _ in 0..4 {
            let req = test::TestRequest::put()
                .uri("/api/vaults/vault-1/upload-sessions/session-1")
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), StatusCode::OK);
        }

        let req = test::TestRequest::put()
            .uri("/api/vaults/vault-1/upload-sessions/session-1")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[actix_web::test]
    async fn upload_requests_do_not_consume_general_api_bucket() {
        let _lock = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = RateLimitEnvGuard::set(2, 2, 4, 60);

        let app = test::init_service(
            App::new()
                .wrap(RateLimitMiddleware)
                .default_service(web::to(|| async { HttpResponse::Ok().finish() })),
        )
        .await;

        for _ in 0..4 {
            let req = test::TestRequest::post()
                .uri("/api/vaults/vault-1/upload-sessions")
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), StatusCode::OK);
        }

        for _ in 0..2 {
            let req = test::TestRequest::get().uri("/api/files").to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), StatusCode::OK);
        }

        let req = test::TestRequest::get().uri("/api/files").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    }
}
