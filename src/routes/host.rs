use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::{
    db::get_school_salt,
    error::{AppError, AppResult},
    gamification::{
        compute_updated_streak, evaluate_badges, DEFAULT_LESSON_POINTS, PERFECT_QUIZ_BONUS_POINTS,
        POINTS_PER_CORRECT_ANSWER,
    },
    lru::{ensure_storage_capacity, get_used_storage_bytes},
    models::{
        AnonymizedStudentSyncRecord, CloudSyncResult, CompleteLessonRequest,
        CompleteLessonResponse, ContentPack, EvictStorageRequest, EvictStorageResponse,
        HostPackSummary, LocalLeaderboardEntry, LoginStudentRequest, PullPackRequest,
        PullPackResponse, QuestionFeedback, RegisterStudentRequest, SchoolConfig, StudentProfile,
        SubmitQuizRequest, SubmitQuizResponse, SyncUploadPayload, UpdateSchoolConfigRequest,
    },
    privacy::{anonymize_student_id, anonymized_display_moniker, hash_pin, validate_pin_format},
    routes::cloud::process_cloud_sync_payload,
    AppState,
};

// ============================================================================
// School Config Endpoints
// ============================================================================

pub async fn get_school_config(State(state): State<AppState>) -> AppResult<Json<SchoolConfig>> {
    let config = fetch_school_config(&state.pool).await?;
    Ok(Json(config))
}

pub async fn fetch_school_config(pool: &SqlitePool) -> AppResult<SchoolConfig> {
    let row = sqlx::query(
        "SELECT school_id, school_name, state, lga, max_storage_bytes, last_synced_at
         FROM host_school_config WHERE id = 1",
    )
    .fetch_one(pool)
    .await?;

    let used_storage_bytes = get_used_storage_bytes(pool).await?;

    Ok(SchoolConfig {
        school_id: row.try_get("school_id")?,
        school_name: row.try_get("school_name")?,
        state: row.try_get("state")?,
        lga: row.try_get("lga")?,
        max_storage_bytes: row.try_get("max_storage_bytes")?,
        used_storage_bytes,
        last_synced_at: row.try_get("last_synced_at")?,
    })
}

pub async fn update_school_config(
    State(state): State<AppState>,
    Json(req): Json<UpdateSchoolConfigRequest>,
) -> AppResult<Json<SchoolConfig>> {
    let current = fetch_school_config(&state.pool).await?;

    let school_name = req.school_name.unwrap_or(current.school_name);
    let state_name = req.state.unwrap_or(current.state);
    let lga = req.lga.unwrap_or(current.lga);
    let max_storage_bytes = req.max_storage_bytes.unwrap_or(current.max_storage_bytes);

    if max_storage_bytes <= 0 {
        return Err(AppError::BadRequest(
            "max_storage_bytes must be positive".to_string(),
        ));
    }

    sqlx::query(
        "UPDATE host_school_config
         SET school_name = ?, state = ?, lga = ?, max_storage_bytes = ?
         WHERE id = 1",
    )
    .bind(&school_name)
    .bind(&state_name)
    .bind(&lga)
    .bind(max_storage_bytes)
    .execute(&state.pool)
    .await?;

    let updated = fetch_school_config(&state.pool).await?;
    Ok(Json(updated))
}

// ============================================================================
// Student Registration, Login & Profile Endpoints
// ============================================================================

