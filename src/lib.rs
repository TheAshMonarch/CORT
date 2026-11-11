pub mod db;
pub mod error;
pub mod gamification;
pub mod http_client;
pub mod lru;
pub mod models;
pub mod privacy;
pub mod routes;

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use tower_http::{
    compression::CompressionLayer,
    cors::CorsLayer,
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};

use crate::{
    error::AppResult,
    routes::{
        cloud::{
            get_cloud_pack, get_cloud_school_leaderboard, get_cloud_student_leaderboard,
            ingest_cloud_sync, list_cloud_packs, publish_cloud_pack,
        },
        host::{
            complete_lesson, evict_host_packs, execute_cloud_sync, export_sync_payload,
            fetch_school_config, get_host_national_leaderboard, get_host_pack,
            get_local_leaderboard, get_school_config, get_student_profile, list_host_packs,
            login_student, pull_pack_from_cloud, register_student, submit_quiz,
            update_school_config, verify_teacher_pin_handler,
        },
    },
};

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
}

async fn health_check(State(state): State<AppState>) -> AppResult<Json<Value>> {
    let school = fetch_school_config(&state.pool).await?;
    Ok(Json(json!({
        "status": "ok",
        "service": "cort-backend",
        "architecture": "unified-host-and-cloud",
        "school_id": school.school_id,
        "school_name": school.school_name,
        "state": school.state,
        "cloud_server_url": school.cloud_server_url,
        "used_storage_bytes": school.used_storage_bytes,
        "max_storage_bytes": school.max_storage_bytes,
    })))
}

