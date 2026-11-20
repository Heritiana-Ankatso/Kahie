# P0-16 — Handwriting corpus collection protocol

- **Status:** Draft for product-owner approval
- **Date:** 2026-11-20
- **Feeds:** P0-06 (recognition benchmark), WS3 targets, decision D2
- **Languages (D5):** French, English, **Malagasy** — handwriting recognition for all three at launch

## 1. Purpose

Build a small, consented corpus of real handwriting from teachers and students on the reference devices, to:

1. **Evaluate** recognition engines (text character error rate, maths exact match, shape accuracy) — P0-06.
2. **Train / fine-tune** open models where licences allow, in particular for Malagasy text, for which no public online-handwriting dataset is known to us.

## 2. What is collected

| Data | Collected | Notes |
|---|---|---|
| Pen strokes: x, y, pressure, timestamp per point | Yes | Raw, unmodified |
| Device model, screen size, stylus or finger | Yes | For per-device analysis |
| The prompt shown and the label (expected text / LaTeX / shape) | Yes | Ground truth |
| Writer metadata: age band (< 12, 12–17, 18–25, 26–40, > 40), handedness, languages written fluently, school level / teacher | Yes, optional | No exact age |
| Name, e-mail, school name, photo, location, IP address | **No** | Never recorded |
| Free text written by the participant about themselves | **No** | Prompts are fixed; no free writing |

Each writer gets a random identifier (`w-` + 8 hex characters) generated on the device. The link between a person and their identifier is **not** kept anywhere.

## 3. Consent

A one-page consent form, in French, English and Malagasy, read and signed (paper or on screen) **before** any collection. For participants under 18, a parent or guardian also signs.

The form states:

- **Purpose:** improving handwriting recognition in Kahie, an open-source (GPL-3) notebook app.
- **What is recorded** (table above) and what is not.
- **Two separate choices** (both optional, both default "no"):
  1. *Evaluation:* the samples may be used to measure recognition quality and may be published in the open-source repository as test data.
  2. *Training:* the samples may be used to train recognition models, which may be published under an open licence.
- **Retention:** kept until the participant withdraws, or at most 10 years.
- **Withdrawal:** at any time, by giving the writer identifier shown at the end of the session (printed on a slip given to the participant) to the project contact; samples are deleted from the private store and from the next release of public data. Already-published model weights cannot be "un-trained"; this is stated plainly.
- **Contact:** the project's product owner (address filled in on the printed form).

Samples from a participant who gave neither consent are not kept.

## 4. Anonymisation

- No personal identifiers are recorded (section 2).
- Before storage, each sample is checked by a second person for accidental personal data (a participant writing their own name instead of the prompt). Such samples are deleted.
- Timestamps are stored relative to the start of each stroke group, not as wall-clock time.

## 5. Storage policy

| Consent | Where | Access |
|---|---|---|
| Evaluation = yes | `testdata/hwr/<language>/` in the repository (public) | Everyone |
| Training = yes, evaluation = no | Private encrypted bucket chosen by the product owner | ML engineers on the project |
| Neither | Not kept | — |

Evaluation and training sets never share a writer (writer-level split), so evaluation measures generalisation to new handwriting.

## 6. Prompts and target sizes

Targets for the P0-06 benchmark, per language. A session is about 20 minutes (≈ 60 prompts).

| Content | French | English | Malagasy | Notes |
|---|---|---|---|---|
| Words and short phrases | 1 500 | 1 500 | 2 500 | Malagasy higher: no public dataset; include the diacritics used in Malagasy (e.g. *tanàna*) and long derived forms (e.g. *mpampianatra*, *fampianarana*); prompt lists reviewed by a native speaker before use |
| Sentences (10–15 words) | 300 | 300 | 500 | From open-licence school texts |
| Numbers, dates, units | 300 | 300 | 300 | Shared prompts; local conventions (decimal comma) |
| Maths expressions | 1 000 (shared across languages) | | | Lycée level: fractions, powers, roots, sums, integrals, matrices; complements CROHME / MathWriting |
| Shapes and curves | 500 (shared) | | | Lines, circles, polygons, arrows, function sketches |

**Writers:** at least 30 per language, balanced between students and teachers, at least 20 % left-handed if possible, on at least two reference devices (P0-15).

## 7. Collection tool

`app/web/collect.html` — runs in any browser, offline, on the reference tablets:

1. The organiser chooses the language and prompt set; the participant enters only optional metadata (section 2).
2. One prompt at a time with *Next* / *Clear* / *Skip*; the raw strokes are kept exactly as sampled.
3. At the end, the tool shows the writer identifier and downloads one JSON file (format below). Nothing is sent over the network.

```json
{
  "format": "kahie-hwr-corpus", "version": 1,
  "writer": { "id": "w-3fa9c1d2", "age_band": "12-17", "handedness": "left", "role": "student",
              "languages": ["mg", "fr"] },
  "consent": { "evaluation": true, "training": false },
  "device": { "user_agent": "…", "screen": [1600, 2560], "dpr": 2, "pointer": "pen" },
  "language": "mg",
  "samples": [
    { "prompt_id": "mg-w-0042", "kind": "text", "label": "fampianarana",
      "strokes": [ { "points": [[x, y, pressure, t_ms], …] } ] }
  ]
}
```

## 8. Approval checklist

- [ ] Product owner approves this protocol and the consent text (all three languages).
- [ ] Organisers trained on the consent process (one page brief).
- [ ] Collection tool tested by a non-engineer on a reference tablet.
- [ ] Private bucket created (for training-only samples) with access list.
