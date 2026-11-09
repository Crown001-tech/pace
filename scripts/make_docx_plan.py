"""
PACE Master Implementation Plan DOCX Generator
Generates a polished, professional Microsoft Word document for the implementation plan.
"""

import os
from docx import Document
from docx.shared import Inches, Pt, RGBColor
from docx.enum.text import WD_ALIGN_PARAGRAPH
from docx.enum.table import WD_TABLE_ALIGNMENT, WD_ALIGN_VERTICAL
from docx.oxml import OxmlElement
from docx.oxml.ns import qn

def set_cell_background(cell, hex_color):
    """Set the background color of a table cell."""
    tc_pr = cell._tc.get_or_add_tcPr()
    shd = OxmlElement('w:shd')
    shd.set(qn('w:val'), 'clear')
    shd.set(qn('w:color'), 'auto')
    shd.set(qn('w:fill'), hex_color)
    tc_pr.append(shd)

def set_cell_margins(cell, top=100, bottom=100, left=150, right=150):
    """Set padding inside a table cell (in dxa: 20 dxa = 1 pt)."""
    tc_pr = cell._tc.get_or_add_tcPr()
    tc_mar = OxmlElement('w:tcMar')
    for m, val in [('w:top', top), ('w:bottom', bottom), ('w:left', left), ('w:right', right)]:
        node = OxmlElement(m)
        node.set(qn('w:w'), str(val))
        node.set(qn('w:type'), 'dxa')
        tc_mar.append(node)
    tc_pr.append(tc_mar)

def add_callout(doc, title, text, bg_color="F1F5F9", border_color="0284C7"):
    """Add a professional styled callout box (table with left accent border)."""
    tbl = doc.add_table(rows=1, cols=1)
    tbl.alignment = WD_TABLE_ALIGNMENT.CENTER
    cell = tbl.cell(0, 0)
    set_cell_background(cell, bg_color)
    set_cell_margins(cell, top=140, bottom=140, left=200, right=200)
    
    # Left border
    tc_pr = cell._tc.get_or_add_tcPr()
    tc_borders = OxmlElement('w:tcBorders')
    
    left_b = OxmlElement('w:left')
    left_b.set(qn('w:val'), 'single')
    left_b.set(qn('w:sz'), '24') # 3pt
    left_b.set(qn('w:space'), '0')
    left_b.set(qn('w:color'), border_color)
    tc_borders.append(left_b)
    
    for side in ['top', 'bottom', 'right']:
        b = OxmlElement(f'w:{side}')
        b.set(qn('w:val'), 'none')
        tc_borders.append(b)
    tc_pr.append(tc_borders)
    
    p = cell.paragraphs[0]
    p.paragraph_format.space_before = Pt(0)
    p.paragraph_format.space_after = Pt(4)
    run_title = p.add_run(title)
    run_title.bold = True
    run_title.font.name = 'Segoe UI'
    run_title.font.size = Pt(10.5)
    run_title.font.color.rgb = RGBColor(15, 23, 42)
    
    p2 = cell.add_paragraph()
    p2.paragraph_format.space_before = Pt(0)
    p2.paragraph_format.space_after = Pt(0)
    run_text = p2.add_run(text)
    run_text.font.name = 'Segoe UI'
    run_text.font.size = Pt(10)
    run_text.font.color.rgb = RGBColor(51, 65, 85)
    
    doc.add_paragraph().paragraph_format.space_after = Pt(4)

def format_table(tbl, col_widths, header_bg="1E293B", alt_bg="F8FAFC"):
    """Format a table with header color, alternating row colors, and proper borders."""
    tbl.alignment = WD_TABLE_ALIGNMENT.CENTER
    tbl.autofit = False
    
    for r_idx, row in enumerate(tbl.rows):
        is_header = (r_idx == 0)
        bg = header_bg if is_header else (alt_bg if r_idx % 2 == 1 else "FFFFFF")
        for c_idx, cell in enumerate(row.cells):
            cell.width = Inches(col_widths[c_idx])
            set_cell_background(cell, bg)
            set_cell_margins(cell, top=100, bottom=100, left=140, right=140)
            cell.vertical_alignment = WD_ALIGN_VERTICAL.CENTER
            
            # Subtle borders
            tc_pr = cell._tc.get_or_add_tcPr()
            tc_borders = OxmlElement('w:tcBorders')
            for side in ['top', 'bottom', 'left', 'right']:
                b = OxmlElement(f'w:{side}')
                b.set(qn('w:val'), 'single')
                b.set(qn('w:sz'), '4') # 0.5pt
                b.set(qn('w:color'), 'CBD5E1' if not is_header else '0F172A')
                tc_borders.append(b)
            tc_pr.append(tc_borders)
            
            for p in cell.paragraphs:
                p.paragraph_format.space_before = Pt(0)
                p.paragraph_format.space_after = Pt(0)
                for run in p.runs:
                    run.font.name = 'Segoe UI'
                    if is_header:
                        run.bold = True
                        run.font.color.rgb = RGBColor(255, 255, 255)
                        run.font.size = Pt(9.5)
                    else:
                        run.font.color.rgb = RGBColor(30, 41, 59)
                        run.font.size = Pt(9.5)

