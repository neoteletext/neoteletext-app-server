#!/usr/bin/env bash
# The project's one check script (requirement 2.1). Every step either
# passes or fails the run: nothing is skipped, a missing tool or an
# unreachable host is a failure.
#   scripts/check.sh commit   the pre-commit gate: formatting, lints in
#                             every shape and tests, refused while the
#                             checked parts have changes that are not
#                             staged, so what is checked is what is
#                             committed
#   scripts/check.sh full     the pre-push gate: everything, on a clean
#                             tree at the commit being pushed, including
#                             release tests, licences and advisories,
#                             unused dependencies, fuzzing, and the Linux
#                             build and tests on $TELETEXT_LINUX_HOST
# The channels' programs and their kits have their own gates, in
# ~/devel/neoteletext-server-channels (decision 12).
set -euo pipefail
cd "$(dirname "$0")/.."

mode="${1:-}"
# The Linux host of the neoteletext full gates (DEVELOPMENT.md section 12).
linux_host="${TELETEXT_LINUX_HOST:-foundintelquiet4060}"
# Seconds each fuzz target runs in the full gate, after replaying every
# input that has ever found a fault (fuzz/regressions/).
fuzz_seconds="${TELETEXT_FUZZ_SECONDS:-20}"
# Cloudflare Workers' target, which the runtime must build for
# (requirement 1.3).
readonly WASM_TARGET="wasm32-unknown-unknown"
# The crates that build for WebAssembly: everything a Worker links.
readonly WASM_CRATES=(teletext-runtime teletext-cloudflare)

# Timing and stamps (~/devel/neoteletext-common/DEVELOPMENT.md section 9):
# each step prints its time as it ends; a pass records its steps' times,
# for the next run's warning, and the tree it passed on, so a higher tier
# does not run it again.
# The Linux host's copy has no git directory, and records nothing.
git_dir="$(git rev-parse --git-common-dir 2>/dev/null || true)"
step_name=""
step_start=$SECONDS
step_times=""
minutes() { printf '%dm%02ds' $(($1 / 60)) $(($1 % 60)); }
end_step() {
    [[ -n "$step_name" ]] || return 0
    local took=$((SECONDS - step_start))
    printf '   (%s)\n' "$(minutes "$took")"
    step_times+="$took	$step_name"$'\n'
    step_name=""
}
step() {
    end_step
    step_name="$*"
    step_start=$SECONDS
    printf '\n== %s\n' "$*"
}
announce() {
    local file="$git_dir/gate-times-$1"
    if [[ -f "$file" ]]; then
        printf '== the %s gate: its last pass took %s; its slowest steps:\n' "$1" "$(minutes "$(head -1 "$file")")"
        tail -n +2 "$file" | sort -rn | head -3 | while IFS=$'\t' read -r took name; do
            printf '   %s  %s\n' "$(minutes "$took")" "$name"
        done
    else
        printf '== the %s gate: no pass measured yet\n' "$1"
    fi
}
passed() {
    local tree="$2"
    [[ -n "$tree" && -f "$git_dir/gate-passed-$1" && "$(cat "$git_dir/gate-passed-$1")" == "$tree" ]]
}
stamp() {
    [[ -z "$2" ]] || printf '%s\n' "$2" >"$git_dir/gate-passed-$1"
}
record_pass() {
    local tier="$1" tree="$2" took=$((SECONDS - gate_start))
    end_step
    printf '%s\n%s' "$took" "$step_times" >"$git_dir/gate-times-$tier"
    stamp "$tier" "$tree"
    printf '\nthe %s gate passed in %s\n' "$tier" "$(minutes "$took")"
}
fail() {
    printf '\nFAILED: %s\n' "$*" >&2
    exit 1
}

. scripts/tools.sh
out_of_date="$(tools_out_of_date)"
[[ -z "$out_of_date" ]] || fail "tools missing or at another version (run scripts/tools.sh):
$out_of_date"

# The crates that read input from outside their process, each with the
# same no-panic lint list at its root. Every crate root must be listed.
readonly NO_PANIC_ROOTS=(crates/teletext-runtime/src/lib.rs crates/teletext-native/src/lib.rs crates/teletext-native/src/main.rs
    crates/teletext-native/src/bin/teletext-load.rs crates/teletext-cloudflare/src/lib.rs)

