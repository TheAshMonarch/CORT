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
    pub cloud_server_url: Option<String>,
    pub last_synced_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSchoolConfigRequest {
    pub school_name: Option<String>,
    pub state: Option<String>,
    pub lga: Option<String>,
    pub max_storage_bytes: Option<i64>,
    pub cloud_server_url: Option<String>,
    pub new_teacher_pin: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyTeacherPinRequest {
    pub pin: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyTeacherPinResponse {
    pub verified: bool,
    pub school_id: String,
    pub school_name: String,
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
pub struct StudentAuthResponse {
    #[serde(flatten)]
    pub profile: StudentProfile,
    pub session_token: String,
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
    pub curriculum_standard: String,
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
    pub difficulty: String,
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

/// Sanitized student-facing view of a ContentPack that strips `correct_option_index`
/// and `explanation` so answers cannot be inspected before submitting a quiz.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudentContentPack {
    pub pack_id: String,
    pub title: String,
    pub subject: String,
    pub grade_level: String,
    pub curriculum_standard: String,
    pub version: String,
    pub description: String,
    pub language: String,
    pub modules: Vec<StudentCurriculumModule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudentCurriculumModule {
    pub module_id: String,
    pub title: String,
    pub term: String,
    pub week: u32,
    pub summary: String,
    pub lessons: Vec<LessonItem>,
    pub quizzes: Vec<StudentQuizItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudentQuizItem {
    pub quiz_id: String,
    pub title: String,
    pub difficulty: String,
    pub questions: Vec<StudentQuizQuestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudentQuizQuestion {
    pub question_id: String,
    pub prompt: String,
    pub options: Vec<String>,
}

impl ContentPack {
    pub fn to_student_view(&self) -> StudentContentPack {
        StudentContentPack {
            pack_id: self.pack_id.clone(),
            title: self.title.clone(),
            subject: self.subject.clone(),
            grade_level: self.grade_level.clone(),
            curriculum_standard: self.curriculum_standard.clone(),
            version: self.version.clone(),
            description: self.description.clone(),
            language: self.language.clone(),
            modules: self
                .modules
                .iter()
                .map(|m| StudentCurriculumModule {
                    module_id: m.module_id.clone(),
                    title: m.title.clone(),
                    term: m.term.clone(),
                    week: m.week,
                    summary: m.summary.clone(),
                    lessons: m.lessons.clone(),
                    quizzes: m
                        .quizzes
                        .iter()
                        .map(|q| StudentQuizItem {
                            quiz_id: q.quiz_id.clone(),
                            title: q.title.clone(),
                            difficulty: q.difficulty.clone(),
                            questions: q
                                .questions
                                .iter()
                                .map(|qq| StudentQuizQuestion {
                                    question_id: qq.question_id.clone(),
                                    prompt: qq.prompt.clone(),
                                    options: qq.options.clone(),
                                })
                                .collect(),
                        })
                        .collect(),
                })
                .collect(),
        }
    }
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
    pub activity_date: Option<String>,
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
    pub national_leaderboard: Vec<CloudSchoolLeaderboardEntry>,
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
