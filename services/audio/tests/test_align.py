from pace_audio.align import (
    SectionInput,
    SilenceGap,
    SpokenWord,
    calculate_boundaries,
    generate_subtitles,
    normalize_text,
)


def _spoken(words_and_times):
    return [
        SpokenWord(word=w, start_ms=s, end_ms=e, probability=0.95)
        for w, s, e in words_and_times
    ]


def test_normalize_text_numbers_years_and_punctuation():
    assert normalize_text("In 1920, Rome's empire—expanded!") == [
        "in",
        "nineteen",
        "twenty",
        "rome's",
        "empire",
        "expanded",
    ]
    assert normalize_text("Chapter 3: café") == ["chapter", "three", "café"]


def test_exact_skipped_extra_words_and_repeated_sentence():
    sections = [
        SectionInput(id="s1", position=0, text="In 1920 the clock struck twelve."),
        SectionInput(id="s2", position=1, text="A brave pilot crossed the ocean alone."),
        SectionInput(id="s3", position=2, text="The city celebrated the return for days."),
    ]
    spoken = _spoken(
        [
            # Sec 1 (year punctuation formatted differently by Whisper!)
            ("In", 100, 300),
            ("1920,", 320, 750),
            ("clock", 800, 1100),  # skipped "the"
            ("struck", 1120, 1450),
            ("twelve.", 1480, 1900),
            # Ad-lib extra words before pause at 2200..2600
            ("Well", 2650, 2850),
            # Sec 2 repeated phrase attempt -> should align cleanly forward
            ("A", 3000, 3100),
            ("brave", 3120, 3450),
            ("pilot", 3470, 3850),
            ("crossed", 3870, 4200),
            ("ocean", 4250, 4550),
            ("alone.", 4570, 4950),
            # Pause at 5000..5400ms (midpoint 5200ms)
            ("The", 5450, 5600),
            ("city", 5620, 5950),
            ("celebrated", 5970, 6450),
            ("the", 6470, 6580),
            ("return", 6600, 6950),
            ("for", 6970, 7150),
            ("days.", 7180, 7600),
        ]
    )
    silences = [
        SilenceGap(start_ms=2100, end_ms=2600, duration_ms=500),
        SilenceGap(start_ms=5000, end_ms=5400, duration_ms=400),
    ]

    out = calculate_boundaries(sections, spoken, silences, narration_duration_ms=8000)
    assert len(out.boundaries) == 4
    assert out.boundaries[0].time_ms == 0
    assert out.boundaries[-1].time_ms == 8000
    # Strictly increasing boundaries
    assert all(out.boundaries[i].time_ms < out.boundaries[i + 1].time_ms for i in range(3))
    # Boundary 2 should snap cleanly into pause 5000..5400 (~5200ms)
    assert 4900 <= out.boundaries[2].time_ms <= 5450
    # Long normal speech (~3s) must NOT raise Unexpected long gap
    assert "Unexpected long gap" not in out.boundaries[1].flags
    assert "Unexpected long gap" not in out.boundaries[2].flags


def test_long_gap_flag_only_when_gap_before_section_exceeds_4s():
    sections = [
        SectionInput(id="s1", position=0, text="Opening line ends cleanly."),
        SectionInput(id="s2", position=1, text="Second part starts after five seconds."),
    ]
    spoken = _spoken(
        [
            ("Opening", 100, 500),
            ("line", 520, 800),
            ("ends", 820, 1100),
            ("cleanly", 1120, 1500),
            # 5.0s silent gap (1500..6500)
            ("Second", 6500, 6900),
            ("part", 6920, 7200),
            ("starts", 7220, 7550),
            ("after", 7570, 7850),
            ("five", 7870, 8150),
            ("seconds", 8170, 8600),
        ]
    )
    out = calculate_boundaries(sections, spoken, [], narration_duration_ms=9000)
    assert "Unexpected long gap" in out.boundaries[1].flags


def test_unspoken_section_flagged_and_manual_boundaries_preserved():
    sections = [
        SectionInput(id="s1", position=0, text="First section spoken completely here."),
        SectionInput(id="s2", position=1, text="This entire middle paragraph was skipped completely by narrator."),
        SectionInput(id="s3", position=2, text="Final section returns on script."),
    ]
    spoken = _spoken(
        [
            ("First", 100, 400),
            ("section", 420, 800),
            ("spoken", 820, 1200),
            ("completely", 1220, 1700),
            ("here", 1720, 2000),
            ("Final", 2500, 2900),
            ("section", 2920, 3300),
            ("returns", 3320, 3700),
            ("on", 3720, 3900),
            ("script", 3920, 4400),
        ]
    )
    out = calculate_boundaries(
        sections,
        spoken,
        [],
        narration_duration_ms=5000,
        manual_boundaries={2: 2450},
    )
    assert "Section not found" in out.boundaries[1].flags
    assert out.boundaries[2].source == "manual"
    assert out.boundaries[2].time_ms == 2450


def test_subtitles_use_script_spelling_continuous_positions_and_aspect_limits():
    sections = [
        SectionInput(
            id="s1",
            position=0,
            text="This very long sentence keeps going past eighty characters so every word must appear in subtitles.",
        ),
        SectionInput(
            id="s2",
            position=1,
            text="Short finale sentence.",
        ),
    ]
    spoken = _spoken(
        [
            ("This", 100, 300),
            ("very", 320, 500),
            ("long", 520, 700),
            ("sentence", 720, 1000),
            ("keeps", 1020, 1250),
            ("going", 1270, 1500),
            ("past", 1520, 1750),
            ("eighty", 1770, 2050),
            ("characters", 2070, 2500),
            ("so", 2520, 2700),
            ("every", 2720, 3000),
            ("word", 3020, 3250),
            ("must", 3270, 3500),
            ("appear", 3520, 3800),
            ("in", 3820, 3950),
            ("subtitles", 3970, 4400),
            ("Short", 4700, 5000),
            ("finale", 5020, 5400),
            ("sentence", 5420, 5900),
        ]
    )
    out = calculate_boundaries(sections, spoken, [], narration_duration_ms=6500, aspect_ratio="9:16")
    cues = out.subtitle_cues
    assert len(cues) >= 2
    assert [c.position for c in cues] == list(range(len(cues)))
    combined = " ".join(c.text.replace("\n", " ") for c in cues)
    assert "subtitles." in combined
    assert "finale sentence." in combined
    for cue in cues:
        for line in cue.text.splitlines():
            assert len(line) <= 32