check_no_panic_lists() {
    step "the no-panic lint list is the same in every crate root"
    local first="" root list
    for root in crates/*/src/lib.rs crates/*/src/main.rs crates/*/src/bin/*.rs; do
        [[ -f "$root" ]] || continue
        [[ " ${NO_PANIC_ROOTS[*]} " == *" $root "* ]] || fail "$root is a crate root missing from NO_PANIC_ROOTS"
    done
    for root in "${NO_PANIC_ROOTS[@]}"; do
        [[ -f "$root" ]] || fail "$root is listed in NO_PANIC_ROOTS but does not exist"
        list="$(sed -n '/^#!\[deny(/,/^)\]/p' "$root")"
        [[ -n "$list" ]] || fail "$root has no #![deny(...)] list"
        if [[ -z "$first" ]]; then
            first="$list"
        elif [[ "$list" != "$first" ]]; then
            fail "$root's #![deny(...)] list differs from ${NO_PANIC_ROOTS[0]}'s"
        fi
    done
}

# The engine is imported at one commit, never copied (CLAUDE.md).
check_engine_pin() {
    step "the engine is pinned at one commit"
    local manifests=(Cargo.toml crates/*/Cargo.toml) revs
    [[ -f fuzz/Cargo.toml ]] && manifests+=(fuzz/Cargo.toml)
    revs="$(sed -n 's/^neoteletext-[a-z0-9-]* = { git = "[^"]*", rev = "\([0-9a-f]*\)".*/\1/p' "${manifests[@]}" | sort -u)"
    if grep -q '^neoteletext-[a-z0-9-]* = ' "${manifests[@]}"; then
        [[ -n "$revs" && "$revs" != *$'\n'* ]] || fail "every engine crate must name one commit, found: ${revs:-none}"
    else
        printf 'no engine crate imported yet\n'
    fi
}

check_workspace() {
    check_engine_pin
    step "lockfiles up to date"
    cargo metadata --quiet --locked --format-version 1 >/dev/null
    if [[ -f fuzz/Cargo.toml ]]; then
        (cd fuzz && cargo metadata --quiet --locked --format-version 1 >/dev/null)
    fi
    step "formatting"
    cargo fmt --check
    check_no_panic_lists
    step "Clippy, warnings as errors"
    cargo clippy --quiet --workspace --all-targets -- -D warnings
}

# What the push gate adds: every shape, and the tests.
check_shapes_and_tests() {
    step "Clippy with every optional feature compiled out, warnings as errors"
    cargo clippy --quiet --workspace --all-targets --no-default-features -- -D warnings
    step "Clippy for $WASM_TARGET, warnings as errors: ${WASM_CRATES[*]}"
    local crate packages=()
    for crate in "${WASM_CRATES[@]}"; do packages+=(-p "$crate"); done
    cargo clippy --quiet "${packages[@]}" --target "$WASM_TARGET" -- -D warnings
    step "tests"
    cargo nextest run --workspace --hide-progress-bar --status-level fail --final-status-level fail --no-tests=warn
}

# Paths with changes that are not staged, or files git does not track,
# under any of the given paths.
unstaged_under() {
    git diff --name-only -- "$@"
    git ls-files --others --exclude-standard -- "$@"
}

