use chrono::Utc;
use sqlx::{sqlite::SqlitePoolOptions, Row, SqlitePool};

use crate::{
    error::AppResult,
    models::{ContentPack, CurriculumModule, LessonItem, QuizItem, QuizQuestion},
};

pub async fn init_db(database_url: &str) -> AppResult<SqlitePool> {
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?;

    run_migrations(&pool).await?;
    seed_defaults(&pool).await?;

    Ok(pool)
}

async fn run_migrations(pool: &SqlitePool) -> AppResult<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS host_school_config (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            school_id TEXT NOT NULL,
            school_name TEXT NOT NULL,
            state TEXT NOT NULL,
            lga TEXT NOT NULL,
            school_salt TEXT NOT NULL,
            max_storage_bytes INTEGER NOT NULL,
            last_synced_at TEXT
        );

        CREATE TABLE IF NOT EXISTS host_students (
            student_id TEXT PRIMARY KEY,
            nickname TEXT NOT NULL UNIQUE COLLATE NOCASE,
            pin_hash TEXT,
            grade_level TEXT NOT NULL,
            total_points INTEGER NOT NULL DEFAULT 0,
            unsynced_points INTEGER NOT NULL DEFAULT 0,
            streak_days INTEGER NOT NULL DEFAULT 0,
            last_activity_date TEXT,
            badges_json TEXT NOT NULL DEFAULT '[]',
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS host_lesson_completions (
            student_id TEXT NOT NULL,
            pack_id TEXT NOT NULL,
            lesson_id TEXT NOT NULL,
            points_awarded INTEGER NOT NULL,
            completed_at TEXT NOT NULL,
            PRIMARY KEY (student_id, lesson_id)
        );

        CREATE TABLE IF NOT EXISTS host_quiz_attempts (
            attempt_id TEXT PRIMARY KEY,
            student_id TEXT NOT NULL,
            pack_id TEXT NOT NULL,
            quiz_id TEXT NOT NULL,
            correct_count INTEGER NOT NULL,
            total_questions INTEGER NOT NULL,
            points_awarded INTEGER NOT NULL,
            is_perfect INTEGER NOT NULL,
            attempted_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS host_packs (
            pack_id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            subject TEXT NOT NULL,
            grade_level TEXT NOT NULL,
            curriculum_standard TEXT NOT NULL,
            version TEXT NOT NULL,
            size_bytes INTEGER NOT NULL,
            pinned INTEGER NOT NULL DEFAULT 0,
            downloaded_at TEXT NOT NULL,
            last_accessed_at TEXT NOT NULL,
            content_json TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS cloud_packs (
            pack_id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            subject TEXT NOT NULL,
            grade_level TEXT NOT NULL,
            curriculum_standard TEXT NOT NULL,
            version TEXT NOT NULL,
            description TEXT NOT NULL,
            size_bytes INTEGER NOT NULL,
            published_at TEXT NOT NULL,
            content_json TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS cloud_school_leaderboard (
            school_id TEXT PRIMARY KEY,
            school_name TEXT NOT NULL,
            state TEXT NOT NULL,
            lga TEXT NOT NULL,
            total_points INTEGER NOT NULL DEFAULT 0,
            active_students_count INTEGER NOT NULL DEFAULT 0,
            last_synced_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS cloud_student_leaderboard (
            anonymized_student_id TEXT PRIMARY KEY,
            display_moniker TEXT NOT NULL,
            school_id TEXT NOT NULL,
            school_name TEXT NOT NULL,
            state TEXT NOT NULL,
            grade_level TEXT NOT NULL,
            total_points INTEGER NOT NULL DEFAULT 0,
            streak_days INTEGER NOT NULL DEFAULT 0,
            badges_count INTEGER NOT NULL DEFAULT 0,
            updated_at TEXT NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

async fn seed_defaults(pool: &SqlitePool) -> AppResult<()> {
    // 1. Ensure default Host School Config exists
    let school_exists: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM host_school_config WHERE id = 1")
            .fetch_one(pool)
            .await?;

    if school_exists == 0 {
        sqlx::query(
            "INSERT INTO host_school_config (id, school_id, school_name, state, lga, school_salt, max_storage_bytes, last_synced_at)
             VALUES (1, ?, ?, ?, ?, ?, ?, NULL)",
        )
        .bind("sch_lagos_ikeja_01")
        .bind("Community Junior Secondary School, Ikeja")
        .bind("Lagos")
        .bind("Ikeja")
        .bind("salt_cort_nigeria_ikeja_2026")
        .bind(50 * 1024 * 1024_i64) // 50 MB default quota
        .execute(pool)
        .await?;
    }

    // 2. Seed official NERDC Curriculum Content Packs into Cloud Catalog
    let cloud_pack_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cloud_packs")
        .fetch_one(pool)
        .await?;

    if cloud_pack_count == 0 {
        let now = Utc::now().to_rfc3339();
        let starter_packs = build_nerdc_starter_packs();

        for (index, pack) in starter_packs.iter().enumerate() {
            let content_json = serde_json::to_string(pack)?;
            let size_bytes = content_json.len() as i64;

            sqlx::query(
                "INSERT INTO cloud_packs (pack_id, title, subject, grade_level, curriculum_standard, version, description, size_bytes, published_at, content_json)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
            .execute(pool)
            .await?;

            // Pre-install the first pack (JSS1 Mathematics) on the Host Device out-of-the-box
            if index == 0 {
                sqlx::query(
                    "INSERT INTO host_packs (pack_id, title, subject, grade_level, curriculum_standard, version, size_bytes, pinned, downloaded_at, last_accessed_at, content_json)
                     VALUES (?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?)",
                )
                .bind(&pack.pack_id)
                .bind(&pack.title)
                .bind(&pack.subject)
                .bind(&pack.grade_level)
                .bind(&pack.curriculum_standard)
                .bind(&pack.version)
                .bind(size_bytes)
                .bind(&now)
                .bind(&now)
                .bind(&content_json)
                .execute(pool)
                .await?;
            }
        }
    }

    // 3. Seed peer schools in the Cloud National Leaderboard for inter-school competition context
    let school_lb_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cloud_school_leaderboard")
        .fetch_one(pool)
        .await?;

    if school_lb_count == 0 {
        let now = Utc::now().to_rfc3339();
        let peer_schools = [
            (
                "sch_kano_municipal_01",
                "Government Junior Secondary School, Kano Municipal",
                "Kano",
                "Kano Municipal",
                420_i64,
                18_i64,
            ),
            (
                "sch_enugu_nsukka_01",
                "Urban Community Secondary School, Nsukka",
                "Enugu",
                "Nsukka",
                365_i64,
                14_i64,
            ),
            (
                "sch_lagos_yaba_02",
                "Birrel Avenue Junior High School, Yaba",
                "Lagos",
                "Yaba",
                290_i64,
                12_i64,
            ),
        ];

        for (sid, name, state, lga, pts, count) in peer_schools {
            sqlx::query(
                "INSERT INTO cloud_school_leaderboard (school_id, school_name, state, lga, total_points, active_students_count, last_synced_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(sid)
            .bind(name)
            .bind(state)
            .bind(lga)
            .bind(pts)
            .bind(count)
            .bind(&now)
            .execute(pool)
            .await?;
        }
    }

    Ok(())
}

pub async fn get_school_salt(pool: &SqlitePool) -> AppResult<String> {
    let row = sqlx::query("SELECT school_salt FROM host_school_config WHERE id = 1")
        .fetch_one(pool)
        .await?;
    Ok(row.try_get("school_salt")?)
}

fn build_nerdc_starter_packs() -> Vec<ContentPack> {
    vec![
        ContentPack {
            pack_id: "nerdc-jss1-math-t1".to_string(),
            title: "JSS1 Mathematics: Number & Numeration (Term 1)".to_string(),
            subject: "Mathematics".to_string(),
            grade_level: "JSS1".to_string(),
            curriculum_standard: "NERDC 9-Year Basic Education Curriculum / BECE".to_string(),
            version: "1.0.0".to_string(),
            description: "Official NERDC JSS1 Term 1 Mathematics covering Whole Numbers, Place Value, LCM, HCF, and Fractions.".to_string(),
            language: "en-NG".to_string(),
            modules: vec![
                CurriculumModule {
                    module_id: "mod-jss1-math-w1".to_string(),
                    title: "Whole Numbers: Counting in Millions and Billions".to_string(),
                    term: "Term 1".to_string(),
                    week: 1,
                    summary: "Read, write, and identify place values of digits in large whole numbers up to billions and trillions.".to_string(),
                    lessons: vec![
                        LessonItem {
                            lesson_id: "les-jss1-math-w1-1".to_string(),
                            title: "Place Value and Large Numbers in Everyday Nigerian Life".to_string(),
                            content_markdown: "# Whole Numbers in Millions and Billions\n\nIn Nigeria, populations, budgets, and distances are often measured in **millions** ($1,000,000$) and **billions** ($1,000,000,000$).\n\n- **One Million** = $1,000 \\times 1,000 = 1,000,000$ (6 zeros)\n- **One Billion** = $1,000 \\times 1,000,000 = 1,000,000,000$ (9 zeros)\n\n### Example\nIn the number **$4,520,810$**:\n- The digit **$4$** stands for **4 Millions** ($4,000,000$)\n- The digit **$5$** stands for **5 Hundred Thousands** ($500,000$)\n- The digit **$2$** stands for **2 Ten Thousands** ($20,000$)".to_string(),
                            audio_url: Some("/audio/jss1-math-w1-1.ogg".to_string()),
                            estimated_minutes: 8,
                            points_reward: 15,
                        },
                        LessonItem {
                            lesson_id: "les-jss1-math-w1-2".to_string(),
                            title: "Lowest Common Multiple (LCM) & Highest Common Factor (HCF)".to_string(),
                            content_markdown: "# LCM and HCF\n\n- **Factors** divide a number exactly without a remainder. For example, factors of $12$ are $1, 2, 3, 4, 6, 12$.\n- **Highest Common Factor (HCF)** is the largest factor shared by two or more numbers.\n- **Lowest Common Multiple (LCM)** is the smallest positive number that is a multiple of two or more numbers.\n\n### Example\nFind the HCF and LCM of $12$ and $18$:\n- $12 = 2^2 \\times 3$\n- $18 = 2 \\times 3^2$\n- $\\text{HCF} = 2 \\times 3 = 6$\n- $\\text{LCM} = 2^2 \\times 3^2 = 36$".to_string(),
                            audio_url: Some("/audio/jss1-math-w1-2.ogg".to_string()),
                            estimated_minutes: 10,
                            points_reward: 15,
                        },
                    ],
                    quizzes: vec![QuizItem {
                        quiz_id: "quiz-jss1-math-w1".to_string(),
                        title: "JSS1 Mathematics Week 1-2 Checkpoint Quiz".to_string(),
                        difficulty: "easy".to_string(),
                        questions: vec![
                            QuizQuestion {
                                question_id: "q-math-1".to_string(),
                                prompt: "What is the place value of 5 in the number 35,420,100?".to_string(),
                                options: vec![
                                    "5 Hundred Thousand".to_string(),
                                    "5 Million".to_string(),
                                    "5 Ten Million".to_string(),
                                    "50 Thousand".to_string(),
                                ],
                                correct_option_index: 1,
                                explanation: "In 35,420,100, the digit 5 is in the Millions position (5,000,000).".to_string(),
                            },
                            QuizQuestion {
                                question_id: "q-math-2".to_string(),
                                prompt: "Find the Highest Common Factor (HCF) of 12 and 18.".to_string(),
                                options: vec![
                                    "3".to_string(),
                                    "6".to_string(),
                                    "12".to_string(),
                                    "36".to_string(),
                                ],
                                correct_option_index: 1,
                                explanation: "Factors of 12 are {1,2,3,4,6,12} and factors of 18 are {1,2,3,6,9,18}. The highest common factor is 6.".to_string(),
                            },
                            QuizQuestion {
                                question_id: "q-math-3".to_string(),
                                prompt: "Find the Lowest Common Multiple (LCM) of 8 and 12.".to_string(),
                                options: vec![
                                    "16".to_string(),
                                    "24".to_string(),
                                    "48".to_string(),
                                    "96".to_string(),
                                ],
                                correct_option_index: 1,
                                explanation: "Multiples of 8 are 8, 16, 24, 32... Multiples of 12 are 12, 24, 36... The smallest common multiple is 24.".to_string(),
                            },
                        ],
                    }],
                },
            ],
        },
        ContentPack {
            pack_id: "nerdc-jss1-basic-sci-t1".to_string(),
            title: "JSS1 Basic Science: Family Health, Nutrition & Matter (Term 1)".to_string(),
            subject: "Basic Science".to_string(),
            grade_level: "JSS1".to_string(),
            curriculum_standard: "NERDC 9-Year Basic Education Curriculum / BECE".to_string(),
            version: "1.0.0".to_string(),
            description: "Official NERDC JSS1 Term 1 Basic Science covering Sanitation, Nutrition, Balanced Diet, and Living vs Non-Living Things.".to_string(),
            language: "en-NG".to_string(),
            modules: vec![CurriculumModule {
                module_id: "mod-jss1-sci-w1".to_string(),
                title: "Family Health: Sanitation and Nutrition".to_string(),
                term: "Term 1".to_string(),
                week: 1,
                summary: "Understand personal hygiene, environmental sanitation, and the six classes of food in a balanced Nigerian diet.".to_string(),
                lessons: vec![LessonItem {
                    lesson_id: "les-jss1-sci-w1-1".to_string(),
                    title: "Classes of Food and Balanced Diet".to_string(),
                    content_markdown: "# Nutrition and Classes of Food\n\nA **balanced diet** contains all six classes of food in the right proportions:\n1. **Carbohydrates** (Energy): Yam, cassava (garri), rice, maize.\n2. **Proteins** (Body-building & repair): Beans, fish, eggs, groundnuts, meat.\n3. **Fats and Oils** (Energy & warmth): Palm oil, groundnut oil, butter.\n4. **Vitamins** (Protection against disease): Oranges, pawpaw, ugu (fluted pumpkin leaves).\n5. **Mineral Salts** (Strong bones & teeth): Milk, iodized salt, snails.\n6. **Water** (Digestion & temperature regulation).".to_string(),
                    audio_url: Some("/audio/jss1-sci-w1-1.ogg".to_string()),
                    estimated_minutes: 7,
                    points_reward: 15,
                }],
                quizzes: vec![QuizItem {
                    quiz_id: "quiz-jss1-sci-w1".to_string(),
                    title: "JSS1 Basic Science: Sanitation & Nutrition Quiz".to_string(),
                    difficulty: "easy".to_string(),
                    questions: vec![
                        QuizQuestion {
                            question_id: "q-sci-1".to_string(),
                            prompt: "Which class of food is primarily responsible for repairing worn-out body tissues?".to_string(),
                            options: vec![
                                "Carbohydrates".to_string(),
                                "Proteins".to_string(),
                                "Fats and Oils".to_string(),
                                "Vitamins".to_string(),
                            ],
                            correct_option_index: 1,
                            explanation: "Proteins (found in beans, fish, eggs, and meat) build the body and repair worn-out tissues.".to_string(),
                        },
                        QuizQuestion {
                            question_id: "q-sci-2".to_string(),
                            prompt: "Which of the following Nigerian foods is richest in Carbohydrates?".to_string(),
                            options: vec![
                                "Pounded Yam".to_string(),
                                "Boiled Egg".to_string(),
                                "Crayfish".to_string(),
                                "Ugu Leaves".to_string(),
                            ],
                            correct_option_index: 0,
                            explanation: "Yam is a tuber rich in starch (carbohydrates), which provides energy.".to_string(),
                        },
                    ],
                }],
            }],
        },
        ContentPack {
            pack_id: "nerdc-sss3-waec-math-prep".to_string(),
            title: "SSS3 WAEC/JAMB Mathematics: Indices & Logarithms".to_string(),
            subject: "Mathematics".to_string(),
            grade_level: "SSS3".to_string(),
            curriculum_standard: "WAEC WASSCE / JAMB UTME Syllabus".to_string(),
            version: "1.0.0".to_string(),
            description: "Senior Secondary exam preparation module covering laws of indices, standard form, and logarithms with past WAEC/JAMB style questions.".to_string(),
            language: "en-NG".to_string(),
            modules: vec![CurriculumModule {
                module_id: "mod-sss3-math-w1".to_string(),
                title: "Laws of Indices and Exponential Equations".to_string(),
                term: "Exam Prep".to_string(),
                week: 1,
                summary: "Master the laws of indices and solve exponential equations frequently tested on WASSCE and UTME.".to_string(),
                lessons: vec![LessonItem {
                    lesson_id: "les-sss3-math-w1-1".to_string(),
                    title: "Core Laws of Indices".to_string(),
                    content_markdown: "# Laws of Indices\n\nFor any non-zero base $a$:\n1. $a^m \\times a^n = a^{m+n}$\n2. $a^m \\div a^n = a^{m-n}$\n3. $(a^m)^n = a^{mn}$\n4. $a^0 = 1$\n5. $a^{-n} = \\frac{1}{a^n}$".to_string(),
                    audio_url: None,
                    estimated_minutes: 12,
                    points_reward: 20,
                }],
                quizzes: vec![QuizItem {
                    quiz_id: "quiz-sss3-math-w1".to_string(),
                    title: "WAEC/JAMB Indices Practice Quiz".to_string(),
                    difficulty: "medium".to_string(),
                    questions: vec![QuizQuestion {
                        question_id: "q-waec-1".to_string(),
                        prompt: "Solve for x if 2^(x + 1) = 16.".to_string(),
                        options: vec![
                            "2".to_string(),
                            "3".to_string(),
                            "4".to_string(),
                            "5".to_string(),
                        ],
                        correct_option_index: 1,
                        explanation: "Since 16 = 2^4, we have 2^(x + 1) = 2^4, so x + 1 = 4, which gives x = 3.".to_string(),
                    }],
                }],
            }],
        },
    ]
}
