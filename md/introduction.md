1. Core Architecture (What you actually need to build)

The architecture revolves around "one device downloading, multiple devices accessing."

· The Host Device (Teacher's phone): Runs a small local server. This phone needs internet only once a week (or month) to download a "content pack" (textbooks, quizzes, videos).
· The Client Devices (Students' phones): Connect to the Host Device via Wi-Fi Hotspot or Bluetooth. Students do not need internet or mobile data. They open a browser and access the learning material. This is exactly how open-source tools like Knowledge to Go (K2Go) and Kolibri work for offline learning .
· The Sync Mechanism: When the teacher's phone connects to the internet (e.g., at a cybercafé or market), the app uploads the students' leaderboard scores and downloads the latest content pack. This is your "offline-first" sync model .

2. Curriculums (Where to get them and why you need them)

You absolutely need the official Nigerian curriculum. If your content doesn't align with what students are tested on (WAEC, JAMB, BECE), teachers and parents won't trust it.

· Where to get it: The Nigerian Educational Research and Development Council (NERDC) is the official body that develops the national curriculum for Basic and Senior Secondary Education . They have developed the 9-Year Basic Education Curriculum and Teacher Guides .
· How to get it: You don't need to wait for a formal partnership for the hackathon. You can:
  · Find the NERDC curriculum PDFs online or from local schools.
  · Align your content to WAEC/JAMB syllabi (which are publicly available) to ensure relevance for older students .
  · For the prototype, focus on one subject (e.g., JSS1 Mathematics or English) and structure it exactly according to the official topic breakdown.

3. Do you need actual teachers? (Yes, but not how you think)

You don't need to hire teachers to record video lessons (that's expensive and hard to update). You need teachers in two specific roles:

· Content Validators (Advisory): Find 1-2 local teachers in your network to review your content pack. Ask them: "Does this match your scheme of work? Are the questions at the right difficulty level?" This gives you credibility for the USPF application.
· The "Host" in the Community: In the field, the teacher (or a community volunteer) is the operator of the Host Device. They are the ones who download the content and turn on the hotspot. Your app must be extremely simple for them to use, requiring minimal digital skills.

4. How to implement the "Gamification/Leaderboard" (The competition part)

To drive the "need to read," you integrate gamification into the offline app:

· Points & Badges: Students earn points for completing offline reading modules and quizzes. They earn badges for streaks (e.g., "3 days in a row") .
· Local Leaderboards (Offline): Since students are on the local hotspot, the app can show a classroom or school leaderboard in real-time without internet. Students can see their rank among peers immediately.
· National/Regional Leaderboards (Sync): When the Host Device syncs to the internet, the school's total points are uploaded. The app then displays a state or national leaderboard showing how their school ranks against others. This creates inter-school competition.

5. Technical Details (What the judges will look for)

For the "Supporting Evidence" and "Development Progress" sections, you need to show you've thought about the tech:

· Storage Management: The app must handle limited phone storage. Use LRU (Least Recently Used) caching to automatically delete old, unplayed content to free up space .
· Low Bandwidth: Compress videos heavily, or better yet, use text + audio (which uses tiny amounts of data) for the initial version. This ensures the Host Device can download the weekly pack on a slow 2G connection.
· Data Privacy: Since you are collecting student names and scores, you must address the USPF question on "Personal Data Collection." State that you will use local encryption and only sync anonymized scores to the leaderboard.

6. A Simple Roadmap for the Hackathon

1. Week 1: Define the exact topic list for one subject (e.g., JSS1 Basic Science). Find open-source or public domain text/images to build the "content pack."
2. Week 2: Build a Progressive Web App (PWA) (easier than a native app). A PWA can work offline and be installed on any phone browser. Implement the Local Host feature (using a library like NanoHTTPD or a similar local server).
3. Week 3: Build the Quiz Engine and the Leaderboard UI. Test the "Host -> Student" connection on two phones in the same room (one on hotspot, one on Wi-Fi).
4. Week 4: Create a demo video showing a student taking a quiz offline and the score appearing on a leaderboard, then show the teacher syncing to the internet to update the national leaderboard.
5.
