---
# soltrace-8182
title: Apply rustfmt (style_edition 2024) across workspace src files
status: completed
type: task
created_at: 2026-09-28T12:19:32Z
updated_at: 2026-09-28T12:19:32Z
---

Reorder imports per rustfmt style_edition 2024 (lowercase/snake_case before CamelCase) and collapse assert! wraps. Pure formatting; cargo test --workspace green before and after.