pub async fn register_student(
    State(state): State<AppState>,
    Json(req): Json<RegisterStudentRequest>,
) -> AppResult<(StatusCode, Json<StudentProfile>)> {
    let nickname = req.nickname.trim().to_string();
    if nickname.len() < 2 || nickname.len() > 40 {
        return Err(AppError::BadRequest(
            "Nickname must be between 2 and 40 characters".to_string(),
        ));
    }

    let existing: Option<String> = sqlx::query_scalar(
        "SELECT student_id FROM host_students WHERE LOWER(nickname) = LOWER(?)",
    )
    .bind(&nickname)
    .fetch_optional(&state.pool)
    .await?;

    if existing.is_some() {
        return Err(AppError::Conflict(format!(
            "A student with nickname '{nickname}' is already registered on this Host Device"
        )));
    }

    let pin_hash = if let Some(ref pin) = req.pin {
        validate_pin_format(pin)?;
        let salt = get_school_salt(&state.pool).await?;
        Some(hash_pin(pin, &salt))
    } else {
        None
    };

    let student_id = format!("stu_{}", &Uuid::new_v4().simple().to_string()[..12]);
    let grade_level = req
        .grade_level
        .unwrap_or_else(|| "JSS1".to_string())
        .trim()
        .to_uppercase();
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO host_students (student_id, nickname, pin_hash, grade_level, total_points, unsynced_points, streak_days, last_activity_date, badges_json, created_at)
         VALUES (?, ?, ?, ?, 0, 0, 0, NULL, '[]', ?)",
    )
    .bind(&student_id)
    .bind(&nickname)
    .bind(&pin_hash)
    .bind(&grade_level)
    .bind(&now)
    .execute(&state.pool)
    .await?;

    let profile = fetch_student_profile(&state.pool, &student_id).await?;
    Ok((StatusCode::CREATED, Json(profile)))
}

pub async fn login_student(
    State(state): State<AppState>,
    Json(req): Json<LoginStudentRequest>,
) -> AppResult<Json<StudentProfile>> {
    let row = sqlx::query(
        "SELECT student_id, pin_hash FROM host_students WHERE LOWER(nickname) = LOWER(?)",
    )
    .bind(req.nickname.trim())
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Student nickname not found".to_string()))?;

    let student_id: String = row.try_get("student_id")?;
    let stored_pin_hash: Option<String> = row.try_get("pin_hash")?;

    if let Some(expected_hash) = stored_pin_hash {
        let provided_pin = req.pin.as_deref().ok_or_else(|| {
            AppError::Unauthorized("PIN is required for this student profile".to_string())
        })?;
        let salt = get_school_salt(&state.pool).await?;
        let actual_hash = hash_pin(provided_pin, &salt);
        if actual_hash != expected_hash {
            return Err(AppError::Unauthorized("Invalid PIN".to_string()));
        }
    }

    let profile = fetch_student_profile(&state.pool, &student_id).await?;
    Ok(Json(profile))
}

pub async fn get_student_profile(
    State(state): State<AppState>,
    Path(student_id): Path<String>,
) -> AppResult<Json<StudentProfile>> {
    let profile = fetch_student_profile(&state.pool, &student_id).await?;
    Ok(Json(profile))
}

