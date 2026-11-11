use sqlx::{Row, SqlitePool};

use crate::error::{AppError, AppResult};

/// Returns the current total bytes used by all cached content packs on the Host Device.
pub async fn get_used_storage_bytes(pool: &SqlitePool) -> AppResult<i64> {
    let row = sqlx::query("SELECT COALESCE(SUM(size_bytes), 0) AS used FROM host_packs")
        .fetch_one(pool)
        .await?;
    let used: i64 = row.try_get("used")?;
    Ok(used)
}

/// Ensures that `incoming_bytes` can fit within `max_storage_bytes` on the Host Device
/// by evicting the least-recently-used (`last_accessed_at ASC`) unpinned content packs.
/// If an existing pack with `incoming_pack_id` is being replaced, its current size is excluded.
/// Returns the list of evicted `pack_id`s.
pub async fn ensure_storage_capacity(
    pool: &SqlitePool,
    max_storage_bytes: i64,
    incoming_bytes: i64,
    incoming_pack_id: Option<&str>,
) -> AppResult<Vec<String>> {
    if incoming_bytes > max_storage_bytes {
        return Err(AppError::InsufficientStorage(format!(
            "Content pack size ({incoming_bytes} bytes) exceeds total device storage quota ({max_storage_bytes} bytes)"
        )));
    }

    let existing_pack_size: i64 = if let Some(pid) = incoming_pack_id {
        let row = sqlx::query("SELECT size_bytes FROM host_packs WHERE pack_id = ?")
            .bind(pid)
            .fetch_optional(pool)
            .await?;
        row.map(|r| r.get::<i64, _>("size_bytes")).unwrap_or(0)
    } else {
        0
    };

    let mut current_used = get_used_storage_bytes(pool).await? - existing_pack_size;
    if current_used + incoming_bytes <= max_storage_bytes {
        return Ok(Vec::new());
    }

    // Fetch unpinned packs ordered by least recently accessed first
    let candidates = sqlx::query(
        "SELECT pack_id, size_bytes FROM host_packs
         WHERE pinned = 0 AND (? IS NULL OR pack_id != ?)
         ORDER BY last_accessed_at ASC, downloaded_at ASC",
    )
    .bind(incoming_pack_id)
    .bind(incoming_pack_id)
    .fetch_all(pool)
    .await?;

    let mut evicted_ids = Vec::new();

    for row in candidates {
        if current_used + incoming_bytes <= max_storage_bytes {
            break;
        }
        let pack_id: String = row.try_get("pack_id")?;
        let size_bytes: i64 = row.try_get("size_bytes")?;

        sqlx::query("DELETE FROM host_packs WHERE pack_id = ?")
            .bind(&pack_id)
            .execute(pool)
            .await?;

        current_used -= size_bytes;
        evicted_ids.push(pack_id);
    }

    if current_used + incoming_bytes > max_storage_bytes {
        return Err(AppError::InsufficientStorage(format!(
            "Unable to free enough space via LRU eviction (needed {} bytes, available {} bytes after evicting unpinned packs)",
            incoming_bytes,
            max_storage_bytes - current_used
        )));
    }

    Ok(evicted_ids)
}
