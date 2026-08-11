# AGENTS.md — dravr-browser

Agent rules for this repo. Validation: run `./.build/validation/validate.sh` before pushing.

## Limitation register (org-wide)

- A genuine, documented limitation in code carries `LIMITATION(registre#<issue>):` on the marker line, naming the limited item, backed by an issue in the **private** `dravr-ai/dravr-carnet` tracker (labels `limitation` + this repo's name). Most dravr repos are PUBLIC — internal gaps and security residuals never go on this repo's own tracker.
- Deferral/confession prose ("for now", "not yet implemented", "is the follow-up", "in a follow-up commit", "not yet wired", "not threaded through") is CI-gated by `.build/validation/limitation-gates.sh` (invoked by `validate.sh`); a registered marker line is the only exemption. Implement the real thing, or register the gap — never document it unregistered.
- A capability declared but consumed only by tests is a phantom surface: wire a production consumer in the same change, or register it with a marker naming the item.
- A feature shipped disarmed (flag off, shadow/observe mode, log-only phase) gets a `feature-phases.yaml` entry (name/surface/current/advance_when/review_by); dravr-build-config's reusable `feature-phase-review` workflow opens a registre issue when the review date passes.