pub async fn fetch_student_profile(
    pool: &SqlitePool,
    student_id: &str,
) -> AppResult<StudentProfile> {
    let row = sqlx::query(
        "SELECT student_id, nickname, grade_level, total_points, unsynced_points, streak_days, last_activity_date, badges_json, created_at
         FROM host_students WHERE student_id = ?",
    )
    .bind(student_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Student '{student_id}' not found")))?;

    let badges_json: String = row.try_get("badges_json")?;
    let badges: Vec<String> = serde_json::from_str(&badges_json).unwrap_or_default();

    let lesson_rows = sqlx::query(
        "SELECT lesson_id FROM host_lesson_completions WHERE student_id = ? ORDER BY completed_at ASC",
    )
    .bind(student_id)
    .fetch_all(pool)
    .await?;

    let completed_lessons = lesson_rows
        .into_iter()
        .filter_map(|r| r.try_get::<String, _>("lesson_id").ok())
        .collect();

    let quiz_attempts_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM host_quiz_attempts WHERE student_id = ?")
            .bind(student_id)
            .fetch_one(pool)
            .await?;

    Ok(StudentProfile {
        student_id: row.try_get("student_id")?,
        nickname: row.try_get("nickname")?,
        grade_level: row.try_get("grade_level")?,
        total_points: row.try_get("total_points")?,
        unsynced_points: row.try_get("unsynced_points")?,
        streak_days: row.try_get("streak_days")?,
        last_activity_date: row.try_get("last_activity_date")?,
        badges,
        completed_lessons,
        quiz_attempts_count,
        created_at: row.try_get("created_at")?,
    })
}

// ============================================================================
// Host Content Packs & LRU Storage Management
// ============================================================================

pub async fn list_host_packs(
    State(state): State<AppState>,
) -> AppResult<Json<Vec<HostPackSummary>>> {
    let rows = sqlx::query(
        "SELECT pack_id, title, subject, grade_level, curriculum_standard, version, size_bytes, pinned, downloaded_at, last_accessed_at
         FROM host_packs
         ORDER BY last_accessed_at DESC",
    )
    .fetch_all(&state.pool)
    .await?;

    let mut packs = Vec::with_capacity(rows.len());
    for row in rows {
        let pinned_int: i64 = row.try_get("pinned")?;
        packs.push(HostPackSummary {
            pack_id: row.try_get("pack_id")?,
            title: row.try_get("title")?,
            subject: row.try_get("subject")?,
            grade_level: row.try_get("grade_level")?,
            curriculum_standard: row.try_get("curriculum_standard")?,
            version: row.try_get("version")?,
            size_bytes: row.try_get("size_bytes")?,
            pinned: pinned_int != 0,
            downloaded_at: row.try_get("downloaded_at")?,
            last_accessed_at: row.try_get("last_accessed_at")?,
        });
    }

    Ok(Json(packs))
}

/// Fetches a cached content pack on the Host Device and updates its `last_accessed_at` timestamp
/// so active packs are retained by the LRU cache.
pub async fn get_host_pack(
    State(state): State<AppState>,
    Path(pack_id): Path<String>,
) -> AppResult<Json<ContentPack>> {
    let row = sqlx::query("SELECT content_json FROM host_packs WHERE pack_id = ?")
        .bind(&pack_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "Content pack '{pack_id}' is not cached on this Host Device"
            ))
        })?;

    let now = Utc::now().to_rfc3339();
    sqlx::query("UPDATE host_packs SET last_accessed_at = ? WHERE pack_id = ?")
        .bind(&now)
        .bind(&pack_id)
        .execute(&state.pool)
        .await?;

    let content_json: String = row.try_get("content_json")?;
    let pack: ContentPack = serde_json::from_str(&content_json)?;
    Ok(Json(pack))
}

/// Manually triggers LRU cache eviction to bring Host Device storage usage within `target_max_bytes`
/// (or the school's configured `max_storage_bytes`).
pub async fn evict_host_packs(
    State(state): State<AppState>,
    Json(req): Json<EvictStorageRequest>,
) -> AppResult<Json<EvictStorageResponse>> {
    let config = fetch_school_config(&state.pool).await?;
    let target_budget = req.target_max_bytes.unwrap_or(config.max_storage_bytes);

    if target_budget < 0 {
        return Err(AppError::BadRequest(
            "target_max_bytes cannot be negative".to_string(),
        ));
    }

    let evicted_pack_ids = ensure_storage_capacity(&state.pool, target_budget, 0, None).await?;
    let used_storage_bytes = get_used_storage_bytes(&state.pool).await?;

    Ok(Json(EvictStorageResponse {
        evicted_pack_ids,
        used_storage_bytes,
        max_storage_bytes: config.max_storage_bytes,
    }))
}

// ============================================================================
// Learning Progress: Complete Lesson & Submit Quiz
// ============================================================================