pub fn build_router(pool: SqlitePool) -> Router {
    let state = AppState { pool };

    let host_routes = Router::new()
        .route("/school", get(get_school_config).put(update_school_config))
        .route("/teacher/verify", post(verify_teacher_pin_handler))
        .route("/students/register", post(register_student))
        .route("/students/login", post(login_student))
        .route("/students/{student_id}", get(get_student_profile))
        .route("/packs", get(list_host_packs))
        .route("/packs/evict", post(evict_host_packs))
        .route("/packs/{pack_id}", get(get_host_pack))
        .route("/lessons/complete", post(complete_lesson))
        .route("/quizzes/submit", post(submit_quiz))
        .route("/leaderboard", get(get_local_leaderboard))
        .route("/leaderboard/national", get(get_host_national_leaderboard))
        .route("/sync/export", post(export_sync_payload))
        .route("/sync/pull-pack", post(pull_pack_from_cloud))
        .route("/sync/execute", post(execute_cloud_sync));

    let cloud_routes = Router::new()
        .route("/packs", get(list_cloud_packs).post(publish_cloud_pack))
        .route("/packs/{pack_id}", get(get_cloud_pack))
        .route("/sync", post(ingest_cloud_sync))
        .route("/leaderboard/schools", get(get_cloud_school_leaderboard))
        .route("/leaderboard/students", get(get_cloud_student_leaderboard));

    let static_service =
        ServeDir::new("static").not_found_service(ServeFile::new("static/index.html"));

    Router::new()
        .route("/api/health", get(health_check))
        .nest("/api/host", host_routes)
        .nest("/api/cloud", cloud_routes)
        .nest_service("/audio", ServeDir::new("static/audio"))
        .fallback_service(static_service)
        .layer(CompressionLayer::new())
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use db::DEFAULT_TEACHER_PIN;
    use http_body_util::BodyExt;
    use serde_json::Value;
    use tokio::net::TcpListener;
    use tower::ServiceExt;

    async fn setup_test_app() -> Router {
        let pool = db::init_db("sqlite::memory:").await.unwrap();
        build_router(pool)
    }

    async fn request_json_with_headers(
        app: &Router,
        method: &str,
        uri: &str,
        headers: &[(&str, &str)],
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut req_builder = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json");

        for (k, v) in headers {
            req_builder = req_builder.header(*k, *v);
        }

        let req = match body {
            Some(b) => req_builder
                .body(Body::from(serde_json::to_vec(&b).unwrap()))
                .unwrap(),
            None => req_builder.body(Body::empty()).unwrap(),
        };

        let response = app.clone().oneshot(req).await.unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, json)
    }

    async fn request_json(
        app: &Router,
        method: &str,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        request_json_with_headers(app, method, uri, &[], body).await
    }

    #[tokio::test]
    async fn test_health_and_quiz_answers_sanitized_on_host_pack() {
        let app = setup_test_app().await;

        let (status, health) = request_json(&app, "GET", "/api/health", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(health["status"], "ok");

        // Fetch host pack as a student and verify `correct_option_index` and `explanation` are stripped
        let (status, student_pack) =
            request_json(&app, "GET", "/api/host/packs/nerdc-jss1-math-t1", None).await;
        assert_eq!(status, StatusCode::OK);
        let first_question = &student_pack["modules"][0]["quizzes"][0]["questions"][0];
        assert!(first_question.get("prompt").is_some());
        assert!(first_question.get("options").is_some());
        assert!(
            first_question.get("correct_option_index").is_none(),
            "Student view of content pack must not leak correct_option_index"
        );
        assert!(
            first_question.get("explanation").is_none(),
            "Student view of content pack must not leak explanation"
        );
    }

    #[tokio::test]
    async fn test_student_lifecycle_auth_guard_gamification_and_local_leaderboard() {
        let app = setup_test_app().await;

        // 1. Register two students (Chinedu with PIN, Amina without PIN)
        let (status, stu1) = request_json(
            &app,
            "POST",
            "/api/host/students/register",
            Some(json!({
                "nickname": "Chinedu",
                "pin": "4321",
                "grade_level": "JSS1"
            })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        let chinedu_id = stu1["student_id"].as_str().unwrap().to_string();
        let chinedu_token = stu1["session_token"].as_str().unwrap().to_string();
        let chinedu_bearer = format!("Bearer {chinedu_token}");

        let (status, stu2) = request_json(
            &app,
            "POST",
            "/api/host/students/register",
            Some(json!({
                "nickname": "Amina",
                "grade_level": "JSS1"
            })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        let amina_id = stu2["student_id"].as_str().unwrap().to_string();
        let amina_token = stu2["session_token"].as_str().unwrap().to_string();
        let amina_bearer = format!("Bearer {amina_token}");

        // 2. Verify spoofing protection: Amina's token cannot submit lessons for Chinedu
        let (spoof_status, _) = request_json_with_headers(
            &app,
            "POST",
            "/api/host/lessons/complete",
            &[("authorization", &amina_bearer)],
            Some(json!({
                "student_id": chinedu_id,
                "pack_id": "nerdc-jss1-math-t1",
                "lesson_id": "les-jss1-math-w1-1"
            })),
        )
        .await;
        assert_eq!(spoof_status, StatusCode::UNAUTHORIZED);

        // 3. Chinedu completes Lesson 1 on Day 1 using his valid session token
        let (status, les_res) = request_json_with_headers(
            &app,
            "POST",
            "/api/host/lessons/complete",
            &[("authorization", &chinedu_bearer)],
            Some(json!({
                "student_id": chinedu_id,
                "pack_id": "nerdc-jss1-math-t1",
                "lesson_id": "les-jss1-math-w1-1",
                "activity_date": "2026-11-10"
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(les_res["points_awarded"], 15);
        assert_eq!(les_res["streak_days"], 1);

        // 4. Chinedu aces the JSS1 Math Quiz on Day 2
        let (status, quiz_res) = request_json_with_headers(
            &app,
            "POST",
            "/api/host/quizzes/submit",
            &[("authorization", &chinedu_bearer)],
            Some(json!({
                "student_id": chinedu_id,
                "pack_id": "nerdc-jss1-math-t1",
                "quiz_id": "quiz-jss1-math-w1",
                "answers": [1, 1, 1],
                "activity_date": "2026-11-11"
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(quiz_res["is_perfect_score"], true);
        assert_eq!(quiz_res["points_awarded"], 70);
        assert_eq!(quiz_res["total_points"], 85);
        assert_eq!(quiz_res["streak_days"], 2);

        // 5. Amina completes Lesson 1
        let (status, _) = request_json_with_headers(
            &app,
            "POST",
            "/api/host/lessons/complete",
            &[("authorization", &amina_bearer)],
            Some(json!({
                "student_id": amina_id,
                "pack_id": "nerdc-jss1-math-t1",
                "lesson_id": "les-jss1-math-w1-1",
                "activity_date": "2026-11-11"
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        // 6. Check real-time Local Classroom Leaderboard
        let (status, lb) = request_json(&app, "GET", "/api/host/leaderboard", None).await;
        assert_eq!(status, StatusCode::OK);
        let lb_arr = lb.as_array().unwrap();
        assert_eq!(lb_arr.len(), 2);
        assert_eq!(lb_arr[0]["nickname"], "Chinedu");
        assert_eq!(lb_arr[0]["total_points"], 85);
        assert_eq!(lb_arr[1]["nickname"], "Amina");
        assert_eq!(lb_arr[1]["total_points"], 15);
    }

    #[tokio::test]
    async fn test_teacher_pin_protection_and_cached_national_leaderboard() {
        let app = setup_test_app().await;

        let (_, stu) = request_json(
            &app,
            "POST",
            "/api/host/students/register",
            Some(json!({
                "nickname": "Oluwaseun",
                "pin": "1234",
                "grade_level": "JSS1"
            })),
        )
        .await;
        let student_id = stu["student_id"].as_str().unwrap();
        let bearer = format!("Bearer {}", stu["session_token"].as_str().unwrap());

        request_json_with_headers(
            &app,
            "POST",
            "/api/host/quizzes/submit",
            &[("authorization", &bearer)],
            Some(json!({
                "student_id": student_id,
                "pack_id": "nerdc-jss1-math-t1",
                "quiz_id": "quiz-jss1-math-w1",
                "answers": [1, 1, 1]
            })),
        )
        .await;

        // Calling sync without X-Teacher-Pin must be rejected with 401
        let (unauth_status, _) =
            request_json(&app, "POST", "/api/host/sync/execute", None).await;
        assert_eq!(unauth_status, StatusCode::UNAUTHORIZED);

        // Calling sync with valid X-Teacher-Pin succeeds
        let (status, sync_res) = request_json_with_headers(
            &app,
            "POST",
            "/api/host/sync/execute",
            &[("x-teacher-pin", DEFAULT_TEACHER_PIN)],
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(sync_res["updated_school_points"], 70);

        // Verify offline cached national leaderboard on Host Device now includes our school!
        let (status, host_nat_lb) = request_json(
            &app,
            "GET",
            "/api/host/leaderboard/national?state=Lagos",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let lagos_arr = host_nat_lb.as_array().unwrap();
        assert!(lagos_arr
            .iter()
            .any(|s| s["school_id"] == "sch_lagos_ikeja_01"));
    }

    #[tokio::test]
    async fn test_lru_storage_eviction_on_pull_pack() {
        let app = setup_test_app().await;

        let (_, cloud_packs) = request_json(&app, "GET", "/api/cloud/packs", None).await;
        let packs = cloud_packs.as_array().unwrap();
        let math_size = packs
            .iter()
            .find(|p| p["pack_id"] == "nerdc-jss1-math-t1")
            .unwrap()["size_bytes"]
            .as_i64()
            .unwrap();
        let sci_size = packs
            .iter()
            .find(|p| p["pack_id"] == "nerdc-jss1-basic-sci-t1")
            .unwrap()["size_bytes"]
            .as_i64()
            .unwrap();

        let tight_quota = math_size.max(sci_size) + 100;
        let (status, _) = request_json_with_headers(
            &app,
            "PUT",
            "/api/host/school",
            &[("x-teacher-pin", DEFAULT_TEACHER_PIN)],
            Some(json!({ "max_storage_bytes": tight_quota })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, pull_res) = request_json_with_headers(
            &app,
            "POST",
            "/api/host/sync/pull-pack",
            &[("x-teacher-pin", DEFAULT_TEACHER_PIN)],
            Some(json!({ "pack_id": "nerdc-jss1-basic-sci-t1" })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let evicted = pull_res["evicted_pack_ids"].as_array().unwrap();
        assert_eq!(evicted.len(), 1);
        assert_eq!(evicted[0], "nerdc-jss1-math-t1");
    }

    #[tokio::test]
    async fn test_real_http_host_to_cloud_sync_between_two_separate_servers() {
        // 1. Spin up a dedicated Cloud Server on a real TCP port with its own database
        let cloud_pool = db::init_db("sqlite::memory:").await.unwrap();
        let cloud_app = build_router(cloud_pool);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let cloud_addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, cloud_app).await.unwrap();
        });

        let cloud_base_url = format!("http://{cloud_addr}");

        // 2. Create a separate Host Device database & router
        let host_pool = db::init_db("sqlite::memory:").await.unwrap();
        let host_app = build_router(host_pool);

        // Configure Host Device to point to the remote Cloud Server URL
        let (status, cfg) = request_json_with_headers(
            &host_app,
            "PUT",
            "/api/host/school",
            &[("x-teacher-pin", DEFAULT_TEACHER_PIN)],
            Some(json!({ "cloud_server_url": cloud_base_url })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(cfg["cloud_server_url"], cloud_base_url);

        // 3. Pull SSS3 WAEC Prep pack from the remote Cloud Server over real HTTP!
        let (status, pull_res) = request_json_with_headers(
            &host_app,
            "POST",
            "/api/host/sync/pull-pack",
            &[("x-teacher-pin", DEFAULT_TEACHER_PIN)],
            Some(json!({ "pack_id": "nerdc-sss3-waec-math-prep" })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            pull_res["downloaded_pack"]["pack_id"],
            "nerdc-sss3-waec-math-prep"
        );

        // 4. Register a student on Host Device, complete the newly pulled WAEC lesson, and sync over HTTP
        let (_, stu) = request_json(
            &host_app,
            "POST",
            "/api/host/students/register",
            Some(json!({ "nickname": "Tunde", "grade_level": "SSS3" })),
        )
        .await;
        let stu_id = stu["student_id"].as_str().unwrap();
        let bearer = format!("Bearer {}", stu["session_token"].as_str().unwrap());

        let (status, _) = request_json_with_headers(
            &host_app,
            "POST",
            "/api/host/lessons/complete",
            &[("authorization", &bearer)],
            Some(json!({
                "student_id": stu_id,
                "pack_id": "nerdc-sss3-waec-math-prep",
                "lesson_id": "les-sss3-math-w1-1"
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        // Execute sync over HTTP to remote Cloud Server
        let (status, sync_res) = request_json_with_headers(
            &host_app,
            "POST",
            "/api/host/sync/execute",
            &[("x-teacher-pin", DEFAULT_TEACHER_PIN)],
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(sync_res["updated_school_points"], 20);
        assert_eq!(sync_res["synced_students_count"], 1);
    }
}
