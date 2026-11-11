use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use serde::Deserialize;
use sqlx::{Row, SqlitePool};

use crate::{
    error::{AppError, AppResult},
    models::{
        CloudPackSummary, CloudSchoolLeaderboardEntry, CloudStudentLeaderboardEntry,
        CloudSyncResult, ContentPack, SyncUploadPayload,
    },
    AppState,
};

#[derive(Debug, Deserialize)]
pub struct LeaderboardFilter {
    pub state: Option<String>,
    pub limit: Option<i64>,
}

/// Lists all available NERDC curriculum content packs in the Cloud Catalog.
pub async fn list_cloud_packs(
    State(state): State<AppState>,
) -> AppResult<Json<Vec<CloudPackSummary>>> {
    let summaries = fetch_cloud_pack_summaries(&state.pool).await?;
    Ok(Json(summaries))
}

pub async fn fetch_cloud_pack_summaries(pool: &SqlitePool) -> AppResult<Vec<CloudPackSummary>> {
    let rows = sqlx::query(
        "SELECT pack_id, title, subject, grade_level, curriculum_standard, version, description, size_bytes, published_at
         FROM cloud_packs
         ORDER BY grade_level ASC, subject ASC",
    )
    .fetch_all(pool)
    .await?;

    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        result.push(CloudPackSummary {
            pack_id: row.try_get("pack_id")?,
            title: row.try_get("title")?,
            subject: row.try_get("subject")?,
            grade_level: row.try_get("grade_level")?,
            curriculum_standard: row.try_get("curriculum_standard")?,
            version: row.try_get("version")?,
            description: row.try_get("description")?,
            size_bytes: row.try_get("size_bytes")?,
            published_at: row.try_get("published_at")?,
        });
    }
    Ok(result)
}

/// Downloads a specific content pack bundle by `pack_id`.
pub async fn get_cloud_pack(
    State(state): State<AppState>,
    Path(pack_id): Path<String>,
) -> AppResult<Json<ContentPack>> {
    let row = sqlx::query("SELECT content_json FROM cloud_packs WHERE pack_id = ?")
        .bind(&pack_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Cloud content pack '{pack_id}' not found")))?;

    let content_json: String = row.try_get("content_json")?;
    let pack: ContentPack = serde_json::from_str(&content_json)?;
    Ok(Json(pack))
}

/// Publishes or updates a curriculum content pack in the Cloud Catalog.
pub async fn publish_cloud_pack(
    State(state): State<AppState>,
    Json(pack): Json<ContentPack>,
) -> AppResult<(StatusCode, Json<CloudPackSummary>)> {
    if pack.pack_id.trim().is_empty() || pack.title.trim().is_empty() {
        return Err(AppError::BadRequest(
            "pack_id and title must not be empty".to_string(),
        ));
    }

    let now = Utc::now().to_rfc3339();
    let content_json = serde_json::to_string(&pack)?;
    let size_bytes = content_json.len() as i64;

    sqlx::query(
        "INSERT INTO cloud_packs (pack_id, title, subject, grade_level, curriculum_standard, version, description, size_bytes, published_at, content_json)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(pack_id) DO UPDATE SET
            title = excluded.title,
            subject = excluded.subject,
            grade_level = excluded.grade_level,
            curriculum_standard = excluded.curriculum_standard,
            version = excluded.version,
            description = excluded.description,
            size_bytes = excluded.size_bytes,
            published_at = excluded.published_at,
            content_json = excluded.content_json",
    )
    .bind(&pack.pack_id)
    .bind(&pack.title)
    .bind(&pack.subject)
    .bind(&pack.grade_level)
    .bind(&pack.curriculum_standard)
    .bind(&pack.version)
    .bind(&pack.description)
    .bind(size_bytes)
    .bind(&now)
    .bind(&content_json)
    .execute(&state.pool)
    .await?;

    let summary = CloudPackSummary {
        pack_id: pack.pack_id,
        title: pack.title,
        subject: pack.subject,
        grade_level: pack.grade_level,
        curriculum_standard: pack.curriculum_standard,
        version: pack.version,
        description: pack.description,
        size_bytes,
        published_at: now,
    };

    Ok((StatusCode::CREATED, Json(summary)))
}

/// Ingests an anonymized sync payload from a Host Device, updating national & state leaderboards.
pub async fn ingest_cloud_sync(
    State(state): State<AppState>,
    Json(payload): Json<SyncUploadPayload>,
) -> AppResult<Json<CloudSyncResult>> {
    let result = process_cloud_sync_payload(&state.pool, payload).await?;
    Ok(Json(result))
}

pub async fn process_cloud_sync_payload(
    pool: &SqlitePool,
    payload: SyncUploadPayload,
) -> AppResult<CloudSyncResult> {
    let now = Utc::now().to_rfc3339();

    // Upsert school leaderboard record
    sqlx::query(
        "INSERT INTO cloud_school_leaderboard (school_id, school_name, state, lga, total_points, active_students_count, last_synced_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(school_id) DO UPDATE SET
            school_name = excluded.school_name,
            state = excluded.state,
            lga = excluded.lga,
            total_points = excluded.total_points,
            active_students_count = excluded.active_students_count,
            last_synced_at = excluded.last_synced_at",
    )
    .bind(&payload.school_id)
    .bind(&payload.school_name)
    .bind(&payload.state)
    .bind(&payload.lga)
    .bind(payload.total_school_points)
    .bind(payload.active_students_count as i64)
    .bind(&now)
    .execute(pool)
    .await?;

    // Upsert anonymized student leaderboard records
    for student in &payload.student_records {
        sqlx::query(
            "INSERT INTO cloud_student_leaderboard (anonymized_student_id, display_moniker, school_id, school_name, state, grade_level, total_points, streak_days, badges_count, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(anonymized_student_id) DO UPDATE SET
                display_moniker = excluded.display_moniker,
                school_name = excluded.school_name,
                state = excluded.state,
                grade_level = excluded.grade_level,
                total_points = excluded.total_points,
                streak_days = excluded.streak_days,
                badges_count = excluded.badges_count,
                updated_at = excluded.updated_at",
        )
        .bind(&student.anonymized_student_id)
        .bind(&student.display_moniker)
        .bind(&payload.school_id)
        .bind(&payload.school_name)
        .bind(&payload.state)
        .bind(&student.grade_level)
        .bind(student.total_points)
        .bind(student.streak_days)
        .bind(student.badges_count as i64)
        .bind(&now)
        .execute(pool)
        .await?;
    }

    // Compute national rank of this school
    let higher_national: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM cloud_school_leaderboard WHERE total_points > ?",
    )
    .bind(payload.total_school_points)
    .fetch_one(pool)
    .await?;

    // Compute state rank of this school
    let higher_state: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM cloud_school_leaderboard WHERE LOWER(state) = LOWER(?) AND total_points > ?",
    )
    .bind(&payload.state)
    .bind(payload.total_school_points)
    .fetch_one(pool)
    .await?;

    let available_packs = fetch_cloud_pack_summaries(pool).await?;

    Ok(CloudSyncResult {
        school_id: payload.school_id,
        updated_school_points: payload.total_school_points,
        school_national_rank: (higher_national as usize) + 1,
        school_state_rank: (higher_state as usize) + 1,
        synced_students_count: payload.student_records.len(),
        available_packs,
    })
}