pub async fn complete_lesson(
    State(state): State<AppState>,
    Json(req): Json<CompleteLessonRequest>,
) -> AppResult<Json<CompleteLessonResponse>> {
    // 1. Verify student exists
    let student = fetch_student_profile(&state.pool, &req.student_id).await?;

    // 2. Verify pack & lesson exist on the Host Device and touch LRU timestamp
    let pack_row = sqlx::query("SELECT content_json FROM host_packs WHERE pack_id = ?")
        .bind(&req.pack_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "Content pack '{}' not found on Host Device",
                req.pack_id
            ))
        })?;

    let content_json: String = pack_row.try_get("content_json")?;
    let pack: ContentPack = serde_json::from_str(&content_json)?;

    let lesson = pack
        .modules
        .iter()
        .flat_map(|m| m.lessons.iter())
        .find(|l| l.lesson_id == req.lesson_id)
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "Lesson '{}' not found in pack '{}'",
                req.lesson_id, req.pack_id
            ))
        })?;

    let now_rfc = Utc::now().to_rfc3339();
    sqlx::query("UPDATE host_packs SET last_accessed_at = ? WHERE pack_id = ?")
        .bind(&now_rfc)
        .bind(&req.pack_id)
        .execute(&state.pool)
        .await?;

    // 3. Check if already completed
    let already_done: Option<i64> = sqlx::query_scalar(
        "SELECT points_awarded FROM host_lesson_completions WHERE student_id = ? AND lesson_id = ?",
    )
    .bind(&req.student_id)
    .bind(&req.lesson_id)
    .fetch_optional(&state.pool)
    .await?;

    if already_done.is_some() {
        return Ok(Json(CompleteLessonResponse {
            student_id: student.student_id,
            lesson_id: req.lesson_id,
            already_completed: true,
            points_awarded: 0,
            total_points: student.total_points,
            streak_days: student.streak_days,
            newly_awarded_badges: Vec::new(),
            all_badges: student.badges,
        }));
    }

    // 4. Award points, update streak & badges
    let points_awarded = if lesson.points_reward > 0 {
        lesson.points_reward
    } else {
        DEFAULT_LESSON_POINTS
    };

    let today_str = req
        .activity_date
        .unwrap_or_else(|| Utc::now().format("%Y-%m-%d").to_string());

    let new_streak = compute_updated_streak(
        student.streak_days,
        student.last_activity_date.as_deref(),
        &today_str,
    );
    let new_total_points = student.total_points + points_awarded;
    let new_unsynced_points = student.unsynced_points + points_awarded;
    let completed_lessons_count = (student.completed_lessons.len() as i64) + 1;

    let (all_badges, newly_awarded_badges) = evaluate_badges(
        &student.badges,
        new_total_points,
        new_streak,
        completed_lessons_count,
        student.quiz_attempts_count,
        false,
    );
    let badges_json = serde_json::to_string(&all_badges)?;

    sqlx::query(
        "INSERT INTO host_lesson_completions (student_id, pack_id, lesson_id, points_awarded, completed_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&req.student_id)
    .bind(&req.pack_id)
    .bind(&req.lesson_id)
    .bind(points_awarded)
    .bind(&now_rfc)
    .execute(&state.pool)
    .await?;

    sqlx::query(
        "UPDATE host_students
         SET total_points = ?, unsynced_points = ?, streak_days = ?, last_activity_date = ?, badges_json = ?
         WHERE student_id = ?",
    )
    .bind(new_total_points)
    .bind(new_unsynced_points)
    .bind(new_streak)
    .bind(&today_str)
    .bind(&badges_json)
    .bind(&req.student_id)
    .execute(&state.pool)
    .await?;

    Ok(Json(CompleteLessonResponse {
        student_id: req.student_id,
        lesson_id: req.lesson_id,
        already_completed: false,
        points_awarded,
        total_points: new_total_points,
        streak_days: new_streak,
        newly_awarded_badges,
        all_badges,
    }))
}

