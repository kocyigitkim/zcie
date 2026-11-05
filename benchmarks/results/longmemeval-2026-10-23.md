# LongMemEval retrieval test — 2026-10-23 (ADR-028, D-096)

- Data: `longmemeval_oracle.json` of `xiaowu0162/longmemeval-cleaned` (500 questions; the history has only the evidence sessions). The first 200 usable questions (no abstention, answer of 60 characters or fewer).
- Command: `python3 run.py longmemeval_oracle.json out.jsonl 200 6`. A hit: the answer text is in the title or snippet of the top 6 results of `/v1/query`. No model reads the results.

| question type | questions | answer in top 6 |
|---|---|---|
| knowledge-update | 69 | 53 (77%) |
| multi-session | 49 | 6 (12%) |
| single-session-assistant | 33 | 18 (55%) |
| temporal-reasoning | 49 | 13 (27%) |
| all | 200 | 90 (45%) |

## Limits of this measurement
- Many multi-session and temporal answers are computed (a count, a number of days); the text of the answer is not in any message. A literal match cannot find them, so these two values are low limits, not recall.
- The value is not comparable with published LongMemEval scores, which grade the answer of a model.
- knowledge-update (77 %) and single-session-assistant (55 %) are the meaningful values. The second is lower because a new home stores assistant messages only with `capture.assistant_messages = true`, and assistant statements rank ×0.8 (ADR-020).
