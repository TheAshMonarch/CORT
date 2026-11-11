# Backend Architecture & API

The `cort-backend` crate provides the HTTP API, persistence layer, gamification engine, LRU cache manager, and offline-to-cloud sync protocol.

## Directory Structure

- `src/main.rs`: CLI entrypoint and server bootstrap.
- `src/lib.rs`: Application router builder and shared state (`AppState`).
- `src/db.rs`: SQLite schema initialization and seed data (NERDC JSS1 Mathematics & Basic Science).
- `src/models.rs`: Domain models, request/response DTOs, and database rows.
- `src/gamification.rs`: Points calculation, daily streak tracking, and badge awarding rules.
- `src/privacy.rs`: PIN hashing and student identity anonymization (`SHA-256`) for cloud sync.
- `src/lru.rs`: Least-Recently-Used (LRU) storage quota enforcement for Host Device content packs.
- `src/routes/host.rs`: Local Host endpoints for students and teachers in the classroom.
- `src/routes/cloud.rs`: Central Cloud endpoints for content pack distribution and national/state leaderboards.

## API Endpoints

### Health & Info
- `GET /api/health` — Returns service health, mode info, and school metadata.

### Local Host API (`/api/host`)
- **School Configuration**
  - `GET /api/host/school` — Get configured Host school profile (school ID, name, state, storage quota).
  - `PUT /api/host/school` — Update school profile and storage quota (`max_storage_bytes`).
- **Student Profiles (Lightweight Offline Auth)**
  - `POST /api/host/students/register` — Register a student (`nickname`, optional 4-digit `pin`, `grade_level`).
  - `POST /api/host/students/login` — Authenticate student by `nickname` + `pin`.
  - `GET /api/host/students/{student_id}` — Get student profile, points, streak, badges, and progress.
- **Curriculum & Content (with LRU tracking)**
  - `GET /api/host/packs` — List locally available content packs on the Host Device.
  - `GET /api/host/packs/{pack_id}` — Fetch full content pack (modules, lessons, quizzes) and update `last_accessed_at` for LRU tracking.
  - `POST /api/host/packs/evict` — Manually trigger LRU eviction up to a target byte limit.
- **Learning & Gamification**
  - `POST /api/host/lessons/complete` — Mark a lesson as completed by a student; awards points, updates streak, and evaluates badges.
  - `POST /api/host/quizzes/submit` — Submit quiz answers; grades submission, awards points & perfect-score bonuses, updates streak, and awards badges.
- **Local Leaderboards**
  - `GET /api/host/leaderboard` — Real-time classroom leaderboard sorted by points.
- **Offline-First Cloud Sync**
  - `POST /api/host/sync/export` — Generate an anonymized sync payload containing unsynced score deltas and school totals.
  - `POST /api/host/sync/pull-pack` — Import a content pack into local storage (automatically running LRU eviction if storage quota is exceeded).
  - `POST /api/host/sync/execute` — Perform a full in-process or remote sync with the Cloud catalog and leaderboard.

### Cloud Sync API (`/api/cloud`)
- **Curriculum Catalog**
  - `GET /api/cloud/packs` — List all official NERDC curriculum content packs available for download.
  - `GET /api/cloud/packs/{pack_id}` — Download a full content pack JSON bundle.
  - `POST /api/cloud/packs` — Publish a new or updated curriculum content pack.
- **Leaderboard & Sync Ingestion**
  - `POST /api/cloud/sync` — Receive an anonymized sync payload from a Host Device; updates school aggregate scores and anonymized national student records.
  - `GET /api/cloud/leaderboard/schools` — National or state-filtered (`?state=Lagos`) inter-school leaderboard.
  - `GET /api/cloud/leaderboard/students` — National anonymized top students leaderboard.