pub async fn submit_quiz(
    State(state): State<AppState>,
    Json(req): Json<SubmitQuizRequest>,
) -> AppResult<Json<SubmitQuizResponse>> {
    // 1. Verify student exists
    let student = fetch_student_profile(&state.pool, &req.student_id).await?;

    // 2. Verify pack & quiz exist on the Host Device
    let pack_row = sqlx::query("SELECT content_json FROM host_packs WHERE pack_id = ?")
        .bind(&req.pack_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "Content pack '{}' not found on Host Device",
                req.pack_id
            ))
        })?;

    let content_json: String = pack_row.try_get("content_json")?;
    let pack: ContentPack = serde_json::from_str(&content_json)?;

    let quiz = pack
        .modules
        .iter()
        .flat_map(|m| m.quizzes.iter())
        .find(|q| q.quiz_id == req.quiz_id)
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "Quiz '{}' not found in pack '{}'",
                req.quiz_id, req.pack_id
            ))
        })?;

    let now_rfc = Utc::now().to_rfc3339();
    sqlx::query("UPDATE host_packs SET last_accessed_at = ? WHERE pack_id = ?")
        .bind(&now_rfc)
        .bind(&req.pack_id)
        .execute(&state.pool)
        .await?;

    // 3. Grade quiz answers
    let total_questions = quiz.questions.len();
    let mut correct_count = 0usize;
    let mut feedback = Vec::with_capacity(total_questions);

    for (idx, q) in quiz.questions.iter().enumerate() {
        let selected = req.answers.get(idx).copied();
        let is_correct = selected == Some(q.correct_option_index);
        if is_correct {
            correct_count += 1;
        }
        feedback.push(QuestionFeedback {
            question_id: q.question_id.clone(),
            selected_option_index: selected,
            correct_option_index: q.correct_option_index,
            is_correct,
            explanation: q.explanation.clone(),
        });
    }

    let is_perfect_score = total_questions > 0 && correct_count == total_questions;
    let score_percent = if total_questions > 0 {
        ((correct_count as f64 / total_questions as f64) * 100.0).round() as u32
    } else {
        0
    };

    // Only award points above the student's previous best on the same quiz to prevent point farming
    let previous_best_points: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(points_awarded), 0) FROM host_quiz_attempts WHERE student_id = ? AND quiz_id = ?",
    )
    .bind(&req.student_id)
    .bind(&req.quiz_id)
    .fetch_one(&state.pool)
    .await?;

    let raw_points = (correct_count as i64) * POINTS_PER_CORRECT_ANSWER
        + if is_perfect_score {
            PERFECT_QUIZ_BONUS_POINTS
        } else {
            0
        };
    let points_awarded = (raw_points - previous_best_points).max(0);

    let today_str = req
        .activity_date
        .unwrap_or_else(|| Utc::now().format("%Y-%m-%d").to_string());

    let new_streak = compute_updated_streak(
        student.streak_days,
        student.last_activity_date.as_deref(),
        &today_str,
    );
    let new_total_points = student.total_points + points_awarded;
    let new_unsynced_points = student.unsynced_points + points_awarded;

    let (all_badges, newly_awarded_badges) = evaluate_badges(
        &student.badges,
        new_total_points,
        new_streak,
        student.completed_lessons.len() as i64,
        student.quiz_attempts_count + 1,
        is_perfect_score,
    );
    let badges_json = serde_json::to_string(&all_badges)?;

    let attempt_id = format!("qz_att_{}", &Uuid::new_v4().simple().to_string()[..12]);

    sqlx::query(
        "INSERT INTO host_quiz_attempts (attempt_id, student_id, pack_id, quiz_id, correct_count, total_questions, points_awarded, is_perfect, attempted_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&attempt_id)
    .bind(&req.student_id)
    .bind(&req.pack_id)
    .bind(&req.quiz_id)
    .bind(correct_count as i64)
    .bind(total_questions as i64)
    .bind(raw_points)
    .bind(if is_perfect_score { 1_i64 } else { 0_i64 })
    .bind(&now_rfc)
    .execute(&state.pool)
    .await?;

    sqlx::query(
        "UPDATE host_students
         SET total_points = ?, unsynced_points = ?, streak_days = ?, last_activity_date = ?, badges_json = ?
         WHERE student_id = ?",
    )
    .bind(new_total_points)
    .bind(new_unsynced_points)
    .bind(new_streak)
    .bind(&today_str)
    .bind(&badges_json)
    .bind(&req.student_id)
    .execute(&state.pool)
    .await?;

    Ok(Json(SubmitQuizResponse {
        attempt_id,
        student_id: req.student_id,
        quiz_id: req.quiz_id,
        correct_count,
        total_questions,
        score_percent,
        is_perfect_score,
        points_awarded,
        total_points: new_total_points,
        streak_days: new_streak,
        newly_awarded_badges,
        all_badges,
        feedback,
    }))
}