def build_docx(output_path):
    doc = Document()
    
    # Page setup: Standard margins (0.75 in)
    sections = doc.sections
    for section in sections:
        section.top_margin = Inches(0.75)
        section.bottom_margin = Inches(0.75)
        section.left_margin = Inches(0.75)
        section.right_margin = Inches(0.75)
        
    # Title
    p_title = doc.add_paragraph()
    p_title.paragraph_format.space_before = Pt(0)
    p_title.paragraph_format.space_after = Pt(2)
    run_super = p_title.add_run("PACE\n")
    run_super.bold = True
    run_super.font.name = 'Segoe UI'
    run_super.font.size = Pt(26)
    run_super.font.color.rgb = RGBColor(15, 23, 42)
    
    run_sub = p_title.add_run("Master Implementation Plan\n")
    run_sub.bold = True
    run_sub.font.name = 'Segoe UI'
    run_sub.font.size = Pt(18)
    run_sub.font.color.rgb = RGBColor(37, 99, 235)
    
    run_desc = p_title.add_run("The complete execution roadmap, learning tracks, and verification gates (M0 to M13)")
    run_desc.italic = True
    run_desc.font.name = 'Segoe UI'
    run_desc.font.size = Pt(11)
    run_desc.font.color.rgb = RGBColor(100, 116, 139)
    
    # Metadata Table
    meta_table = doc.add_table(rows=6, cols=2)
    meta_data = [
        ("Product", "PACE — Automated narrated video production from scripts, images & audio"),
        ("Document", "Master Implementation Plan & Milestones Guide"),
        ("Version", "1.0 (Comprehensive Build Roadmap)"),
        ("Owner", "Peter Oluwafemi Ajao — HeteroGenius"),
        ("Status", "Approved for implementation; changes via ADRs"),
        ("Companion Documents", "PACE_PRD.docx (WHAT) · PACE_Technical_Design_Document.docx (HOW) · PACE_File_Structure.docx (WHERE) · PACE_Milestones.docx (ORDER)")
    ]
    for idx, (f, v) in enumerate(meta_data):
        meta_table.cell(idx, 0).paragraphs[0].add_run(f).bold = True
        meta_table.cell(idx, 1).paragraphs[0].add_run(v)
    format_table(meta_table, [2.0, 5.0], header_bg="0F172A", alt_bg="F8FAFC")
    doc.add_paragraph().paragraph_format.space_after = Pt(12)
    
    # Section 1: Executive Summary & Hierarchy
    h1 = doc.add_heading("1. Executive Summary & Source of Truth", level=1)
    h1.paragraph_format.space_before = Pt(12)
    h1.paragraph_format.space_after = Pt(6)
    
    add_callout(doc, "Conflict Resolution Hierarchy (Binding)", 
                "If specifications conflict across documents: PRD wins on WHAT to build · TDD wins on HOW it is built · File Structure wins on WHERE code lives · Milestones wins on the ORDER we build and learn in.")
    
    p = doc.add_paragraph()
    p.add_run("PACE transforms a raw text script, a folder of still images, and a single voice narration recording into a complete, frame-synchronized MP4 video (20–40 minutes long, in 16:9 landscape or 9:16 portrait). It splits the script into paragraph sections, associates one image per section, transcribes narration using Groq Whisper, aligns words to calculate exact scene change boundaries snapped to speech pauses, and draws every frame on CPU with camera motion and transitions.\n\n"
              "This document translates the four source-of-truth documents into a practical, step-by-step paired engineering roadmap. Every milestone includes learning items, step-by-step build tasks, failure-first tests, and explicit exit criteria.")
    
    # Section 2: Fixed Decisions
    h1 = doc.add_heading("2. Fixed Architectural Decisions & Hardware Constraints", level=1)
    h1.paragraph_format.space_before = Pt(14)
    h1.paragraph_format.space_after = Pt(6)
    
    dec_table = doc.add_table(rows=10, cols=3)
    dec_table.cell(0, 0).paragraphs[0].add_run("ID")
    dec_table.cell(0, 1).paragraphs[0].add_run("Decision")
    dec_table.cell(0, 2).paragraphs[0].add_run("Why in Plain Words")
    
    decisions = [
        ("D1", "Dual-Compiled Rust Engine", "pace-engine is pure mathematics (no I/O); compiles natively for server export and to WebAssembly for browser preview, guaranteeing frame parity."),
        ("D2", "Rust Draws Every Frame", "Rust rasterizes frames on CPU with fast_image_resize SIMD; FFmpeg only encodes frames and mixes/ducks audio for silky smooth sub-pixel zooms."),
        ("D3", "Python Audio Worker", "Python service dedicated strictly to audio prep, Opus transcode, Groq Whisper ASR, and Needleman-Wunsch script alignment."),
        ("D4", "PostgreSQL Dual Role", "Postgres 16 stores structured data AND acts as the job queue via FOR UPDATE SKIP LOCKED; zero extra queue services needed."),
        ("D5", "Object Storage with Pre-signed URLs", "Heavy files upload directly from browser to storage (MinIO locally, Backblaze B2 online), never touching the API."),
        ("D6", "Static Export Web App", "Next.js + TypeScript built as output: 'export' for free, instant hosting on Cloudflare Pages."),
        ("D7", "Single Contract Source", "Data shapes defined once in Rust; cargo xtask gen produces TypeScript types and Python Pydantic models automatically."),
        ("D8", "Hardware Memory Boundary", "Laptop: Core i7-8550U, 8 GB RAM. WSL2 capped at 5 GB RAM and 4 GB swap via .wslconfig; Rust builds use mold and jobs=4."),
        ("D9", "Local-First & $0 Cost", "Everything runs locally in Docker at $0 until Milestone 11 (Go Online). No cloud fees or credit cards needed to build.")
    ]
    for idx, (d_id, dec, why) in enumerate(decisions, start=1):
        dec_table.cell(idx, 0).paragraphs[0].add_run(d_id)
        dec_table.cell(idx, 1).paragraphs[0].add_run(dec)
        dec_table.cell(idx, 2).paragraphs[0].add_run(why)
    format_table(dec_table, [0.6, 2.8, 3.6], header_bg="1E293B")
    doc.add_paragraph().paragraph_format.space_after = Pt(12)
    
    # Section 3: Roadmap Matrix
    h1 = doc.add_heading("3. The Roadmap at a Glance", level=1)
    h1.paragraph_format.space_before = Pt(14)
    h1.paragraph_format.space_after = Pt(6)
    
    add_callout(doc, "The Critical Technical Path",
                "The core engineering spine is M1 (Engine) -> M5 (Audio Alignment) -> M7 (Browser Preview) -> M8 (Render & Export). Every other milestone bends around these four foundational components.",
                border_color="E11D48", bg_color="FFF1F2")
    
    road_table = doc.add_table(rows=15, cols=4)
    road_table.cell(0, 0).paragraphs[0].add_run("Milestone")
    road_table.cell(0, 1).paragraphs[0].add_run("Name")
    road_table.cell(0, 2).paragraphs[0].add_run("Goal / What Works at End")
    road_table.cell(0, 3).paragraphs[0].add_run("Cost")
    
    milestone_summary = [
        ("M0", "Setup & First Experiments", "Docker runs Postgres & MinIO; Groq ASR & direct uploads proven", "$0"),
        ("M1", "The Engine Core", "Pure Rust engine calculates timing, motion, fit; render speed measured", "$0"),
        ("M2", "Accounts & Projects API", "axum API with auth, multi-tenant projects, and error handling", "$0"),
        ("M3", "Web App: Script & Sections", "Next.js dashboard & script editor with autosave and undo/redo", "$0"),
        ("M4", "Media Upload & Assignment", "Hundreds of images upload directly to MinIO and auto-assign to sections", "$0"),
        ("M5", "Narration & Sync", "Python worker transcribes speech and aligns words to script with flags", "$0"),
        ("M6", "Timeline Editor", "Visual waveform timeline with draggable boundaries and pause snapping", "$0"),
        ("M7", "Effects & Browser Preview", "Camera motions & full in-browser preview via Rust WebAssembly", "$0"),
        ("M8", "Render & Export", "Rust render worker draws frames, encodes with FFmpeg, verifies MP4", "$0"),
        ("M9", "Music & Subtitles", "Ducked background music; editable subtitles burned into video", "$0"),
        ("M10", "Hardening & Local Matrix", "Full release test matrix passed locally in Docker with zero data loss", "$0"),
        ("M11", "Go Online", "PACE deployed on Railway, Backblaze B2, and Cloudflare Pages (<$8/mo)", "~$5–8/mo"),
        ("M12", "Chrome Extension", "One-click image and prompt import from Google Flow UI", "$0"),
        ("M13", "Private Beta", "3 invited creators each produce real 20-40 min videos without help", "~$5–8/mo")
    ]
    for idx, (m_id, name, goal, cost) in enumerate(milestone_summary, start=1):
        road_table.cell(idx, 0).paragraphs[0].add_run(m_id)
        road_table.cell(idx, 1).paragraphs[0].add_run(name)
        road_table.cell(idx, 2).paragraphs[0].add_run(goal)
        road_table.cell(idx, 3).paragraphs[0].add_run(cost)
    format_table(road_table, [0.8, 1.8, 3.8, 0.6], header_bg="0F172A")
    doc.add_paragraph().paragraph_format.space_after = Pt(12)
    
    # Milestone Details Helper
    milestones_data = [
        {
            "id": "M0", "title": "Setup and First Experiments",
            "scope": "Foundations for all milestones; TDD Sections 2–4",
            "cost": "$0", "status": "IN PROGRESS (Toolchain & Docker verified)",
            "plain": "In plain words: We set up the laptop so Docker runs the database and storage in small containers, configure the toolchains inside native Ubuntu, and prove that external services (Groq transcription and direct uploads) work before writing application code.",
            "learn": [
                ("Docker Fundamentals", "Image vs Container vs Volume vs Port mapping; Docker Compose orchestration.", "Docker Get Started Guide"),
                ("Git & WSL2", "Git CLI branching, commits, and Linux filesystem permissions.", "Pro Git Book (Ch 1-3)"),
                ("Memory Management", "WSL2 resource boundaries (.wslconfig) on 8 GB laptop.", "TDD Section 4.1")
            ],
            "tasks": [
                ("M0-1", "Configure .wslconfig (5 GB RAM, 4 GB swap) and install Docker Desktop with WSL integration.", "Stable Windows/WSL host"),
                ("M0-2", "Install Ubuntu packages (clang, mold, ffmpeg) and toolchains (Rust, Node fnm, Python uv).", "Verified development toolchain"),
                ("M0-3", "Scaffold workspace layout per PACE_File_Structure.docx inside ~/code/pace.", "Clean monorepo structure"),
                ("M0-4", "Create infra/docker/compose.yml with PostgreSQL 16 and MinIO.", "make up operational"),
                ("M0-5", "Write root Cargo.toml, .cargo/config.toml (mold linker), and crates/pace-engine.", "Passing sanity test"),
                ("M0-6", "Experiment S2: Python script using uv testing Groq Whisper API vs local faster-whisper.", "Timing and accuracy comparison"),
                ("M0-7", "Experiment S3: Browser upload directly to MinIO via pre-signed URL with simulated disconnect.", "Direct upload verification"),
                ("M0-8", "Record learnings in learning/m00-docker-and-setup.md.", "Milestone 0 notes")
            ],
            "tests": ["Trivial cargo test (2 + 2 = 4 and engine_version()) passes in pace-engine."],
            "done": [
                "docker run hello-world works; make up starts PostgreSQL and MinIO.",
                "First Git commit created and CI runs cargo test.",
                "Groq returns word timings for audio test clip; faster-whisper benchmarked.",
                "Browser upload to MinIO via pre-signed URL works and survives network interruption."
            ],
            "fallback": "If Docker Desktop uses too much RAM on 8 GB, PostgreSQL and MinIO can be run as native services inside Ubuntu."
        },
        {
            "id": "M1", "title": "The Engine Core (First Real Rust)",
            "scope": "SCR-02, TML-05..07, EFX-01..02 (logic); TDD Sections 12.1–12.4",
            "cost": "$0", "status": "UP NEXT",
            "plain": "In plain words: The engine is the brain of PACE. It has zero file, network, or database code. Given a script and boundary times, it works out exact frame numbers, camera movements, and crop rectangles. It compiles natively and to WebAssembly.",
            "learn": [
                ("Ownership & Borrowing", "Rust's memory safety rules, stack vs heap, references, lifetimes.", "Rust Book Ch 4"),
                ("Structs & Enums", "Custom data types, algebraic data types, pattern matching with match.", "Rust Book Ch 5-6"),
                ("Error Handling", "Result<T, E>, Option<T>, unwrap vs ?, custom error types.", "Rust Book Ch 9"),
                ("Property Testing", "Using proptest to test hundreds of randomized inputs automatically.", "proptest documentation")
            ],
            "tasks": [
                ("M1-1", "Implement segmentation.rs: split script text into sections at blank lines.", "Section list generator"),
                ("M1-2", "Implement timing.rs: convert boundary times (ms) to exact frame numbers at 30 fps without drift.", "Frame conversion model"),
                ("M1-3", "Implement timing rules: minimum scene length (0.5s), boundary 0 at 0ms, last boundary at narration end.", "Validation rules"),
                ("M1-4", "Implement motion.rs: CamState, motion presets (zoom_in, pan_left, etc.), easing curves.", "Camera motion engine"),
                ("M1-5", "Implement effects.rs: seeded automatic variety generator (rand_pcg) respecting diversity rules.", "Predictable effect variety"),
                ("M1-6", "Implement fit.rs: calculate source crop rectangle from image dimensions, frame size, focal point, and zoom.", "Geometry crop calculations"),
                ("M1-7", "Spike S1: CLI program in learning/exercises/render-spike to measure 60s CPU render speed at 720p/1080p.", "Render speed benchmark")
            ],
            "tests": [
                "Text splitting handles Windows line endings, space-only lines, 60,000 char scripts.",
                "Property test: 300 random boundaries over 40 mins sum exactly to total frames; no drift.",
                "Moving boundary k changes only scenes k-1 and k.",
                "Variety rules: no preset >2 consecutive scenes; no preset >35% total.",
                "Fit geometry: crop rectangle never breaches image bounds for any focal point/zoom."
            ],
            "done": [
                "All pace-engine unit and property tests pass in CI.",
                "Spike S1 renders a smooth 60s MP4; FPS recorded for 720p and 1080p.",
                "Estimated render time for 22-min 720p video documented in docs/adr/ADR-003."
            ],
            "fallback": "If 1080p CPU rendering is very slow on the laptop, keep 720p as default; online worker has more CPUs."
        },
        {
            "id": "M2", "title": "Accounts and Projects API",
            "scope": "AUTH-01..06, PRJ-01..06, ADM-01 (basic); TDD Section 7",
            "cost": "$0", "status": "PLANNED",
            "plain": "In plain words: The API is the secure front desk. It checks who you are, creates projects, stores sections, and guarantees that no user can ever see or modify another user's projects.",
            "learn": [
                ("Async Rust & Tokio", "Tasks, async/await, channels, tokio runtime fundamentals.", "Tokio Tutorial"),
                ("Axum Web Framework", "Routing, extractors (State, Json, Extension), tower middleware.", "Axum GitHub Examples"),
                ("SQLx Database Toolkit", "Connection pools, migrations (sqlx migrate), compile-time verified queries.", "SQLx README"),
                ("Security & Sessions", "Argon2id password hashing, secure httpOnly session cookies, CSRF tokens.", "TDD Section 17")
            ],
            "tasks": [
                ("M2-1", "Create pace-db with migration 0001_initial.sql (users, sessions, login_attempts, projects, audit_log).", "Database schema"),
                ("M2-2", "Create pace-api with axum router, middleware (tracing, CORS, request IDs), and /healthz.", "API skeleton"),
                ("M2-3", "Implement authentication: Argon2id hashing, session tokens in DB, lockout after 10 failures.", "Session auth system"),
                ("M2-4", "Create scripts/create-admin.sh and admin user management routes.", "Admin capabilities"),
                ("M2-5", "Implement project CRUD routes with aspect ratio (16:9/9:16) and If-Match version checking.", "Project API"),
                ("M2-6", "Define ApiError enum producing RFC 9457 Problem Details JSON.", "Standardized error handling"),
                ("M2-7", "Add utoipa annotations; configure cargo xtask gen to export contracts/openapi.json and TypeScript types.", "Automated contract generation")
            ],
            "tests": [
                "Login succeeds with correct credentials, fails with incorrect, locks after 10 failed attempts.",
                "Logout, password change, and deactivation terminate active sessions.",
                "Multi-tenancy isolation: all project endpoints return 404 when accessed by another user.",
                "Stale If-Match version returns 409 PROJECT_VERSION_CONFLICT."
            ],
            "done": [
                "Full user and project lifecycle functional via curl/Bruno against local PostgreSQL.",
                "All API integration tests pass in CI."
            ],
            "fallback": "Keep admin features plain; start with sqlx::query_as if compile-time macros feel overwhelming initially."
        },
        {
            "id": "M3", "title": "Web App: Dashboard, Script, and Sections",
            "scope": "SCR-01..07, SCR-09, PRJ-04, PRJ-07; Screens: Sign in, Dashboard, Step 1; TDD Section 15",
            "cost": "$0", "status": "PLANNED",
            "plain": "In plain words: The creator opens their browser, logs in, creates a video project, pastes their script, and edits paragraphs into sections with instant autosave and undo/redo.",
            "learn": [
                ("Next.js Static Export", "App Router with output: 'export', query params (?id=...), client-side fetching.", "Next.js Documentation"),
                ("TanStack Query", "Server cache management, mutations, optimistic updates, query invalidation.", "TanStack Query Docs"),
                ("Zustand & Zundo", "Client-side editor state management with automatic undo/redo history.", "Zustand GitHub"),
                ("Tailwind & shadcn/ui", "Utility-first CSS and accessible headless component primitives.", "shadcn/ui Docs")
            ],
            "tasks": [
                ("M3-1", "Scaffold apps/web with Next.js, TypeScript, Tailwind CSS, and shadcn/ui.", "Web app foundation"),
                ("M3-2", "Build sign-in screen, forced password change dialog, and persistent session provider.", "Authentication UI"),
                ("M3-3", "Build Dashboard: project list, New Project modal (name, 16:9 vs 9:16), storage indicator.", "Dashboard UI"),
                ("M3-4", "Add API sections routes (scripts, sections tables; split, merge, reorder).", "Sections API"),
                ("M3-5", "Build Step 1 UI: script editor with blank-line splitting, word count, 150 WPM duration estimates.", "Script editor"),
                ("M3-6", "Implement interactive section editing: split at cursor, merge with next, drag reorder (dnd-kit).", "Section management"),
                ("M3-7", "Build editor store with Zustand, zundo (50+ undo steps), and autosave indicator (Saving/Saved/Error).", "Autosave & history engine")
            ],
            "tests": [
                "Text integrity: concatenating all section texts exactly equals original script.",
                "Undo/redo: 50 mixed user actions return reliably to earlier states.",
                "Autosave: closing tab 2s after edit preserves changes (Playwright)."
            ],
            "done": [
                "40-paragraph script splits into 40 sections; reordering survives reload.",
                "Two open tabs editing the same project cannot overwrite each other silently."
            ],
            "fallback": "Script import from .txt/.docx (SCR-08) can wait until M10; keep styling minimal."
        },
        {
            "id": "M4", "title": "Media Upload and Assignment",
            "scope": "MED-01..08, ASN-01..05, STO-01, AUTH-06; TDD Section 9",
            "cost": "$0", "status": "PLANNED",
            "plain": "In plain words: The creator uploads hundreds of images. Files fly directly from the browser into storage using pre-signed links. A Web Worker creates thumbnails and checks for duplicates, and PACE assigns images to sections in order.",
            "learn": [
                ("Pre-Signed S3 URLs", "Delegating secure PUT/GET upload permissions without proxying data through API.", "AWS S3 / MinIO Docs"),
                ("Multipart Uploads", "Chunking large files into 8 MB parts with resume capabilities.", "S3 Multipart Docs"),
                ("Web Workers & SubtleCrypto", "Off-main-thread image hashing (SHA-256) and canvas resizing.", "MDN Web Workers")
            ],
            "tasks": [
                ("M4-1", "Create pace-storage: Storage trait, S3 implementation (rusty-s3 + reqwest), key formats.", "Storage abstraction"),
                ("M4-2", "Add minio-init container to Docker Compose to auto-create 'pace' bucket and CORS rules.", "Automated bucket init"),
                ("M4-3", "API upload routes: POST /projects/{id}/uploads (presigned PUT or multipart), verify sizes.", "Upload signing API"),
                ("M4-4", "Client upload manager: Web Worker generates thumbnail (320px WebP), preview (1280px), and SHA-256.", "Fast client prep"),
                ("M4-5", "Build Step 2 UI: dropzone, gallery with natural sorting (img2 before img10), duplicate warnings.", "Media library UI"),
                ("M4-6", "Build Assignment UI: auto-assign (k -> k), drag-assign, section swap, mismatch banner, export block.", "Assignment workflow")
            ],
            "tests": [
                "Natural sorting handles mixed case, leading zeros, and numerical sequences.",
                "Uploads over user storage quota are refused before signing; usage updates on delete.",
                "Deduplication detects identical file contents.",
                "Playwright test: 150 images upload with simulated network disconnect, resuming cleanly."
            ],
            "done": [
                "150 images (~300 MB) upload reliably and auto-assign to sections.",
                "Thumbnails appear within 2 seconds of each upload finishing."
            ],
            "fallback": "Keep simple retries for images; use multipart uploads strictly for narration audio."
        },
        {
            "id": "M5", "title": "Narration and Sync (Python Audio Worker)",
            "scope": "NAR-01..06, ALN-01..07, JOB-01..03; TDD Sections 8, 10, 11",
            "cost": "$0", "status": "PLANNED",
            "plain": "In plain words: The creator uploads voice narration. A Python worker compresses it, finds natural speech pauses, transcribes it via Groq Whisper, and matches spoken words to the script to set scene change times.",
            "learn": [
                ("Python uv Tooling", "Managing Python 3.12 dependencies with uv lock and pyproject.toml.", "Astral uv Docs"),
                ("Postgres Job Queues", "FOR UPDATE SKIP LOCKED, heartbeats, retry backoff, and poison pills.", "TDD Section 8"),
                ("FFmpeg Audio Filters", "ebur128 loudness measurement, silencedetect for pause finding, Opus encoding.", "FFmpeg Documentation"),
                ("Sequence Alignment", "Needleman-Wunsch algorithm, 3-word fixed anchors, and phonetic matching.", "TDD Section 11")
            ],
            "tasks": [
                ("M5-1", "Create pace-jobs: queue polling logic, jobs table migrations, payload JSON schemas.", "Job queue core"),
                ("M5-2", "Create services/audio: daemon process, queue worker loop, Starlette /wake server.", "Audio worker daemon"),
                ("M5-3", "Implement audio.prepare: ffprobe checks, ebur128, silencedetect, peaks.json waveform, Opus transcode.", "Audio inspection & prep"),
                ("M5-4", "Implement transcribe: Groq API with 10-min chunks and 2s overlap; fallback to local faster-whisper.", "Speech-to-text pipeline"),
                ("M5-5", "Implement align: text normalization (num2words), fixed anchors, Needleman-Wunsch, pause snapping, flags.", "Script alignment engine"),
                ("M5-6", "Generate synthetic test narrations with Piper TTS; record real-world benchmark narrations.", "Test fixtures"),
                ("M5-7", "Build Step 3 UI: narration upload, waveform, SSE progress stream, flag list with Jump-to links.", "Sync UI")
            ],
            "tests": [
                "Alignment unit tests: exact speech, skipped words, retakes/repeated lines, unspoken sections.",
                "Monotonic boundaries: boundary times strictly increase; first is 0ms; last is narration end.",
                "Queue resilience: two workers never claim the same job; failed worker job recovered after 120s.",
                "Groq fallback: stubbed HTTP 429 automatically fails over to local faster-whisper."
            ],
            "done": [
                "Alignment accuracy benchmark: >=90% scene changes within +-0.75s; >=80% errors >2s flagged.",
                "40-min narration transcribed in <10 mins (Groq) and aligned in <30 seconds.",
                "Killing audio worker mid-job and restarting finishes the job automatically."
            ],
            "fallback": "Do not lower accuracy target (it is the heart of PACE). Defer Chrome extension (M12) to make time if needed."
        },
        {
            "id": "M6", "title": "Timeline Editor",
            "scope": "TML-01..07; TDD Section 15",
            "cost": "$0", "status": "PLANNED",
            "plain": "In plain words: The creator fine-tunes timing on a visual waveform. Dragging a boundary line shifts time between two scenes without affecting any other scene in the video.",
            "learn": [
                ("WaveSurfer.js v7", "Waveform rendering with Regions plugin fed by pre-computed peaks.json.", "WaveSurfer Docs"),
                ("Optimistic UI Updates", "Rendering drag updates instantly while syncing asynchronously with API.", "TanStack Query Guides")
            ],
            "tasks": [
                ("M6-1", "Add API boundaries routes (GET /projects/{id}/timeline, PATCH /projects/{id}/boundaries).", "Timeline API"),
                ("M6-2", "Build Step 4 Timeline: waveform canvas, scene thumbnail blocks, zoom slider, scroll sync.", "Interactive timeline"),
                ("M6-3", "Implement draggable boundary lines with frame snapping, Alt-snap to pauses, and 0.5s minimum scene length.", "Boundary interaction"),
                ("M6-4", "Add exact time modal (mm:ss.mmm), Play from here, and scene loop mode.", "Playback controls")
            ],
            "tests": [
                "Dragging boundary k updates scene k-1 and k while leaving all other scenes identical.",
                "Order and 0.5s minimum scene duration enforced in client and API."
            ],
            "done": [
                "40-minute project with 250 scenes scrubs smoothly at 60 fps.",
                "Every flagged boundary can be reached directly from flag drawer and corrected."
            ],
            "fallback": "Phones receive read-only summary; full editing requires desktop/laptop width (>=1024px)."
        },
        {
            "id": "M7", "title": "Effects and In-Browser Preview",
            "scope": "EFX-01..06, EFX-08..09, PRV-01, PRV-03..05; TDD Sections 12, 14",
            "cost": "$0", "status": "PLANNED",
            "plain": "In plain words: PACE automatically applies varied camera motions. In the browser, you hit Play: the narration plays, and Rust (compiled to WebAssembly) calculates frame draw plans rendered onto a canvas at 30 fps in real-time.",
            "learn": [
                ("Rust to WebAssembly", "wasm-bindgen, wasm-pack, JavaScript interoperability, memory management.", "wasm-bindgen Guide"),
                ("Typed Arrays & Performance", "Passing flat numeric arrays between WASM and JS to avoid JSON serialization.", "MDN TypedArrays"),
                ("HTML5 Canvas 2D", "drawImage with sub-pixel source cropping, globalAlpha blending, createImageBitmap.", "HTML5 Canvas Specs")
            ],
            "tasks": [
                ("M7-1", "Implement RenderSpec, compile_spec, spec hashing, and frame_plan in pace-engine; export via ts-rs.", "Engine spec & plan"),
                ("M7-2", "Create pace-engine-wasm wrapper; configure cargo xtask wasm outputting packages/engine-wasm.", "WASM engine package"),
                ("M7-3", "API render-spec endpoint: compiles frozen spec and attaches signed preview image links.", "Render spec API"),
                ("M7-4", "Build PreviewPlayer component: HTML5 Audio clock, image prefetch cache, Canvas 2D renderer.", "Real-time player"),
                ("M7-5", "Build Scene properties panel: preset picker, strength, easing, fit mode, focal point clicker, Re-roll.", "Visual effects UI")
            ],
            "tests": [
                "FramePlan validation: transition layer opacities sum to 1.0; fade-to-black calculates correct alpha.",
                "Spec hash is deterministic: identical project settings produce identical SHA-256.",
                "WASM parity test: WebAssembly and native builds output bit-identical FramePlans for 100 sample frames."
            ],
            "done": [
                "22-minute project previews smoothly from any timestamp within 3s.",
                "Changing an effect updates the preview canvas instantly and persists across reload."
            ],
            "fallback": "Camera shake (EFX-07) can defer to v1.1. If WASM setup issues arise, get total_frames working first."
        },
        {
            "id": "M8", "title": "Render and Export",
            "scope": "EXP-01..11, PRV-02, STO-02, STO-04, JOB-04; TDD Section 13",
            "cost": "$0", "status": "PLANNED",
            "plain": "In plain words: The render worker takes the frozen spec, asks the engine what every frame looks like, draws frames on CPU, feeds them to FFmpeg, and caches scene segments. Small edits re-render only the affected scenes.",
            "learn": [
                ("Piping to FFmpeg", "Spawning child processes with tokio::process::Command and streaming raw RGBA bytes into stdin.", "Tokio Process Docs"),
                ("Parallel SIMD Resizing", "rayon threadpool + fast_image_resize for high-speed CPU image scaling.", "fast_image_resize Docs"),
                ("Segment Concatenation", "FFmpeg concat demuxer with stream copy (-c copy) for instantaneous joining.", "FFmpeg Concat Demuxer")
            ],
            "tasks": [
                ("M8-1", "Create pace-render-worker daemon claiming jobs from queue = 'render'.", "Render worker daemon"),
                ("M8-2", "Implement segment planner: divide render spec into body and transition segments.", "Segment planning"),
                ("M8-3", "Implement rasterizer (raster/cpu.rs): execute FramePlan using fast_image_resize and box-blur.", "CPU frame drawing"),
                ("M8-4", "Implement segment cache: key by hash of image, motion, fit, and frames to avoid duplicate work.", "Scene caching"),
                ("M8-5", "Implement FFmpeg encoder, concat demuxer, and audio muxing pipeline.", "Video encoding engine"),
                ("M8-6", "Implement export validator: probe black frames, silence, stream specs, and narration duration parity.", "Export validation"),
                ("M8-7", "Build Step 6 UI: Export modal (resolutions, estimated time/size, 1080p limit), SSE progress, downloads.", "Export UI"),
                ("M8-8", "Add automated parity test: compare preview canvas frames against rendered MP4 frames (SSIM >= 0.95).", "Frame parity test")
            ],
            "tests": [
                "Concat integrity: joining synthetic segments matches expected frame count and timescale.",
                "Cache invalidation: editing scene 7 re-renders exactly 3 segments (body 7 and two transitions).",
                "Validator catches: truncated files, missing audio, black gaps, duration mismatch.",
                "Interruption test: stopping worker mid-export and clicking Retry resumes from cached segments."
            ],
            "done": [
                "10-min 720p export completes locally and passes all validation checks.",
                "All 8 size and orientation combinations export and play on VLC, Chrome, iOS, and Android.",
                "Re-exporting after a single-scene edit takes <3 minutes for a 22-minute video."
            ],
            "fallback": "Test with 5-10 min clips locally on laptop; full 40-min runs verified online in M11."
        },
        {
            "id": "M9", "title": "Background Music and Subtitles",
            "scope": "MUS-01..06, SUB-01..05; TDD Sections 11.5, 13.2, 13.5",
            "cost": "$0", "status": "PLANNED",
            "plain": "In plain words: Music plays softly under narration and automatically dips when the narrator speaks. Subtitles are generated from the script, can be edited and styled, and are burned directly into the video.",
            "learn": [
                ("FFmpeg Audio Filter Graphs", "loudnorm, afade, sidechaincompress (ducking), amix, and peak limiting.", "FFmpeg Audio Filters"),
                ("Text Rasterization in Rust", "Rendering glyphs, outlines, and backgrounds with ab_glyph matching browser fonts.", "ab_glyph Docs")
            ],
            "tasks": [
                ("M9-1", "Implement music upload, volume slider (-40 dB to 0 dB), crossfading loop, and fade in/out settings.", "Music management"),
                ("M9-2", "Build Step 5 UI: Subtitle editor (edit text, adjust timings, split/merge) and style panel with safe-area guides.", "Subtitle editor UI"),
                ("M9-3", "Render worker audio mix pipeline: loudnorm (-16 LUFS narration), sidechain ducking, master -14 LUFS mix.", "Audio mix pipeline"),
                ("M9-4", "Render worker subtitle burn-in: pre-render subtitle images via ab_glyph and composite onto video frames.", "Subtitle rasterization")
            ],
            "tests": [
                "Subtitle rules: <=2 lines, <=42 chars/line (32 for 9:16), durations 1-7s.",
                "Mix verification: final mix -14 +-1 LUFS, peak <= -1 dBTP, music dips under speech.",
                "Visual parity: burned-in export subtitle frames match browser preview (SSIM >= 0.95)."
            ],
            "done": [
                "Video with music and burned subtitles exports cleanly and sounds balanced across speakers and headphones.",
                "Editing one subtitle cue only invalidates segments covering that subtitle's timeframe."
            ],
            "fallback": "Keep to the short curated font list; custom fonts defer to v1.1."
        },
        {
            "id": "M10", "title": "Hardening and Local Acceptance",
            "scope": "PRD Sections 10–12 (local part), ADM-02, STO-03..05; TDD Section 19",
            "cost": "$0", "status": "PLANNED",
            "plain": "In plain words: We run the entire product in Docker exactly as it will run online, run the full test matrix on short and medium videos, and perform security and memory leak checks.",
            "learn": [
                ("Rust Profiling", "Using samply or flamegraph to identify CPU hotspots and memory allocations.", "Rust Profiling Tools"),
                ("Security Scanners", "cargo audit, cargo deny, pip-audit, pnpm audit, gitleaks in CI.", "DevSecOps Guides")
            ],
            "tasks": [
                ("M10-1", "Execute scripts/acceptance-matrix.py locally for 5- and 10-minute projects across all resolutions.", "Local test matrix"),
                ("M10-2", "Security pass: Content Security Policy, Argon2 tuning, ownership boundary test suite.", "Security audit"),
                ("M10-3", "Complete Janitor background task (3-day export cleanup, orphan upload aborts).", "Janitor service"),
                ("M10-4", "Verify full stack with make full (docker compose --profile full up --build).", "Full container stack"),
                ("M10-5", "Run Playwright end-to-end user journeys against full Docker stack.", "End-to-end journeys"),
                ("M10-6", "Write privacy policy notice and in-app data retention explanations.", "Compliance documentation")
            ],
            "tests": ["Automated acceptance test report generated and stored in docs/acceptance/."],
            "done": [
                "Every Must requirement passes its acceptance tests locally.",
                "Local acceptance matrix passes at >=95% with zero data loss bugs.",
                "make full boots and runs complete product in Docker without errors."
            ],
            "fallback": "Add zero new features here; anything new goes to v1.1 backlog."
        },
        {
            "id": "M11", "title": "Go Online",
            "scope": "TDD Section 18; PRD Release Criteria 3 (online) & 6",
            "cost": "~$5–8/month", "status": "PLANNED",
            "plain": "In plain words: PACE goes online for the first time. The web app deploys to Cloudflare Pages, the API and workers deploy to Railway with automatic sleep, and storage switches from MinIO to Backblaze B2.",
            "learn": [
                ("Railway PaaS", "Docker deployment, serverless sleeping, private networking, volume mounts.", "Railway Documentation"),
                ("Cloudflare Pages", "Deploying static SPAs, custom domains, security headers.", "Cloudflare Pages Docs"),
                ("Backblaze B2", "S3-compatible bucket permissions, application keys, CORS rules.", "Backblaze B2 Docs")
            ],
            "tasks": [
                ("M11-1", "Deploy render worker to Railway free trial; benchmark render speed against laptop.", "Cloud render benchmark"),
                ("M11-2", "Configure domain on Cloudflare; deploy web app to Cloudflare Pages (app.<domain>).", "Web app hosting"),
                ("M11-3", "Provision Backblaze B2 private bucket, scoped keys, and CORS rules; switch from MinIO to B2.", "Cloud storage setup"),
                ("M11-4", "Deploy API, PostgreSQL, and workers to Railway; enable serverless sleep and $8 spending limit.", "Production backend"),
                ("M11-5", "Configure daily database backup to B2 and practice a restore into a temporary database.", "Backup & recovery"),
                ("M11-6", "Run full acceptance matrix online, including 22-, 30-, and 40-minute videos.", "Online validation matrix")
            ],
            "tests": ["Automated online smoke test script: sign-in, upload, sync, and export 1-min video against production."],
            "done": [
                "Full acceptance matrix passes at >=95% online.",
                "Automated daily backups verified and restore practiced.",
                "Monthly operating cost verified at <=$8/month."
            ],
            "fallback": "If cloud renders cost more than expected, lower daily 1080p allowance or default to 720p."
        },
        {
            "id": "M12", "title": "Chrome Extension (Google Flow Import — Should)",
            "scope": "EXT-01..05, MED-09; TDD Section 16",
            "cost": "$0", "status": "PLANNED",
            "plain": "In plain words: While generating images in Google Flow, the creator clicks a 'Send to PACE' button to upload images and their generation prompts directly into their project without manual file saving.",
            "learn": [
                ("Chrome Extensions V3", "Service workers, content scripts, DOM injection, permissions scoping.", "Chrome Extensions Docs")
            ],
            "tasks": [
                ("M12-1", "Add API one-time pairing codes and scoped extension tokens.", "Extension auth API"),
                ("M12-2", "Build extension popup: enter pairing code, select project, view recent transfers.", "Popup UI"),
                ("M12-3", "Build content script (flow-adapter.ts): detect Google Flow images, inject 'Send to PACE' button.", "DOM injection"),
                ("M12-4", "Background script: fetch image blob, request presigned URL from API, upload to storage.", "Background upload")
            ],
            "tests": [
                "Adapter unit tests against saved Google Flow DOM snapshots.",
                "Security: extension tokens rejected on all non-extension API routes."
            ],
            "done": [
                "20 images sent from live Google Flow session arrive in order with prompts.",
                "DOM changes in Google Flow fail gracefully with a notice suggesting manual upload."
            ],
            "fallback": "Optional for v1. If time is short, ship v1 with manual upload and make this the first v1.1 item."
        },
        {
            "id": "M13", "title": "Private Beta",
            "scope": "PRD Release Criterion 5",
            "cost": "~$5–8/month", "status": "PLANNED",
            "plain": "In plain words: Three invited video creators use PACE to produce their own real videos. We watch for confusion or errors, fix issues immediately, and prove the product works before wider release.",
            "learn": [
                ("User Feedback Synthesis", "Categorizing creator feedback into blockers, usability polish, and v1.1 features.", "Product Management")
            ],
            "tasks": [
                ("M13-1", "Create accounts for 3 invited creators and provide a 1-page onboarding guide.", "Beta onboarding"),
                ("M13-2", "Monitor error telemetry (Sentry) and server logs during user runs.", "Active monitoring"),
                ("M13-3", "Fix any blocker or confusion immediately; list enhancements for v1.1.", "Rapid bug triage"),
                ("M13-4", "Tag release v1.0.0 and archive final acceptance report.", "Official v1.0.0 release")
            ],
            "tests": ["Run online smoke tests after every hotfix deployment."],
            "done": [
                "Each of the 3 creators exports at least one complete 20+ minute video without developer help.",
                "Zero data loss and zero critical bugs during the beta period."
            ],
            "fallback": "If a user is blocked by confusion rather than a bug, fix the UI wording before declaring v1 done."
        }
    ]
    
    # Render all milestones into document
    for m in milestones_data:
        h1 = doc.add_heading(f"Milestone {m['id']}: {m['title']}", level=1)
        h1.paragraph_format.space_before = Pt(16)
        h1.paragraph_format.space_after = Pt(4)
        
        # Meta info table
        m_tbl = doc.add_table(rows=4, cols=2)
        m_tbl.cell(0, 0).paragraphs[0].add_run("PRD Scope").bold = True
        m_tbl.cell(0, 1).paragraphs[0].add_run(m["scope"])
        m_tbl.cell(1, 0).paragraphs[0].add_run("Cost").bold = True
        m_tbl.cell(1, 1).paragraphs[0].add_run(m["cost"])
        m_tbl.cell(2, 0).paragraphs[0].add_run("Current Status").bold = True
        m_tbl.cell(2, 1).paragraphs[0].add_run(m["status"])
        m_tbl.cell(3, 0).paragraphs[0].add_run("Fallback Strategy").bold = True
        m_tbl.cell(3, 1).paragraphs[0].add_run(m["fallback"])
        format_table(m_tbl, [1.8, 5.2], header_bg="1E293B")
        doc.add_paragraph().paragraph_format.space_after = Pt(6)
        
        # Plain words callout
        add_callout(doc, f"Milestone {m['id']} in Plain Words", m["plain"], bg_color="F8FAFC", border_color="2563EB")
        
        # Learn First
        doc.add_heading("Learn First (Concepts & Resources)", level=2).paragraph_format.space_before = Pt(8)
        l_tbl = doc.add_table(rows=len(m["learn"]) + 1, cols=3)
        l_tbl.cell(0, 0).paragraphs[0].add_run("Topic")
        l_tbl.cell(0, 1).paragraphs[0].add_run("What You Learn")
        l_tbl.cell(0, 2).paragraphs[0].add_run("Resource")
        for idx, (top, desc, res) in enumerate(m["learn"], start=1):
            l_tbl.cell(idx, 0).paragraphs[0].add_run(top).bold = True
            l_tbl.cell(idx, 1).paragraphs[0].add_run(desc)
            l_tbl.cell(idx, 2).paragraphs[0].add_run(res)
        format_table(l_tbl, [1.6, 3.6, 1.8], header_bg="334155")
        doc.add_paragraph().paragraph_format.space_after = Pt(6)
        
        # Tasks
        doc.add_heading("Build Steps (In Order)", level=2).paragraph_format.space_before = Pt(8)
        t_tbl = doc.add_table(rows=len(m["tasks"]) + 1, cols=3)
        t_tbl.cell(0, 0).paragraphs[0].add_run("Step")
        t_tbl.cell(0, 1).paragraphs[0].add_run("Action / Implementation Details")
        t_tbl.cell(0, 2).paragraphs[0].add_run("Expected Output")
        for idx, (t_id, desc, out) in enumerate(m["tasks"], start=1):
            t_tbl.cell(idx, 0).paragraphs[0].add_run(t_id).bold = True
            t_tbl.cell(idx, 1).paragraphs[0].add_run(desc)
            t_tbl.cell(idx, 2).paragraphs[0].add_run(out)
        format_table(t_tbl, [0.8, 4.4, 1.8], header_bg="0F172A")
        doc.add_paragraph().paragraph_format.space_after = Pt(6)
        
        # Tests & Done
        doc.add_heading("Tests to Write First (TDD)", level=2).paragraph_format.space_before = Pt(8)
        for t in m["tests"]:
            p_t = doc.add_paragraph(style='List Bullet')
            p_t.paragraph_format.space_before = Pt(1)
            p_t.paragraph_format.space_after = Pt(2)
            run = p_t.add_run(t)
            run.font.name = 'Segoe UI'
            run.font.size = Pt(9.5)
            
        doc.add_heading("Done When Checklist", level=2).paragraph_format.space_before = Pt(8)
        for d in m["done"]:
            p_d = doc.add_paragraph(style='List Bullet')
            p_d.paragraph_format.space_before = Pt(1)
            p_d.paragraph_format.space_after = Pt(2)
            run = p_d.add_run(f"[ ] {d}")
            run.font.name = 'Segoe UI'
            run.font.size = Pt(9.5)
            run.bold = True
            
        doc.add_paragraph().paragraph_format.space_after = Pt(12)
        
    # Section 4: Universal Definition of Done
    doc.add_page_break()
    h1 = doc.add_heading("4. Universal Definition of Done for Any Task", level=1)
    h1.paragraph_format.space_before = Pt(12)
    h1.paragraph_format.space_after = Pt(6)
    
    dod_items = [
        ("Test-First Verification", "Its PRD acceptance criteria are covered by failing unit/integration tests written before the implementation code."),
        ("Automated Contracts", "cargo xtask gen has been run; generated TypeScript types and Python models are committed with no diffs."),
        ("Clean Toolchain", "Zero warnings under cargo clippy -D warnings, cargo fmt, Ruff, mypy, and TypeScript strict mode."),
        ("Environment Parity", "Verified in daily local development mode AND verified in make full (full Docker stack) before milestone sign-off."),
        ("Error Standardization", "Every error has a documented UPPER_SNAKE error code (ApiError) and a plain-English user explanation."),
        ("Architecture Integrity", "Any change to architecture or requirements is documented in docs/adr/ and reflected in PRD/TDD."),
        ("Learning Notes", "Three lines of personal learning notes and takeaways recorded in learning/mNN-*.md.")
    ]
    dod_tbl = doc.add_table(rows=len(dod_items) + 1, cols=2)
    dod_tbl.cell(0, 0).paragraphs[0].add_run("Quality Gate")
    dod_tbl.cell(0, 1).paragraphs[0].add_run("Requirement")
    for idx, (g, r) in enumerate(dod_items, start=1):
        dod_tbl.cell(idx, 0).paragraphs[0].add_run(g).bold = True
        dod_tbl.cell(idx, 1).paragraphs[0].add_run(r)
    format_table(dod_tbl, [2.2, 4.8], header_bg="0F172A")
    
    # Save
    doc.save(output_path)
    print(f"Successfully generated DOCX implementation plan at: {output_path}")

if __name__ == "__main__":
    out_dir = "docs/product"
    os.makedirs(out_dir, exist_ok=True)
    out_file = os.path.join(out_dir, "PACE_Implementation_Plan.docx")
    build_docx(out_file)
