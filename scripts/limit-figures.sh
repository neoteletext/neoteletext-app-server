#!/usr/bin/env bash
# REQ-6608 (page-rules.md RULE-57): every limit of a page's rule table is
# one named constant of the engine's `neoteletext_rules::limits`, used by
# name, and no other file states one of its figures. This fails when a
# tracked file of the repository at DIRECTORY has a line that names a
# limit and also holds that limit's figure as a number of its own (16384,
# 16_384 or 16,384; not a digit of a longer number, a word or a clause
# such as RULE-4).
#   scripts/limit-figures.sh LIMITS_RS DIRECTORY
# LIMITS_RS is the engine's crates/neoteletext-rules/src/limits.rs: the
# limits are the names of its RULE_57 table and TICKS_PER_SECOND, each
# with the literal its `pub const` gives.
set -euo pipefail

limits_rs="$1"
directory="$2"
[[ -f "$limits_rs" ]] || { echo "no limits file at $limits_rs" >&2; exit 2; }

# The names, from the RULE_57 table, and the clock's (value_of answers it
# too).
names="$(sed -n '/^pub const RULE_57/,/^];/p' "$limits_rs" | sed -n 's/^ *("\([A-Z0-9_]*\)",.*/\1/p')"
names="$names"$'\n'"TICKS_PER_SECOND"
[[ "$(wc -l <<<"$names")" -gt 1 ]] || { echo "no RULE_57 table in $limits_rs" >&2; exit 2; }

found=0
while read -r name; do
    figure="$(sed -n "s/^pub const $name: [a-z0-9]* = \([0-9_]*\);.*/\1/p" "$limits_rs" | tr -d _)"
    [[ -n "$figure" ]] || { echo "$name has no plain integer value in $limits_rs" >&2; exit 2; }
    # The figure, its thousands optionally separated by _ or ,.
    pattern="$(sed -E ':a; s/([0-9])([0-9]{3})($|[^0-9])/\1[_,]?\2\3/; ta' <<<"$figure")"
    figure_alone="(^|[^0-9A-Za-z_.-])$pattern(\$|[^0-9A-Za-z_])"
    # Each line's text alone is matched, not the path and line number
    # git grep puts before it. The check's own test states figures on
    # purpose, as what must be found.
    hits=""
    while IFS= read -r -d '' path && IFS= read -r -d '' number && IFS= read -r text; do
        if [[ "$text" =~ $figure_alone ]]; then
            hits+="$path:$number: $text"$'\n'
        fi
    done < <(git -C "$directory" grep -n -z -I -w -e "$name" -- . ':!Cargo.lock' ':!fuzz/Cargo.lock' ':!scripts/test-limit-figures.sh' || true)
    if [[ -n "$hits" ]]; then
        printf '%s is %s: state it by name only (REQ-6608)\n%s' "$name" "$figure" "$hits"
        found=1
    fi
done <<<"$names"
exit "$found"