commit_gate() {
    local staged
    staged="$(git diff --cached --name-only)"
    if [[ -z "$staged" ]]; then
        step "nothing staged"
        return
    fi
    local code=no path
    while IFS= read -r path; do
        case "$path" in
        crates/* | fuzz/* | scripts/* | .githooks/* | Cargo.toml | Cargo.lock | rust-toolchain.toml | clippy.toml | rustfmt.toml | deny.toml) code=yes ;;
        esac
    done <<<"$staged"
    # What is checked must be what is committed: no unstaged edit or
    # untracked file may sit in a part being checked.
    if [[ "$code" == yes ]]; then
        local loose
        loose="$(unstaged_under crates fuzz scripts .githooks Cargo.toml Cargo.lock rust-toolchain.toml clippy.toml rustfmt.toml deny.toml)"
        [[ -z "$loose" ]] || fail "changes that are not staged would make this check test something other than the commit; stage or stash them:
$loose"
        tree="$(git write-tree)"
        if passed commit "$tree"; then
            printf '\n== the commit gate already passed on this tree\n'
            tree=""
        else
            check_workspace
        fi
    fi
    return 0
}

# Runs alongside the steps here in the full gate; its output is shown
# when they are done. The copy compares content and keeps none of this
# machine's times, which would make a file edited here before the host's
# last build look older than that build.
check_linux() {
    step_name=""
    step "Linux on $linux_host: Clippy and tests"
    ssh -o BatchMode=yes -o ConnectTimeout=10 "$linux_host" true || fail "$linux_host is unreachable; the full gate needs it"
    rsync -a --no-times --checksum --delete --exclude target --exclude tools --exclude .git --exclude fuzz/corpus --exclude fuzz/artifacts \
        ./ "$linux_host:teletext-server-check/"
    # The engine is a git dependency on its public repository, which the
    # host fetches by itself. rustup installs the toolchain
    # rust-toolchain.toml pins if the host does not have it yet.
    ssh "$linux_host" "set -e; . ~/.cargo/env; cd teletext-server-check; rustup toolchain install
        cargo clippy --quiet --workspace --all-targets -- -D warnings && cargo test --quiet --workspace \\
            && cargo clippy --quiet --workspace --all-targets --no-default-features -- -D warnings"
    end_step
}

check_fuzz() {
    [[ -f fuzz/Cargo.toml ]] || {
        step "fuzzing: no fuzz workspace yet"
        return 0
    }
    step "fuzz: Clippy, then $fuzz_seconds seconds a target after its regressions"
    (
        cd fuzz
        cargo "+$FUZZ_NIGHTLY" clippy --quiet --all-targets -- -D warnings
        local target log
        for target in $(cargo "+$FUZZ_NIGHTLY" fuzz list); do
            mkdir -p "corpus/$target" "regressions/$target" target
            log="target/fuzz-$target.log"
            cargo "+$FUZZ_NIGHTLY" fuzz run "$target" "corpus/$target" "regressions/$target" -- \
                -max_total_time="$fuzz_seconds" -timeout=10 -rss_limit_mb=2048 >"$log" 2>&1 ||
                { tail -40 "$log"; fail "fuzz target $target found a fault; its input is in fuzz/artifacts/$target/"; }
            printf '%s: %s\n' "$target" "$(grep '^Done' "$log")"
        done
    )
}

# The commit pushed is the one checked: the pre-push hook passes the
# pushed commits, and the tree must be clean at HEAD.
clean_head() {
    local pushed
    for pushed in "$@"; do
        [[ "$pushed" == "$(git rev-parse HEAD)" ]] || fail "pushing $pushed, but HEAD is $(git rev-parse HEAD); check out what you push"
    done
    local loose
    loose="$(unstaged_under .; git diff --cached --name-only)"
    [[ -z "$loose" ]] || fail "the tree has changes that are not committed; the $mode gate checks the commit being pushed:
$loose"
    tree="$(git rev-parse 'HEAD^{tree}')"
    if passed "$mode" "$tree"; then
        printf '\n== the %s gate already passed on this tree\n' "$mode"
        exit 0
    fi
}

push_gate() {
    if passed commit "$tree"; then
        printf '\n== the commit gate passed on this tree: skipped\n'
    else
        check_workspace
    fi
    check_shapes_and_tests
}

full_gate() {
    local linux_log="$git_dir/gate-linux.log" linux_pid
    check_linux >"$linux_log" 2>&1 &
    linux_pid=$!
    printf '\n== the Linux steps on %s run alongside, logged in %s\n' "$linux_host" "$linux_log"

    if passed push "$tree"; then
        printf '\n== the push gate passed on this tree: skipped\n'
    else
        push_gate
    fi
    step "documentation tests"
    cargo test --quiet --workspace --doc
    step "documentation builds, warnings as errors"
    RUSTDOCFLAGS="-D warnings" cargo doc --quiet --workspace --no-deps
    step "release tests"
    cargo nextest run --workspace --release --hide-progress-bar --status-level fail --final-status-level fail --no-tests=warn
    step "tests with every optional feature compiled out"
    cargo nextest run --workspace --no-default-features --hide-progress-bar --status-level fail --final-status-level fail --no-tests=warn
    step "licences, advisories and sources"
    cargo deny check
    step "no unused dependencies"
    cargo machete
    check_fuzz

    step "waiting for the Linux steps on $linux_host"
    if ! wait "$linux_pid"; then
        cat "$linux_log"
        fail "the Linux steps on $linux_host failed"
    fi
    cat "$linux_log"

    local loose
    step "the gate changed nothing that is committed"
    loose="$(git status --porcelain --untracked-files=no)"
    [[ -z "$loose" ]] || fail "running the gate changed committed files, so what passed is not what is committed:
$loose"
}

tree=""
case "$mode" in
commit | push | full) announce "$mode" ;;
esac
gate_start=$SECONDS

case "$mode" in
commit)
    commit_gate
    [[ -z "$tree" ]] || record_pass commit "$tree"
    ;;
push)
    shift
    clean_head "$@"
    push_gate
    record_pass push "$tree"
    stamp commit "$tree"
    ;;
full)
    shift
    clean_head "$@"
    full_gate
    record_pass full "$tree"
    stamp push "$tree"
    stamp commit "$tree"
    ;;
*) fail "usage: scripts/check.sh commit | push | full [pushed commit]..." ;;
esac

step "All checks passed ($mode)"
