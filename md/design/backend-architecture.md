# Backend Architecture & API

The `cort-backend` crate provides the HTTP API, persistence layer, gamification engine, LRU cache manager, static asset / offline audio serving, and offline-to-cloud sync protocol.

## Directory Structure

- `src/main.rs`: CLI entrypoint and server bootstrap.
- `src/lib.rs`: Application router builder (`ServeDir` static + audio serving, `/api/host`, `/api/cloud`) and shared state (`AppState`).
- `src/db.rs`: SQLite schema initialization (WAL mode + `busy_timeout` for 30+ concurrent classroom devices) and seed data (NERDC JSS1 Mathematics, JSS1 Basic Science, and SSS3 WAEC Prep).
- `src/models.rs`: Domain models, sanitized student-facing curriculum views (`StudentContentPack`), and request/response DTOs.
- `src/gamification.rs`: Points calculation, daily streak tracking, and badge awarding rules.
- `src/privacy.rs`: Salted SHA-256 PIN hashing, signed student session tokens (`Authorization: Bearer <token>`), teacher PIN verification (`X-Teacher-Pin`), and student identity anonymization (`SHA-256`) for cloud sync.
- `src/lru.rs`: Least-Recently-Used (LRU) storage quota enforcement for Host Device content packs.
- `src/http_client.rs`: Async HTTP client for syncing a Host Device with a remote `cloud_server_url` over the internet.
- `src/routes/host.rs`: Local Host endpoints for students and teachers in the classroom.
- `src/routes/cloud.rs`: Central Cloud endpoints for content pack distribution and national/state leaderboards.

## API Endpoints

### Health & Static Assets
- `GET /api/health` — Returns service health, mode info, and school metadata.
- `GET /audio/*` — Serves offline lesson audio files from `static/audio/`.
- `GET /*` — Serves the offline PWA frontend bundle from `static/`.

### Local Host API (`/api/host`)
- **School & Teacher Configuration**
  - `GET /api/host/school` — Get configured Host school profile (`school_id`, `school_name`, `state`, `max_storage_bytes`, `cloud_server_url`).
  - `PUT /api/host/school` *(requires `X-Teacher-Pin`)* — Update school profile, storage quota, `cloud_server_url`, or teacher PIN.
  - `POST /api/host/teacher/verify` — Verify teacher PIN for accessing the Teacher Hotspot Dashboard.
- **Student Profiles (Lightweight Offline Auth)**
  - `POST /api/host/students/register` — Register a student (`nickname`, optional 4–8 digit `pin`, `grade_level`); returns profile + signed `session_token`.
  - `POST /api/host/students/login` — Authenticate student by `nickname` + `pin`; returns profile + signed `session_token`.
  - `GET /api/host/students/{student_id}` — Get student profile, points, streak, badges, and progress.
- **Curriculum & Content (with Answer Sanitization & LRU Tracking)**
  - `GET /api/host/packs` — List locally available content packs on the Host Device.
  - `GET /api/host/packs/{pack_id}` — Fetch sanitized student content pack (`StudentContentPack`, with `correct_option_index` and `explanation` stripped) and update `last_accessed_at` for LRU tracking.
  - `POST /api/host/packs/evict` *(requires `X-Teacher-Pin`)* — Manually trigger LRU eviction up to a target byte limit.
- **Learning & Gamification** *(requires `Authorization: Bearer <session_token>`)*
  - `POST /api/host/lessons/complete` — Mark a lesson as completed by a student; awards points, updates streak, and evaluates badges.
  - `POST /api/host/quizzes/submit` — Submit quiz answers; grades submission, reveals per-question explanations, awards points & perfect-score bonuses, updates streak, and awards badges.
- **Offline Leaderboards**
  - `GET /api/host/leaderboard` — Real-time classroom leaderboard sorted by points.
  - `GET /api/host/leaderboard/national` — Cached national or state-filtered (`?state=Lagos`) inter-school leaderboard stored on the Host Device for offline viewing.
- **Offline-First Cloud Sync** *(requires `X-Teacher-Pin`)*
  - `POST /api/host/sync/export` — Generate an anonymized sync payload containing unsynced score deltas and school totals.
  - `POST /api/host/sync/pull-pack` — Pull a content pack from `cloud_server_url` (over HTTP) or the built-in catalog into local storage, automatically running LRU eviction if storage quota is exceeded.
  - `POST /api/host/sync/execute` — Perform a full sync over HTTP (when `cloud_server_url` is set) or in-process, caching the returned national school leaderboard snapshot locally for offline students.

### Cloud Sync API (`/api/cloud`)
- **Curriculum Catalog**
  - `GET /api/cloud/packs` — List all official NERDC curriculum content packs available for download.
  - `GET /api/cloud/packs/{pack_id}` — Download a full content pack JSON bundle.
  - `POST /api/cloud/packs` — Publish a new or updated curriculum content pack.
- **Leaderboard & Sync Ingestion**
  - `POST /api/cloud/sync` — Receive an anonymized sync payload from a Host Device; updates school aggregate scores and anonymized national student records, returning ranks and top national schools snapshot.
  - `GET /api/cloud/leaderboard/schools` — National or state-filtered (`?state=Lagos`) inter-school leaderboard.
  - `GET /api/cloud/leaderboard/students` — National anonymized top students leaderboard.
