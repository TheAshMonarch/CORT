use serde::{Deserialize, Serialize};

use crate::gamification::BadgeInfo;

// ============================================================================
// School & Host Device Configuration
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchoolConfig {
    pub school_id: String,
    pub school_name: String,
    pub state: String,
    pub lga: String,
    pub max_storage_bytes: i64,
    pub used_storage_bytes: i64,
    pub last_synced_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSchoolConfigRequest {
    pub school_name: Option<String>,
    pub state: Option<String>,
    pub lga: Option<String>,
    pub max_storage_bytes: Option<i64>,
}

// ============================================================================
// Student Profiles & Lightweight Offline Auth
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudentProfile {
    pub student_id: String,
    pub nickname: String,
    pub grade_level: String,
    pub total_points: i64,
    pub unsynced_points: i64,
    pub streak_days: i64,
    pub last_activity_date: Option<String>,
    pub badges: Vec<String>,
    pub completed_lessons: Vec<String>,
    pub quiz_attempts_count: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterStudentRequest {
    pub nickname: String,
    pub pin: Option<String>,
    pub grade_level: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginStudentRequest {
    pub nickname: String,
    pub pin: Option<String>,
}

// ============================================================================
// Curriculum Content Packs (NERDC / WAEC / JAMB / BECE Aligned)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentPack {
    pub pack_id: String,
    pub title: String,
    pub subject: String,
    pub grade_level: String,
    pub curriculum_standard: String, // e.g., "NERDC 9-Year Basic Education", "WAEC/JAMB"
    pub version: String,
    pub description: String,
    pub language: String,
    pub modules: Vec<CurriculumModule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurriculumModule {
    pub module_id: String,
    pub title: String,
    pub term: String,
    pub week: u32,
    pub summary: String,
    pub lessons: Vec<LessonItem>,
    pub quizzes: Vec<QuizItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LessonItem {
    pub lesson_id: String,
    pub title: String,
    pub content_markdown: String,
    pub audio_url: Option<String>,
    pub estimated_minutes: u32,
    pub points_reward: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuizItem {
    pub quiz_id: String,
    pub title: String,
    pub difficulty: String, // "easy", "medium", "hard"
    pub questions: Vec<QuizQuestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuizQuestion {
    pub question_id: String,
    pub prompt: String,
    pub options: Vec<String>,
    pub correct_option_index: usize,
    pub explanation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostPackSummary {
    pub pack_id: String,
    pub title: String,
    pub subject: String,
    pub grade_level: String,
    pub curriculum_standard: String,
    pub version: String,
    pub size_bytes: i64,
    pub pinned: bool,
    pub downloaded_at: String,
    pub last_accessed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudPackSummary {
    pub pack_id: String,
    pub title: String,
    pub subject: String,
    pub grade_level: String,
    pub curriculum_standard: String,
    pub version: String,
    pub description: String,
    pub size_bytes: i64,
    pub published_at: String,
}

// ============================================================================
// Learning Progress & Gamification Actions
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteLessonRequest {
    pub student_id: String,
    pub pack_id: String,
    pub lesson_id: String,
    pub activity_date: Option<String>, // Optional YYYY-MM-DD override for testing streaks
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteLessonResponse {
    pub student_id: String,
    pub lesson_id: String,
    pub already_completed: bool,
    pub points_awarded: i64,
    pub total_points: i64,
    pub streak_days: i64,
    pub newly_awarded_badges: Vec<BadgeInfo>,
    pub all_badges: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitQuizRequest {
    pub student_id: String,
    pub pack_id: String,
    pub quiz_id: String,
    /// Selected option index for each question in the quiz, in order
    pub answers: Vec<usize>,
    pub activity_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionFeedback {
    pub question_id: String,
    pub selected_option_index: Option<usize>,
    pub correct_option_index: usize,
    pub is_correct: bool,
    pub explanation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitQuizResponse {
    pub attempt_id: String,
    pub student_id: String,
    pub quiz_id: String,
    pub correct_count: usize,
    pub total_questions: usize,
    pub score_percent: u32,
    pub is_perfect_score: bool,
    pub points_awarded: i64,
    pub total_points: i64,
    pub streak_days: i64,
    pub newly_awarded_badges: Vec<BadgeInfo>,
    pub all_badges: Vec<String>,
    pub feedback: Vec<QuestionFeedback>,
}

// ============================================================================
// Leaderboards & Offline-to-Cloud Sync
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalLeaderboardEntry {
    pub rank: usize,
    pub student_id: String,
    pub nickname: String,
    pub grade_level: String,
    pub total_points: i64,
    pub streak_days: i64,
    pub badges_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnonymizedStudentSyncRecord {
    pub anonymized_student_id: String,
    pub display_moniker: String,
    pub grade_level: String,
    pub total_points: i64,
    pub delta_points: i64,
    pub streak_days: i64,
    pub badges_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncUploadPayload {
    pub school_id: String,
    pub school_name: String,
    pub state: String,
    pub lga: String,
    pub total_school_points: i64,
    pub active_students_count: usize,
    pub student_records: Vec<AnonymizedStudentSyncRecord>,
    pub synced_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudSyncResult {
    pub school_id: String,
    pub updated_school_points: i64,
    pub school_national_rank: usize,
    pub school_state_rank: usize,
    pub synced_students_count: usize,
    pub available_packs: Vec<CloudPackSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullPackRequest {
    pub pack_id: String,
    pub pinned: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullPackResponse {
    pub downloaded_pack: HostPackSummary,
    pub evicted_pack_ids: Vec<String>,
    pub used_storage_bytes: i64,
    pub max_storage_bytes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvictStorageRequest {
    pub target_max_bytes: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvictStorageResponse {
    pub evicted_pack_ids: Vec<String>,
    pub used_storage_bytes: i64,
    pub max_storage_bytes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudSchoolLeaderboardEntry {
    pub rank: usize,
    pub school_id: String,
    pub school_name: String,
    pub state: String,
    pub lga: String,
    pub total_points: i64,
    pub active_students_count: i64,
    pub last_synced_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudStudentLeaderboardEntry {
    pub rank: usize,
    pub anonymized_student_id: String,
    pub display_moniker: String,
    pub school_name: String,
    pub state: String,
    pub grade_level: String,
    pub total_points: i64,
    pub streak_days: i64,
    pub badges_count: i64,
}