// ============================================================================
// Local Classroom Leaderboard (Offline Real-time)
// ============================================================================

pub async fn get_local_leaderboard(
    State(state): State<AppState>,
) -> AppResult<Json<Vec<LocalLeaderboardEntry>>> {
    let rows = sqlx::query(
        "SELECT student_id, nickname, grade_level, total_points, streak_days, badges_json
         FROM host_students
         ORDER BY total_points DESC, streak_days DESC, nickname ASC",
    )
    .fetch_all(&state.pool)
    .await?;

    let mut leaderboard = Vec::with_capacity(rows.len());
    for (idx, row) in rows.into_iter().enumerate() {
        let badges_json: String = row.try_get("badges_json")?;
        let badges: Vec<String> = serde_json::from_str(&badges_json).unwrap_or_default();

        leaderboard.push(LocalLeaderboardEntry {
            rank: idx + 1,
            student_id: row.try_get("student_id")?,
            nickname: row.try_get("nickname")?,
            grade_level: row.try_get("grade_level")?,
            total_points: row.try_get("total_points")?,
            streak_days: row.try_get("streak_days")?,
            badges_count: badges.len(),
        });
    }

    Ok(Json(leaderboard))
}

// ============================================================================
// Offline-to-Cloud Sync Operations
// ============================================================================

/// Generates an anonymized sync payload from local student scores without exposing PII.
pub async fn export_sync_payload(
    State(state): State<AppState>,
) -> AppResult<Json<SyncUploadPayload>> {
    let payload = build_sync_upload_payload(&state.pool).await?;
    Ok(Json(payload))
}

pub async fn build_sync_upload_payload(pool: &SqlitePool) -> AppResult<SyncUploadPayload> {
    let config = fetch_school_config(pool).await?;
    let salt = get_school_salt(pool).await?;

    let rows = sqlx::query(
        "SELECT student_id, grade_level, total_points, unsynced_points, streak_days, badges_json
         FROM host_students
         ORDER BY total_points DESC",
    )
    .fetch_all(pool)
    .await?;

    let mut total_school_points = 0_i64;
    let mut student_records = Vec::with_capacity(rows.len());

    for row in rows {
        let student_id: String = row.try_get("student_id")?;
        let grade_level: String = row.try_get("grade_level")?;
        let total_points: i64 = row.try_get("total_points")?;
        let unsynced_points: i64 = row.try_get("unsynced_points")?;
        let streak_days: i64 = row.try_get("streak_days")?;
        let badges_json: String = row.try_get("badges_json")?;
        let badges: Vec<String> = serde_json::from_str(&badges_json).unwrap_or_default();

        total_school_points += total_points;

        let anon_id = anonymize_student_id(&student_id, &salt);
        let display_moniker = anonymized_display_moniker(&anon_id);

        student_records.push(AnonymizedStudentSyncRecord {
            anonymized_student_id: anon_id,
            display_moniker,
            grade_level,
            total_points,
            delta_points: unsynced_points,
            streak_days,
            badges_count: badges.len(),
        });
    }

    Ok(SyncUploadPayload {
        school_id: config.school_id,
        school_name: config.school_name,
        state: config.state,
        lga: config.lga,
        total_school_points,
        active_students_count: student_records.len(),
        student_records,
        synced_at: Utc::now().to_rfc3339(),
    })
}

