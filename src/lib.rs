pub mod db;
pub mod error;
pub mod gamification;
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
use tower_http::{compression::CompressionLayer, cors::CorsLayer, trace::TraceLayer};

use crate::{
    error::AppResult,
    routes::{
        cloud::{
            get_cloud_pack, get_cloud_school_leaderboard, get_cloud_student_leaderboard,
            ingest_cloud_sync, list_cloud_packs, publish_cloud_pack,
        },
        host::{
            complete_lesson, evict_host_packs, execute_cloud_sync, export_sync_payload,
            fetch_school_config, get_host_pack, get_local_leaderboard, get_school_config,
            get_student_profile, list_host_packs, login_student, pull_pack_from_cloud,
            register_student, submit_quiz, update_school_config,
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
        "used_storage_bytes": school.used_storage_bytes,
        "max_storage_bytes": school.max_storage_bytes,
    })))
}

pub fn build_router(pool: SqlitePool) -> Router {
    let state = AppState { pool };

    let host_routes = Router::new()
        .route("/school", get(get_school_config).put(update_school_config))
        .route("/students/register", post(register_student))
        .route("/students/login", post(login_student))
        .route("/students/{student_id}", get(get_student_profile))
        .route("/packs", get(list_host_packs))
        .route("/packs/evict", post(evict_host_packs))
        .route("/packs/{pack_id}", get(get_host_pack))
        .route("/lessons/complete", post(complete_lesson))
        .route("/quizzes/submit", post(submit_quiz))
        .route("/leaderboard", get(get_local_leaderboard))
        .route("/sync/export", post(export_sync_payload))
        .route("/sync/pull-pack", post(pull_pack_from_cloud))
        .route("/sync/execute", post(execute_cloud_sync));

    let cloud_routes = Router::new()
        .route("/packs", get(list_cloud_packs).post(publish_cloud_pack))
        .route("/packs/{pack_id}", get(get_cloud_pack))
        .route("/sync", post(ingest_cloud_sync))
        .route("/leaderboard/schools", get(get_cloud_school_leaderboard))
        .route("/leaderboard/students", get(get_cloud_student_leaderboard));

    Router::new()
        .route("/api/health", get(health_check))
        .nest("/api/host", host_routes)
        .nest("/api/cloud", cloud_routes)
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
    use http_body_util::BodyExt;
    use serde_json::Value;
    use tower::ServiceExt;

    async fn setup_test_app() -> Router {
        let pool = db::init_db("sqlite::memory:").await.unwrap();
        build_router(pool)
    }

    async fn request_json(
        app: &Router,
        method: &str,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let req_builder = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json");

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

    #[tokio::test]
    async fn test_health_and_seeded_nerdc_curriculum() {
        let app = setup_test_app().await;

        let (status, health) = request_json(&app, "GET", "/api/health", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(health["status"], "ok");

        // Cloud catalog should have 3 seeded NERDC/WAEC packs
        let (status, cloud_packs) = request_json(&app, "GET", "/api/cloud/packs", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(cloud_packs.as_array().unwrap().len(), 3);

        // Host device should have 1 pre-installed pack (JSS1 Mathematics)
        let (status, host_packs) = request_json(&app, "GET", "/api/host/packs", None).await;
        assert_eq!(status, StatusCode::OK);
        let packs_arr = host_packs.as_array().unwrap();
        assert_eq!(packs_arr.len(), 1);
        assert_eq!(packs_arr[0]["pack_id"], "nerdc-jss1-math-t1");
    }

    #[tokio::test]
    async fn test_student_lifecycle_gamification_and_local_leaderboard() {
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

        // 2. Verify PIN authentication for Chinedu
        let (bad_status, _) = request_json(
            &app,
            "POST",
            "/api/host/students/login",
            Some(json!({ "nickname": "Chinedu", "pin": "0000" })),
        )
        .await;
        assert_eq!(bad_status, StatusCode::UNAUTHORIZED);

        let (ok_status, _) = request_json(
            &app,
            "POST",
            "/api/host/students/login",
            Some(json!({ "nickname": "Chinedu", "pin": "4321" })),
        )
        .await;
        assert_eq!(ok_status, StatusCode::OK);

        // 3. Chinedu completes Lesson 1 on Day 1
        let (status, les_res) = request_json(
            &app,
            "POST",
            "/api/host/lessons/complete",
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
        assert!(les_res["all_badges"]
            .as_array()
            .unwrap()
            .contains(&json!("first_steps")));

        // 4. Chinedu aces the JSS1 Math Quiz on Day 2 (streak becomes 2, gets perfect quiz bonus + quiz_ace badge)
        let (status, quiz_res) = request_json(
            &app,
            "POST",
            "/api/host/quizzes/submit",
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
        // 3 * 15 + 25 bonus = 70 points
        assert_eq!(quiz_res["points_awarded"], 70);
        assert_eq!(quiz_res["total_points"], 85);
        assert_eq!(quiz_res["streak_days"], 2);
        assert!(quiz_res["all_badges"]
            .as_array()
            .unwrap()
            .contains(&json!("quiz_ace")));

        // 5. Amina completes Lesson 1
        let (status, _) = request_json(
            &app,
            "POST",
            "/api/host/lessons/complete",
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
        assert_eq!(lb_arr[0]["rank"], 1);
        assert_eq!(lb_arr[0]["total_points"], 85);
        assert_eq!(lb_arr[1]["nickname"], "Amina");
        assert_eq!(lb_arr[1]["rank"], 2);
        assert_eq!(lb_arr[1]["total_points"], 15);
    }

    #[tokio::test]
    async fn test_privacy_preserving_cloud_sync_and_national_leaderboard() {
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

        // Earn 85 points
        request_json(
            &app,
            "POST",
            "/api/host/lessons/complete",
            Some(json!({
                "student_id": student_id,
                "pack_id": "nerdc-jss1-math-t1",
                "lesson_id": "les-jss1-math-w1-1"
            })),
        )
        .await;
        request_json(
            &app,
            "POST",
            "/api/host/quizzes/submit",
            Some(json!({
                "student_id": student_id,
                "pack_id": "nerdc-jss1-math-t1",
                "quiz_id": "quiz-jss1-math-w1",
                "answers": [1, 1, 1]
            })),
        )
        .await;

        // Execute Host-to-Cloud sync
        let (status, sync_res) =
            request_json(&app, "POST", "/api/host/sync/execute", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(sync_res["updated_school_points"], 85);
        assert_eq!(sync_res["synced_students_count"], 1);

        // Verify Cloud Student Leaderboard has anonymized ID and moniker, NOT "Oluwaseun"
        let (status, cloud_students) =
            request_json(&app, "GET", "/api/cloud/leaderboard/students", None).await;
        assert_eq!(status, StatusCode::OK);
        let students_arr = cloud_students.as_array().unwrap();
        assert_eq!(students_arr.len(), 1);
        let anon_id = students_arr[0]["anonymized_student_id"].as_str().unwrap();
        let moniker = students_arr[0]["display_moniker"].as_str().unwrap();
        assert!(anon_id.starts_with("anon_"));
        assert!(moniker.starts_with("Scholar #"));
        assert!(!serde_json::to_string(&cloud_students)
            .unwrap()
            .contains("Oluwaseun"));

        // Verify Lagos state filter on Cloud School Leaderboard
        let (status, lagos_schools) = request_json(
            &app,
            "GET",
            "/api/cloud/leaderboard/schools?state=Lagos",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(lagos_schools.as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_lru_storage_eviction_on_pull_pack() {
        let app = setup_test_app().await;

        // Check sizes of cloud packs
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

        // Set max_storage_bytes so it can hold ONLY ONE pack at a time (e.g., max(math_size, sci_size) + 100)
        let tight_quota = math_size.max(sci_size) + 100;
        let (status, _) = request_json(
            &app,
            "PUT",
            "/api/host/school",
            Some(json!({ "max_storage_bytes": tight_quota })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        // Pulling Basic Science pack should automatically evict the older JSS1 Math pack via LRU!
        let (status, pull_res) = request_json(
            &app,
            "POST",
            "/api/host/sync/pull-pack",
            Some(json!({ "pack_id": "nerdc-jss1-basic-sci-t1" })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let evicted = pull_res["evicted_pack_ids"].as_array().unwrap();
        assert_eq!(evicted.len(), 1);
        assert_eq!(evicted[0], "nerdc-jss1-math-t1");

        // Verify Host Device now only has Basic Science cached
        let (_, host_packs) = request_json(&app, "GET", "/api/host/packs", None).await;
        let host_arr = host_packs.as_array().unwrap();
        assert_eq!(host_arr.len(), 1);
        assert_eq!(host_arr[0]["pack_id"], "nerdc-jss1-basic-sci-t1");
    }
}
