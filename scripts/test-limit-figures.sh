#!/usr/bin/env bash
# Tests scripts/limit-figures.sh (REQ-6608) on a repository made for it in
# tmp/: a limits file of two limits, a file naming each by name alone,
# which passes, and then each way of stating a figure beside a name,
# each of which must fail.
set -euo pipefail
cd "$(dirname "$0")/.."
# Run from a git hook, these name the repository being committed: unset,
# every git command here is the made repository's alone.
unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE

mkdir -p tmp
work="$(mktemp -d tmp/limit-figures.XXXXXX)"
trap 'rm -rf "$work"' EXIT

cat >"$work/limits.rs" <<'LIMITS'
pub const MAX_GROUPS_BYTES: usize = 16_384;
pub const MAX_GROUP_DEPTH: usize = 4;
pub const TICKS_PER_SECOND: u32 = 60;
pub const RULE_57: [(&str, u32); 2] = [
    ("MAX_GROUPS_BYTES", MAX_GROUPS_BYTES as u32),
    ("MAX_GROUP_DEPTH", MAX_GROUP_DEPTH as u32),
];
LIMITS
repository="$work/repository"
git init -q "$repository"
# The limit of depth 4 is named on line 4, so that a line number is never
# taken for a figure.
clean=$'One.\nTwo.\nThree.\nThe groups take at most `MAX_GROUPS_BYTES` (RULE-4); a chain is MAX_GROUP_DEPTH deep, 40 in all, at 1,6384 bytes.'
printf '%s\n' "$clean" >"$repository/notes.md"
git -C "$repository" add notes.md

passes() { scripts/limit-figures.sh "$work/limits.rs" "$repository" >/dev/null; }
if ! passes; then
    echo "limit-figures: a file naming the limits by name alone was refused" >&2
    exit 1
fi
for planted in 'MAX_GROUPS_BYTES is 16384' 'MAX_GROUPS_BYTES: 16_384 bytes' '`MAX_GROUPS_BYTES` (16,384)' 'let depth = 4; // MAX_GROUP_DEPTH' \
    'TICKS_PER_SECOND = 60'; do
    printf '%s\n%s\n' "$clean" "$planted" >"$repository/notes.md"
    if passes; then
        echo "limit-figures: \"$planted\" was not found" >&2
        exit 1
    fi
done
# An untracked file is not the repository's.
printf '%s\n' "$clean" >"$repository/notes.md"
printf 'MAX_GROUPS_BYTES is 16384\n' >"$repository/untracked.md"
passes || { echo "limit-figures: an untracked file was read" >&2; exit 1; }
echo "limit-figures: every planted figure found, the clean file passed"