/// Pulls a content pack from the Cloud Catalog into the Host Device's local storage,
/// automatically running LRU cache eviction if the device storage quota would be exceeded.
pub async fn pull_pack_from_cloud(
    State(state): State<AppState>,
    Json(req): Json<PullPackRequest>,
) -> AppResult<Json<PullPackResponse>> {
    let cloud_row = sqlx::query(
        "SELECT pack_id, title, subject, grade_level, curriculum_standard, version, size_bytes, content_json
         FROM cloud_packs WHERE pack_id = ?",
    )
    .bind(&req.pack_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| {
        AppError::NotFound(format!(
            "Content pack '{}' not found in Cloud Catalog",
            req.pack_id
        ))
    })?;

    let size_bytes: i64 = cloud_row.try_get("size_bytes")?;
    let config = fetch_school_config(&state.pool).await?;

    // Ensure LRU storage capacity before writing pack to Host Device
    let evicted_pack_ids = ensure_storage_capacity(
        &state.pool,
        config.max_storage_bytes,
        size_bytes,
        Some(&req.pack_id),
    )
    .await?;

    let now = Utc::now().to_rfc3339();
    let pinned = req.pinned.unwrap_or(false);
    let pinned_int: i64 = if pinned { 1 } else { 0 };

    let title: String = cloud_row.try_get("title")?;
    let subject: String = cloud_row.try_get("subject")?;
    let grade_level: String = cloud_row.try_get("grade_level")?;
    let curriculum_standard: String = cloud_row.try_get("curriculum_standard")?;
    let version: String = cloud_row.try_get("version")?;
    let content_json: String = cloud_row.try_get("content_json")?;

    sqlx::query(
        "INSERT INTO host_packs (pack_id, title, subject, grade_level, curriculum_standard, version, size_bytes, pinned, downloaded_at, last_accessed_at, content_json)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(pack_id) DO UPDATE SET
            title = excluded.title,
            subject = excluded.subject,
            grade_level = excluded.grade_level,
            curriculum_standard = excluded.curriculum_standard,
            version = excluded.version,
            size_bytes = excluded.size_bytes,
            pinned = excluded.pinned,
            downloaded_at = excluded.downloaded_at,
            last_accessed_at = excluded.last_accessed_at,
            content_json = excluded.content_json",
    )
    .bind(&req.pack_id)
    .bind(&title)
    .bind(&subject)
    .bind(&grade_level)
    .bind(&curriculum_standard)
    .bind(&version)
    .bind(size_bytes)
    .bind(pinned_int)
    .bind(&now)
    .bind(&now)
    .bind(&content_json)
    .execute(&state.pool)
    .await?;

    let used_storage_bytes = get_used_storage_bytes(&state.pool).await?;

    Ok(Json(PullPackResponse {
        downloaded_pack: HostPackSummary {
            pack_id: req.pack_id,
            title,
            subject,
            grade_level,
            curriculum_standard,
            version,
            size_bytes,
            pinned,
            downloaded_at: now.clone(),
            last_accessed_at: now,
        },
        evicted_pack_ids,
        used_storage_bytes,
        max_storage_bytes: config.max_storage_bytes,
    }))
}

/// Executes a full Host-to-Cloud synchronization:
/// 1. Builds the anonymized student + school score payload.
/// 2. Uploads it to the Cloud Sync engine.
/// 3. Resets `unsynced_points = 0` on local student records and updates `last_synced_at`.
/// 4. Returns updated national/state school ranks and available Cloud curriculum packs.
pub async fn execute_cloud_sync(
    State(state): State<AppState>,
) -> AppResult<Json<CloudSyncResult>> {
    let payload = build_sync_upload_payload(&state.pool).await?;
    let sync_time = payload.synced_at.clone();

    let result = process_cloud_sync_payload(&state.pool, payload).await?;

    sqlx::query("UPDATE host_students SET unsynced_points = 0")
        .execute(&state.pool)
        .await?;

    sqlx::query("UPDATE host_school_config SET last_synced_at = ? WHERE id = 1")
        .bind(&sync_time)
        .execute(&state.pool)
        .await?;

    Ok(Json(result))
}