/// Returns the national or state-level school leaderboard.
pub async fn get_cloud_school_leaderboard(
    State(state): State<AppState>,
    Query(filter): Query<LeaderboardFilter>,
) -> AppResult<Json<Vec<CloudSchoolLeaderboardEntry>>> {
    let limit = filter.limit.unwrap_or(50).clamp(1, 200);

    let rows = if let Some(ref state_filter) = filter.state {
        sqlx::query(
            "SELECT school_id, school_name, state, lga, total_points, active_students_count, last_synced_at
             FROM cloud_school_leaderboard
             WHERE LOWER(state) = LOWER(?)
             ORDER BY total_points DESC, active_students_count DESC, school_name ASC
             LIMIT ?",
        )
        .bind(state_filter)
        .bind(limit)
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query(
            "SELECT school_id, school_name, state, lga, total_points, active_students_count, last_synced_at
             FROM cloud_school_leaderboard
             ORDER BY total_points DESC, active_students_count DESC, school_name ASC
             LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&state.pool)
        .await?
    };

    let mut entries = Vec::with_capacity(rows.len());
    for (idx, row) in rows.into_iter().enumerate() {
        entries.push(CloudSchoolLeaderboardEntry {
            rank: idx + 1,
            school_id: row.try_get("school_id")?,
            school_name: row.try_get("school_name")?,
            state: row.try_get("state")?,
            lga: row.try_get("lga")?,
            total_points: row.try_get("total_points")?,
            active_students_count: row.try_get("active_students_count")?,
            last_synced_at: row.try_get("last_synced_at")?,
        });
    }

    Ok(Json(entries))
}

/// Returns the national or state-level anonymized student leaderboard.
pub async fn get_cloud_student_leaderboard(
    State(state): State<AppState>,
    Query(filter): Query<LeaderboardFilter>,
) -> AppResult<Json<Vec<CloudStudentLeaderboardEntry>>> {
    let limit = filter.limit.unwrap_or(50).clamp(1, 200);

    let rows = if let Some(ref state_filter) = filter.state {
        sqlx::query(
            "SELECT anonymized_student_id, display_moniker, school_name, state, grade_level, total_points, streak_days, badges_count
             FROM cloud_student_leaderboard
             WHERE LOWER(state) = LOWER(?)
             ORDER BY total_points DESC, streak_days DESC, anonymized_student_id ASC
             LIMIT ?",
        )
        .bind(state_filter)
        .bind(limit)
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query(
            "SELECT anonymized_student_id, display_moniker, school_name, state, grade_level, total_points, streak_days, badges_count
             FROM cloud_student_leaderboard
             ORDER BY total_points DESC, streak_days DESC, anonymized_student_id ASC
             LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&state.pool)
        .await?
    };

    let mut entries = Vec::with_capacity(rows.len());
    for (idx, row) in rows.into_iter().enumerate() {
        entries.push(CloudStudentLeaderboardEntry {
            rank: idx + 1,
            anonymized_student_id: row.try_get("anonymized_student_id")?,
            display_moniker: row.try_get("display_moniker")?,
            school_name: row.try_get("school_name")?,
            state: row.try_get("state")?,
            grade_level: row.try_get("grade_level")?,
            total_points: row.try_get("total_points")?,
            streak_days: row.try_get("streak_days")?,
            badges_count: row.try_get("badges_count")?,
        });
    }

    Ok(Json(entries))
}
