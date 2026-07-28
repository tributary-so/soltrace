---
# soltrace-b6wk
title: Review Epic 3 + milestone close
status: todo
type: task
created_at: 2026-07-28T08:32:14Z
updated_at: 2026-07-28T08:32:14Z
parent: soltrace-87oa
blocked_by:
    - soltrace-b4md
---

See MS soltrace-z9wq HANDOFF. Final review before milestone close.

## Cut

Final review across all epics. Verify against Definition of Done (HANDOFF section 5):

- [ ] --onchain-programs flag works end-to-end in both binaries
- [ ] Live subscription swaps in new IDL on SetData push (verified by Epic 2 tests or localnet demo)
- [ ] Subscription survives WS disconnect (reconnect + re-subscribe)
- [ ] SetImmutable -> unsubscribe; Close -> drop + warn
- [ ] File IDL precedence verified: file IDL for same program as on-chain wins
- [ ] Backfill one-shot fetch verified
- [ ] No behavioral change when --onchain-programs is empty
- [ ] idls/README.md updated (Decision 2, 1, 5, 7 + program-metadata link)
- [ ] cargo build --workspace --all-features green
- [ ] cargo test --workspace green
- [ ] cargo clippy --workspace --all-features -- -D warnings green
- [ ] Ponytail: no speculative abstractions introduced across the milestone

If all green, mark this bean completed AND milestone soltrace-z9wq completed. If issues found, block with comments.

## Summary of Changes (required at completion)
Update this bean with a summary of what was built, files added/modified, deps added, and any deviations from the original HANDOFF (with rationale).
